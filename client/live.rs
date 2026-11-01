//! Live queries: subscriptions with ids, delivered as one consistent batch per data version.
//!
//! Modeled on convex-rs `watch_all`: every version is one consistent map from subscription to
//! result. GV differs in two ways: `subscribe` answers synchronously (the data is local), and a
//! batch carries only the subscriptions whose result changed in that version.
//!
//! Per version, the store opens ONE read transaction, builds ONE in-memory `Forest` from it,
//! answers every subscribed query (forest-backed ones from the forest, SQL-backed ones in the same
//! transaction), then swaps the results in under one lock and emits one `LiveBatch`. Which backing
//! answers a query is declared per question in the client set (`crate::queries::ClientQuery`) and
//! is invisible both to this store and to the consumer.
//!
//! Every answer is assumed to be local and fast: a query that needed the network wouldn't be in
//! the snapshot and would hold up the whole version.
//!
//! First values are the one thing read outside a build, so they can be briefly out of step with
//! the version they're tagged with, in either direction. The rule, which needs no knowledge of the
//! backing: a first value never moves backwards, and a later build is already guaranteed to bring
//! it in line. See `subscribe` and `swap_in`.
//!
//! This sits beside `QueryStore` (the older ping-and-pull cache) during migration; both listen to
//! the same change broadcast.

use std::{
    collections::HashMap,
    future::Future,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex, MutexGuard},
};

use futures_util::FutureExt;
use gv_core::{
    error::{DbErr, DomainError, Result},
    forest::Forest,
    queries::AllEntries,
    query_executor::{ForestExecutor, QueryExecutor},
};
use gv_sql::sqlite::SqliteQueryExecutor;
use sqlx::{SqliteConnection, SqlitePool};
use tokio::sync::{Notify, broadcast, watch};
use tracing::warn;
#[cfg(test)]
use uuid::Uuid;

use crate::queries::{ClientQuery, ClientQueryResponse};
use crate::query_store::next_change;

pub type SubscriptionId = u64;
pub type Version = u64;
type BuildId = u64;

/// What a subscription holds: the query's response, or why there isn't one. An enum rather than
/// `Result` because uniffi can't carry a `Result` in a field.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveResult {
    Response(ClientQueryResponse),
    /// The query itself failed (e.g. a lookup of an entry that no longer exists). Delivered like
    /// a value so one failing query can't stall the version for everyone; consumers keep their
    /// last good value and surface this on a side channel.
    Failed {
        message: String,
    },
}

impl From<Result<ClientQueryResponse>> for LiveResult {
    fn from(result: Result<ClientQueryResponse>) -> Self {
        match result {
            Ok(response) => LiveResult::Response(response),
            Err(e) => LiveResult::Failed {
                message: e.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveUpdate {
    pub subscription_id: SubscriptionId,
    pub value: LiveResult,
}

/// Everything that changed in one version: the full new value of each subscription whose result
/// changed. A consumer applies batches in order on top of the first values `subscribe` returned.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveBatch {
    pub version: Version,
    pub updates: Vec<LiveUpdate>,
}

pub type BatchSink = Arc<dyn Fn(LiveBatch) + Send + Sync>;

/// Result of `subscribe`: the first value, the version it belongs to, and the handle that keeps
/// the subscription alive.
pub struct Subscribed {
    pub subscription_id: SubscriptionId,
    pub version: Version,
    pub value: LiveResult,
    pub handle: LiveSubscription,
}

async fn answer(query: ClientQuery, forest: &Forest, conn: &mut SqliteConnection) -> LiveResult {
    catching_panics(async {
        query
            .execute(
                &mut ForestExecutor::new(forest),
                &mut SqliteQueryExecutor::new(conn),
            )
            .await
    })
    .await
}

/// A question that panics (a broken forest invariant, say) fails like any other question: it
/// must not take the version, or the refresh loop, down with it.
async fn catching_panics(
    answering: impl Future<Output = Result<ClientQueryResponse>>,
) -> LiveResult {
    match AssertUnwindSafe(answering).catch_unwind().await {
        Ok(result) => result.into(),
        Err(panic) => LiveResult::Failed {
            message: panic_message(panic.as_ref()),
        },
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    let detail = panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("no message");
    format!("panicked: {detail}")
}

// --- Store -----------------------------------------------------------------------------------

struct Sub {
    id: SubscriptionId,
    refcount: usize,
    value: LiveResult,
    /// The build this subscription's first value was read during, if any. That build must not
    /// overwrite it (see `swap_in`).
    read_during: Option<BuildId>,
}

struct State {
    /// The latest published version. Every stored value belongs to it (see `subscribe` for the
    /// one transient exception).
    version: Version,
    /// The forest of the latest version; `None` until first needed.
    forest: Option<Arc<Forest>>,
    subs: HashMap<ClientQuery, Sub>,
    next_id: SubscriptionId,
    /// The build in flight.
    building: Option<BuildId>,
    next_build: BuildId,
}

/// How builds have turned out, for `sync`.
#[derive(Default)]
struct BuildLog {
    finished: u64,
    /// Why the latest finished build failed, if it did.
    last_error: Option<String>,
}

struct Inner {
    pool: SqlitePool,
    state: Mutex<State>,
    sink: Mutex<Option<BatchSink>>,
    /// Asks the refresh loop for another version without a data change (see `sync`).
    rebuild: Notify,
    builds: watch::Sender<BuildLog>,
    /// Lets a test act at a known point in a build (see `pause_build_seeing`).
    #[cfg(test)]
    build_pause: Mutex<Option<(Uuid, Pause)>>,
    /// Lets a test act between a first value's read and its insert.
    #[cfg(test)]
    subscribe_pause: Mutex<Option<Pause>>,
    #[cfg(test)]
    panic_next_build: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
struct Pause {
    reached: tokio::sync::oneshot::Sender<()>,
    resume: tokio::sync::oneshot::Receiver<()>,
}

#[cfg(test)]
impl Pause {
    fn new() -> (
        Self,
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (reached_tx, reached_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
        let pause = Pause {
            reached: reached_tx,
            resume: resume_rx,
        };
        (pause, reached_rx, resume_tx)
    }

    async fn wait(self) {
        let _ = self.reached.send(());
        let _ = self.resume.await;
    }
}

#[derive(Clone)]
pub struct LiveStore {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for LiveStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveStore").finish_non_exhaustive()
    }
}

impl LiveStore {
    /// Create the store and spawn its refresh loop. Must be called inside a tokio runtime.
    pub fn new(pool: SqlitePool, mut changes: broadcast::Receiver<()>) -> Self {
        let store = LiveStore {
            inner: Arc::new(Inner {
                pool,
                state: Mutex::new(State {
                    version: 0,
                    forest: None,
                    subs: HashMap::new(),
                    next_id: 1,
                    building: None,
                    next_build: 1,
                }),
                sink: Mutex::new(None),
                rebuild: Notify::new(),
                builds: watch::channel(BuildLog::default()).0,
                #[cfg(test)]
                build_pause: Mutex::new(None),
                #[cfg(test)]
                subscribe_pause: Mutex::new(None),
                #[cfg(test)]
                panic_next_build: std::sync::atomic::AtomicBool::new(false),
            }),
        };
        let loop_store = store.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Lagged reads as "changed": a burst of writes coalesces into one version.
                    alive = next_change(&mut changes) => if !alive { break },
                    _ = loop_store.inner.rebuild.notified() => {}
                }
                loop_store.publish_next_version().await;
            }
        });
        store
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap()
    }

    /// Where batches go. Called in version order, one call per version that changed anything a
    /// subscriber holds, and inside the step that publishes the version: no `subscribe` can report
    /// a version before its batch has reached the sink. So the sink must only enqueue. It must not
    /// block or call back into this store.
    pub fn set_batch_sink(&self, sink: BatchSink) {
        *self.inner.sink.lock().unwrap() = Some(sink);
    }

    pub fn version(&self) -> Version {
        self.lock().version
    }

    pub fn subscription_count(&self) -> usize {
        self.lock().subs.len()
    }

    /// Load the forest now, so the first forest-backed `subscribe` doesn't. Meant to be spawned at
    /// startup, off the thread that will subscribe.
    pub async fn warm(&self) -> Result<()> {
        self.latest_forest().await.map(|_| ())
    }

    /// Wait for a build that started after this call to finish, so a version it publishes reflects
    /// every write committed before the call (read-your-writes for forest-backed answers). Fails
    /// if that build failed.
    ///
    /// Not called on the write path today: `run_action` returns at commit and versions trail it.
    /// Calling this from `run_action` would give read-your-writes at the cost of making every
    /// write wait for a full rebuild.
    pub async fn sync(&self) -> Result<()> {
        let target = {
            let st = self.lock();
            // A build in flight may have read the database before the caller's write.
            self.inner.builds.borrow().finished + if st.building.is_some() { 2 } else { 1 }
        };
        self.inner.rebuild.notify_one();
        let mut rx = self.inner.builds.subscribe();
        let log = rx
            .wait_for(|log| log.finished >= target)
            .await
            .map_err(|e| DomainError::Database(Box::new(e)))?;
        match &log.last_error {
            Some(error) => Err(DomainError::Database(error.clone().into())),
            None => Ok(()),
        }
    }

    /// Subscribe to a query and get its first value now.
    ///
    /// Identical queries share one subscription id (refcounted, like `QueryStore`); the id is
    /// never reused after the last handle drops.
    ///
    /// Coherence: the first value is inserted only if it can be placed cleanly against builds,
    /// otherwise it's read again. Either no build is in flight, so every later build's snapshot is
    /// newer than this read; or the whole read happened during the build in flight, which then
    /// leaves it alone (`swap_in`). A read that straddled a build's start would fit neither: it
    /// might be older than the snapshot, or newer.
    ///
    /// So a first value can be briefly out of step with the returned version. Read "now" (SQL), it
    /// can be ahead; read from the published forest, it can be behind a build in flight. It never
    /// moves backwards, and it converges on the next build.
    pub async fn subscribe(&self, query: ClientQuery) -> Result<Subscribed> {
        {
            let mut st = self.lock();
            if let Some(subscribed) = self.join_existing(&mut st, &query) {
                return Ok(subscribed);
            }
        }

        let mut conn = self.inner.pool.acquire().await.db_err()?;
        loop {
            let read_during = self.lock().building;
            let (forest, seen_version) = self.latest_forest().await?;
            let value = answer(query.clone(), &forest, &mut conn).await;
            #[cfg(test)]
            self.wait_at_subscribe_pause().await;

            let mut st = self.lock();
            if let Some(subscribed) = self.join_existing(&mut st, &query) {
                return Ok(subscribed);
            }
            let straddled_a_build = st.building.is_some() && st.building != read_during;
            if st.version != seen_version || straddled_a_build {
                continue;
            }
            let id = st.next_id;
            st.next_id += 1;
            st.subs.insert(
                query.clone(),
                Sub {
                    id,
                    refcount: 1,
                    value: value.clone(),
                    read_during,
                },
            );
            return Ok(Subscribed {
                subscription_id: id,
                version: st.version,
                value,
                handle: LiveSubscription {
                    inner: Arc::clone(&self.inner),
                    query,
                    id,
                },
            });
        }
    }

    fn join_existing(&self, st: &mut State, query: &ClientQuery) -> Option<Subscribed> {
        let sub = st.subs.get_mut(query)?;
        sub.refcount += 1;
        Some(Subscribed {
            subscription_id: sub.id,
            version: st.version,
            value: sub.value.clone(),
            handle: LiveSubscription {
                inner: Arc::clone(&self.inner),
                query: query.clone(),
                id: sub.id,
            },
        })
    }

    /// Answer a query once against the latest version, without subscribing. Forest-backed
    /// queries use the latest published forest; SQL-backed ones read the database now.
    pub async fn read(&self, query: ClientQuery) -> Result<ClientQueryResponse> {
        let (forest, _) = self.latest_forest().await?;
        let mut conn = self.inner.pool.acquire().await.db_err()?;
        query
            .execute(
                &mut ForestExecutor::new(&forest),
                &mut SqliteQueryExecutor::new(&mut conn),
            )
            .await
    }

    /// The latest published forest and the version it belongs to, loading it if nothing has been
    /// published yet. A forest loaded while a build is in flight is used but not published: it may
    /// be newer than that build, and the build's result must win.
    async fn latest_forest(&self) -> Result<(Arc<Forest>, Version)> {
        {
            let st = self.lock();
            if let Some(forest) = &st.forest {
                return Ok((Arc::clone(forest), st.version));
            }
        }
        let mut conn = self.inner.pool.acquire().await.db_err()?;
        let entries = SqliteQueryExecutor::new(&mut conn)
            .execute(AllEntries)
            .await?;
        let forest = Arc::new(Forest::from(entries));
        let mut st = self.lock();
        if st.building.is_some() {
            return Ok((forest, st.version));
        }
        let forest = Arc::clone(st.forest.get_or_insert(forest));
        Ok((forest, st.version))
    }

    /// Build the next version from one read transaction and publish what changed. A failed or
    /// panicking build publishes nothing; the next change (or `sync`) tries again.
    async fn publish_next_version(&self) {
        let build = {
            let mut st = self.lock();
            let build = st.next_build;
            st.next_build += 1;
            st.building = Some(build);
            build
        };
        let error = match AssertUnwindSafe(self.build_and_swap(build))
            .catch_unwind()
            .await
        {
            Ok(Ok(())) => return,
            Ok(Err(e)) => e.to_string(),
            Err(panic) => panic_message(panic.as_ref()),
        };
        warn!(%error, "live: building a version failed; retrying on next change");
        let mut st = self.lock();
        st.building = None;
        self.inner.builds.send_modify(|log| {
            log.finished += 1;
            log.last_error = Some(error);
        });
    }

    /// Answer every subscription in one read transaction, so the forest and every SQL-backed
    /// answer see the same snapshot, then swap the results in. A subscription added while this
    /// runs is answered too: the swap happens only under a lock that finds nothing left to answer.
    async fn build_and_swap(&self, build: BuildId) -> Result<()> {
        let mut tx = self.inner.pool.begin().await.db_err()?;
        let entries = SqliteQueryExecutor::new(&mut *tx)
            .execute(AllEntries)
            .await?;
        let forest = Arc::new(Forest::from(entries));
        #[cfg(test)]
        self.wait_at_build_pause(&forest).await;
        let mut values: HashMap<ClientQuery, LiveResult> = HashMap::new();
        loop {
            let missing: Vec<ClientQuery> = {
                let mut st = self.lock();
                let missing: Vec<ClientQuery> = st
                    .subs
                    .keys()
                    .filter(|query| !values.contains_key(*query))
                    .cloned()
                    .collect();
                if missing.is_empty() {
                    self.swap_in(&mut st, build, forest, values);
                    break;
                }
                missing
            };
            for query in missing {
                let value = answer(query.clone(), &forest, &mut *tx).await;
                values.insert(query, value);
            }
        }
        // Read-only, and the version is already published: nothing here can undo it.
        if let Err(e) = tx.rollback().await {
            warn!(error = %e, "live: ending a build's read transaction failed");
        }
        Ok(())
    }

    /// Publish a built version and hand its batch to the sink, in one step. Marks the build
    /// finished in that step too: `sync` reads the two together.
    ///
    /// A first value read during this build is left alone, because this build can't tell which is
    /// newer: a SQL read may have seen a write after this build's snapshot, while a forest read
    /// may be behind it. If they disagree it asks for one more build, whose snapshot is newer than
    /// every such first value. Asking is sometimes redundant (the write behind a newer SQL read
    /// has its own notification on the way) but it's only ever one build.
    fn swap_in(
        &self,
        st: &mut State,
        build: BuildId,
        forest: Arc<Forest>,
        values: HashMap<ClientQuery, LiveResult>,
    ) {
        st.building = None;
        st.version += 1;
        st.forest = Some(forest);
        let mut updates = Vec::new();
        let mut reconcile = false;
        for (query, value) in values {
            // Skip keys whose last subscriber dropped while building.
            let Some(sub) = st.subs.get_mut(&query) else {
                continue;
            };
            if sub.value == value {
                continue;
            }
            if sub.read_during == Some(build) {
                reconcile = true;
                continue;
            }
            sub.value = value.clone();
            updates.push(LiveUpdate {
                subscription_id: sub.id,
                value,
            });
        }
        if reconcile {
            self.inner.rebuild.notify_one();
        }
        self.inner.builds.send_modify(|log| {
            log.finished += 1;
            log.last_error = None;
        });
        if !updates.is_empty() {
            if let Some(sink) = self.inner.sink.lock().unwrap().as_ref() {
                sink(LiveBatch {
                    version: st.version,
                    updates,
                });
            }
        }
    }

    /// Pause the first build that can see `entry_id`, right after it has taken its snapshot and
    /// before it answers anything. Resolves `reached` when it gets there; it continues when
    /// `resume` is sent to.
    #[cfg(test)]
    fn pause_build_seeing(
        &self,
        entry_id: Uuid,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (pause, reached, resume) = Pause::new();
        *self.inner.build_pause.lock().unwrap() = Some((entry_id, pause));
        (reached, resume)
    }

    #[cfg(test)]
    async fn wait_at_build_pause(&self, forest: &Forest) {
        use std::sync::atomic::Ordering;
        if self.inner.panic_next_build.swap(false, Ordering::SeqCst) {
            panic!("test build panic");
        }
        let pause = {
            let mut pause = self.inner.build_pause.lock().unwrap();
            match &*pause {
                Some((entry_id, _)) if forest.entry(*entry_id).is_some() => pause.take(),
                _ => None,
            }
        };
        if let Some((_, pause)) = pause {
            pause.wait().await;
        }
    }

    /// Pause the next `subscribe` between reading its first value and inserting it.
    #[cfg(test)]
    fn pause_next_subscribe(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (pause, reached, resume) = Pause::new();
        *self.inner.subscribe_pause.lock().unwrap() = Some(pause);
        (reached, resume)
    }

    #[cfg(test)]
    async fn wait_at_subscribe_pause(&self) {
        let pause = self.inner.subscribe_pause.lock().unwrap().take();
        if let Some(pause) = pause {
            pause.wait().await;
        }
    }
}

/// Keeps a subscription alive. Dropping it releases one reference; the subscription (and its
/// id) is removed when the last handle for that query drops.
pub struct LiveSubscription {
    inner: Arc<Inner>,
    query: ClientQuery,
    id: SubscriptionId,
}

impl Drop for LiveSubscription {
    fn drop(&mut self) {
        if let Ok(mut st) = self.inner.state.lock() {
            if let Some(sub) = st.subs.get_mut(&self.query) {
                if sub.id != self.id {
                    return;
                }
                sub.refcount -= 1;
                if sub.refcount == 0 {
                    st.subs.remove(&self.query);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::SqliteClient;
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use gv_core::{
        DEFAULT_USER_ID,
        actions::{
            AttachValue, CreateAttribute, CreateEntry, CreateUser, MoveEntry, UpdateEntryCompletion,
        },
        io::SystemIo,
        models::{
            activity::{Activity, ActivityName},
            entry::{Entry, Position, Temporal},
            user::User,
        },
        queries::{
            ActivityTemplateJoin, FindAncestors, FindDescendants, FindEntryJoinById,
            RootEntriesInInterval,
        },
        std_lib::StandardLibrary,
    };
    use gv_ui::{ChildSlot, ChildSlots, SuggestedRootInsertionTime, WouldCreateCycle, child_slots};
    use std::collections::HashSet;
    use tokio::sync::{mpsc, oneshot};

    fn day() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 30, 0, 0, 0).unwrap()
    }

    fn entry(position: Option<Position>, temporal: Temporal, is_sequence: bool) -> Entry {
        Entry {
            id: Uuid::new_v4(),
            activity_id: None,
            owner_id: DEFAULT_USER_ID,
            name: None,
            position,
            is_template: false,
            display_as_sets: false,
            is_sequence,
            is_complete: false,
            temporal,
        }
    }

    fn at(hours: i64) -> Temporal {
        Temporal::Start {
            start: day() + Duration::hours(hours),
        }
    }

    async fn client(pool: SqlitePool) -> (SqliteClient, mpsc::UnboundedReceiver<LiveBatch>) {
        let client = SqliteClient::from_pool(pool, Arc::new(SystemIo::default()));
        client
            .run_action(CreateUser::from(User::default_user()).into())
            .await
            .unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        client.live().set_batch_sink(Arc::new(move |batch| {
            let _ = tx.send(batch);
        }));
        (client, rx)
    }

    async fn create(client: &SqliteClient, entry: &Entry) {
        client
            .run_action(CreateEntry::from(entry.clone()).into())
            .await
            .unwrap();
    }

    async fn next_batch(rx: &mut mpsc::UnboundedReceiver<LiveBatch>) -> LiveBatch {
        tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out waiting for a batch")
            .expect("sink dropped")
    }

    /// A file database opened the way the app opens one.
    async fn file_pool() -> SqlitePool {
        let path = std::env::temp_dir().join(format!("gv-live-{}.sqlite", Uuid::new_v4()));
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(20)
            .connect_with(crate::client::connect_options(&url).unwrap())
            .await
            .unwrap();
        sqlx::migrate!("../gv-sql/sqlite/migrations")
            .run(&pool)
            .await
            .unwrap();
        pool
    }

    /// A store that wakes only when told, so a test controls exactly which builds run, and a
    /// client to write with that doesn't wake it.
    async fn quiet_store(pool: SqlitePool) -> (LiveStore, broadcast::Sender<()>, SqliteClient) {
        let (changes, rx) = broadcast::channel(16);
        let store = LiveStore::new(pool.clone(), rx);
        let writer = SqliteClient::from_pool(pool, Arc::new(SystemIo::default()));
        writer
            .run_action(CreateUser::from(User::default_user()).into())
            .await
            .unwrap();
        (store, changes, writer)
    }

    async fn reach(reached: oneshot::Receiver<()>) {
        tokio::time::timeout(std::time::Duration::from_secs(5), reached)
            .await
            .expect("timed out waiting for the pause point")
            .unwrap();
    }

    fn find_entry(entry_id: Uuid) -> ClientQuery {
        ClientQuery::FindEntryById(gv_core::queries::FindEntryById { entry_id })
    }

    fn found_id(value: &LiveResult) -> Option<Uuid> {
        match value {
            LiveResult::Response(ClientQueryResponse::FindEntryById(found)) => {
                found.as_ref().map(|e| e.id)
            }
            other => panic!("unexpected value {other:?}"),
        }
    }

    fn day_roots() -> ClientQuery {
        ClientQuery::RootEntriesInInterval(RootEntriesInInterval {
            from: day(),
            to: day() + Duration::days(1),
        })
    }

    fn child_slots_of(parent_id: Uuid) -> ClientQuery {
        ClientQuery::ChildSlots(ChildSlots { parent_id })
    }

    fn slots(value: &LiveResult) -> &[ChildSlot] {
        match value {
            LiveResult::Response(ClientQueryResponse::ChildSlots(slots)) => slots,
            other => panic!("expected child slots, got {other:?}"),
        }
    }

    fn root_ids(value: &LiveResult) -> Vec<Uuid> {
        match value {
            LiveResult::Response(ClientQueryResponse::RootEntriesInInterval(es)) => {
                es.iter().map(|e| e.id).collect()
            }
            other => panic!("unexpected value {other:?}"),
        }
    }

    fn first_drop_position(value: &LiveResult) -> Position {
        match &slots(value)[0] {
            ChildSlot::Drop { position, .. } => position.clone(),
            other => panic!("expected a drop slot, got {other:?}"),
        }
    }

    /// The core contract: `subscribe` answers immediately; each change yields one batch, at a
    /// newer version, holding exactly the subscriptions whose result changed — across both
    /// backings (forest and SQL) in the same batch.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn batches_carry_only_changed_subscriptions(pool: SqlitePool) {
        let (client, mut rx) = client(pool).await;
        let store = client.live();
        let parent = entry(None, at(9), true);
        let other = entry(None, at(8), false);
        create(&client, &parent).await;
        create(&client, &other).await;
        store.sync().await.unwrap();

        // First values, synchronously. Forest-backed:
        let slots_sub = store.subscribe(child_slots_of(parent.id)).await.unwrap();
        assert_eq!(
            slots(&slots_sub.value).len(),
            1,
            "empty sequence: one drop slot"
        );
        let position = first_drop_position(&slots_sub.value);
        // SQL-backed:
        let roots = store.subscribe(day_roots()).await.unwrap();
        assert_eq!(root_ids(&roots.value), vec![other.id, parent.id]);
        let descendants = store
            .subscribe(ClientQuery::FindDescendants(FindDescendants {
                entry_id: parent.id,
            }))
            .await
            .unwrap();
        let _unrelated = store
            .subscribe(ClientQuery::FindEntryJoinById(FindEntryJoinById {
                entry_id: other.id,
            }))
            .await
            .unwrap();

        // Drain any batches from versions racing the subscribes.
        store.sync().await.unwrap();
        while rx.try_recv().is_ok() {}
        let before = store.version();

        // Add a child at the drop slot's position: changes the slots (forest) and descendants
        // (SQL), not the day's roots or the unrelated entry.
        let child = entry(Some(position), Temporal::None, false);
        create(&client, &child).await;
        let batch = next_batch(&mut rx).await;
        assert!(batch.version > before);
        let changed: HashSet<_> = batch.updates.iter().map(|u| u.subscription_id).collect();
        assert_eq!(
            changed,
            HashSet::from([slots_sub.subscription_id, descendants.subscription_id])
        );
        let slots_value = &batch
            .updates
            .iter()
            .find(|u| u.subscription_id == slots_sub.subscription_id)
            .unwrap()
            .value;
        let after = slots(slots_value);
        assert_eq!(after.len(), 3);
        assert!(matches!(&after[1], ChildSlot::Entry { entry } if entry.id == child.id));

        // Move the parent to the next day: the day's roots change, and so do its descendants
        // (FindDescendants includes the entry itself); its child slots don't.
        client
            .run_action(
                MoveEntry {
                    actor_id: DEFAULT_USER_ID,
                    entry_id: parent.id,
                    position: None,
                    temporal: at(25),
                }
                .into(),
            )
            .await
            .unwrap();
        let batch = next_batch(&mut rx).await;
        let changed: HashSet<_> = batch.updates.iter().map(|u| u.subscription_id).collect();
        assert_eq!(
            changed,
            HashSet::from([roots.subscription_id, descendants.subscription_id])
        );
        let roots_value = &batch
            .updates
            .iter()
            .find(|u| u.subscription_id == roots.subscription_id)
            .unwrap()
            .value;
        assert_eq!(root_ids(roots_value), vec![other.id]);
    }

    /// Identical queries share an id (refcounted); dropping the last handle unsubscribes, after
    /// which changes produce no batch for it.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn handles_share_ids_and_drop_unsubscribes(pool: SqlitePool) {
        let (client, mut rx) = client(pool).await;
        let store = client.live();
        let a = store.subscribe(day_roots()).await.unwrap();
        let b = store.subscribe(day_roots()).await.unwrap();
        assert_eq!(a.subscription_id, b.subscription_id);
        assert_eq!(store.subscription_count(), 1);

        drop(b.handle);
        assert_eq!(store.subscription_count(), 1, "one handle still alive");
        create(&client, &entry(None, at(1), false)).await;
        let batch = next_batch(&mut rx).await;
        assert_eq!(batch.updates[0].subscription_id, a.subscription_id);

        drop(a.handle);
        assert_eq!(store.subscription_count(), 0);
        let before = store.version();
        create(&client, &entry(None, at(2), false)).await;
        store.sync().await.unwrap();
        assert!(store.version() > before);
        assert!(rx.try_recv().is_err(), "no subscribers, no batch");

        // Re-subscribing gets a fresh id, so a stale route can never match.
        let c = store.subscribe(day_roots()).await.unwrap();
        assert_ne!(c.subscription_id, a.subscription_id);
    }

    /// A forest-backed first value read during a build can be behind that build. It isn't
    /// overwritten mid-flight; the disagreement asks for one more build, which brings it in line.
    #[tokio::test]
    async fn forest_first_value_behind_a_build_catches_up_on_the_next() {
        let (client, mut rx) = client(file_pool().await).await;
        let store = client.live();
        let parent = entry(None, at(9), true);
        create(&client, &parent).await;
        store.sync().await.unwrap();
        let existing = store.subscribe(child_slots_of(parent.id)).await.unwrap();
        store.sync().await.unwrap();

        // Hold the build for this write open, after it took its snapshot.
        let child = entry(
            Some(first_drop_position(&existing.value)),
            Temporal::None,
            true,
        );
        let (reached, resume) = store.pause_build_seeing(child.id);
        create(&client, &child).await;
        reach(reached).await;
        // Nothing can publish while the refresh task is paused.
        let before = store.version();
        while rx.try_recv().is_ok() {}

        // Answered from the published forest, which doesn't have the child yet.
        let cycle = store
            .subscribe(ClientQuery::WouldCreateCycle(WouldCreateCycle {
                entry_id: parent.id,
                proposed_parent_id: child.id,
            }))
            .await
            .unwrap();
        assert_eq!(cycle.version, before);
        assert_eq!(
            cycle.value,
            LiveResult::Response(ClientQueryResponse::WouldCreateCycle(false))
        );

        resume.send(()).unwrap();
        let batch = next_batch(&mut rx).await;
        assert_eq!(batch.version, before + 1);
        let changed: HashSet<_> = batch.updates.iter().map(|u| u.subscription_id).collect();
        assert_eq!(changed, HashSet::from([existing.subscription_id]));

        let batch = next_batch(&mut rx).await;
        assert_eq!(batch.version, before + 2);
        assert_eq!(batch.updates.len(), 1);
        assert_eq!(batch.updates[0].subscription_id, cycle.subscription_id);
        assert_eq!(
            batch.updates[0].value,
            LiveResult::Response(ClientQueryResponse::WouldCreateCycle(true))
        );
    }

    /// A SQL-backed first value read during a build can be ahead of that build's snapshot. Neither
    /// that build nor the next may move it backwards. (Review finding 2.)
    #[tokio::test]
    async fn sql_first_value_ahead_of_a_build_never_moves_backwards() {
        let (client, mut rx) = client(file_pool().await).await;
        let store = client.live();
        store.sync().await.unwrap();
        let _keep = store.subscribe(day_roots()).await.unwrap();
        let a = entry(None, at(1), false);
        let w = entry(None, at(2), false);
        let (reached, resume) = store.pause_build_seeing(a.id);
        create(&client, &a).await;
        reach(reached).await; // this build's snapshot has A, not W
        create(&client, &w).await;

        let before = store.version();
        let sub = store.subscribe(find_entry(w.id)).await.unwrap();
        assert_eq!(sub.version, before);
        assert_eq!(found_id(&sub.value), Some(w.id));
        while rx.try_recv().is_ok() {}

        resume.send(()).unwrap();
        store.sync().await.unwrap();
        store.sync().await.unwrap();
        assert!(store.version() >= before + 2, "W's own build has run");
        while let Ok(batch) = rx.try_recv() {
            assert!(
                batch
                    .updates
                    .iter()
                    .all(|u| u.subscription_id != sub.subscription_id),
                "the first value already had W: nothing should touch it"
            );
        }
        let again = store.subscribe(find_entry(w.id)).await.unwrap();
        assert_eq!(found_id(&again.value), Some(w.id));
    }

    /// A first value read before a build started and inserted during it fits neither case: it
    /// could miss a write that build already consumed the notification for, leaving it stale with
    /// no follow-up. So it's read again.
    #[tokio::test]
    async fn first_value_straddling_a_build_start_is_read_again() {
        let (store, changes, writer) = quiet_store(file_pool().await).await;
        let w = entry(None, at(2), false);

        let (read_done, finish_subscribe) = store.pause_next_subscribe();
        let subscribing = tokio::spawn({
            let store = store.clone();
            let query = find_entry(w.id);
            async move { store.subscribe(query).await.unwrap().value }
        });
        reach(read_done).await; // first read done, before W exists

        create(&writer, &w).await;
        let (reached, resume) = store.pause_build_seeing(w.id);
        changes.send(()).unwrap();
        reach(reached).await; // W's build has consumed W's notification
        finish_subscribe.send(()).unwrap();

        let value = subscribing.await.unwrap();
        assert_eq!(found_id(&value), Some(w.id));
        resume.send(()).unwrap();
    }

    /// A forest loaded while a build is in flight may be newer than that build, so it isn't
    /// published: it would be replaced by an older forest at the next version.
    #[tokio::test]
    async fn forest_loaded_during_a_build_is_not_published() {
        let (store, changes, writer) = quiet_store(file_pool().await).await;
        let a = entry(None, at(1), true);
        create(&writer, &a).await;

        let (reached, resume) = store.pause_build_seeing(a.id);
        changes.send(()).unwrap();
        reach(reached).await;

        let sub = store.subscribe(child_slots_of(a.id)).await.unwrap();
        assert_eq!(
            slots(&sub.value).len(),
            1,
            "answered from the loaded forest"
        );
        assert!(store.lock().forest.is_none());

        resume.send(()).unwrap();
        store.sync().await.unwrap();
        assert!(store.lock().forest.is_some());
    }

    /// WAL: a write doesn't wait for a build's read transaction. (Review finding 1.)
    #[tokio::test]
    async fn writes_do_not_wait_for_a_build() {
        let (client, _rx) = client(file_pool().await).await;
        let store = client.live();
        store.sync().await.unwrap();
        let a = entry(None, at(1), false);
        let (reached, resume) = store.pause_build_seeing(a.id);
        create(&client, &a).await;
        reach(reached).await;
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            create(&client, &entry(None, at(2), false)),
        )
        .await
        .expect("a write should not block on the build's read transaction");
        resume.send(()).unwrap();
    }

    /// Writers that read before they write queue up rather than failing with "database is locked",
    /// which is what deferred transactions do under WAL when they interleave.
    #[tokio::test]
    async fn concurrent_writers_all_succeed() {
        let (client, _rx) = client(file_pool().await).await;
        let target = entry(None, at(0), false);
        create(&client, &target).await;
        let mut writers = Vec::new();
        for i in 0..128 {
            let client = client.clone();
            let entry_id = target.id;
            writers.push(tokio::spawn(async move {
                // Reads the entry to authorize the change, then writes.
                let action = UpdateEntryCompletion {
                    actor_id: DEFAULT_USER_ID,
                    entry_id,
                    is_complete: i % 2 == 0,
                };
                client.run_action(action.into()).await
            }));
        }
        for writer in writers {
            writer.await.unwrap().unwrap();
        }
    }

    #[tokio::test]
    async fn init_opens_the_database_in_wal_with_full_sync() {
        let path = std::env::temp_dir().join(format!("gv-live-{}.sqlite", Uuid::new_v4()));
        let client = SqliteClient::init(&format!("sqlite://{}?mode=rwc", path.display()))
            .await
            .unwrap();
        let mut conn = client.pool.acquire().await.unwrap();
        let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(journal, "wal");
        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(synchronous, 2, "FULL");
    }

    /// The batch reaches the sink inside the step that publishes its version, so a `subscribe`
    /// that reports version v knows batch v is already queued. (Review finding 4.)
    #[tokio::test]
    async fn batch_reaches_the_sink_before_its_version_is_visible() {
        let (client, _rx) = client(file_pool().await).await;
        let store = client.live();
        let inner = Arc::downgrade(&store.inner);
        let (held, mut held_rx) = mpsc::unbounded_channel();
        store.set_batch_sink(Arc::new(move |_| {
            let inner = inner.upgrade().unwrap();
            let _ = held.send(inner.state.try_lock().is_err());
        }));
        let _keep = store.subscribe(day_roots()).await.unwrap();
        create(&client, &entry(None, at(1), false)).await;
        let state_was_locked =
            tokio::time::timeout(std::time::Duration::from_secs(5), held_rx.recv())
                .await
                .unwrap()
                .unwrap();
        assert!(state_was_locked);
    }

    /// `sync` reports a failed build instead of waiting for a later success. (Review finding 3.)
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn sync_fails_when_the_build_fails(pool: SqlitePool) {
        let (client, _rx) = client(pool.clone()).await;
        let store = client.live();
        store.sync().await.unwrap();
        sqlx::query("ALTER TABLE entries RENAME TO entries_gone")
            .execute(&pool)
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), store.sync())
            .await
            .expect("sync should finish")
            .expect_err("the build failed");
        assert!(store.lock().building.is_none());
        sqlx::query("ALTER TABLE entries_gone RENAME TO entries")
            .execute(&pool)
            .await
            .unwrap();
        store.sync().await.unwrap();
    }

    /// A panic fails one answer, not the version. (Review finding 6.)
    #[tokio::test]
    async fn panicking_answer_becomes_failed() {
        let value = catching_panics(async { panic!("broken invariant") }).await;
        assert_eq!(
            value,
            LiveResult::Failed {
                message: "panicked: broken invariant".to_string()
            }
        );
    }

    /// A panic in a build doesn't end the refresh loop. (Review finding 6.)
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn refresh_loop_survives_a_panicking_build(pool: SqlitePool) {
        let (store, changes, writer) = quiet_store(pool).await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        store.set_batch_sink(Arc::new(move |batch| {
            let _ = tx.send(batch);
        }));
        let roots = store.subscribe(day_roots()).await.unwrap();
        store
            .inner
            .panic_next_build
            .store(true, std::sync::atomic::Ordering::SeqCst);
        tokio::time::timeout(std::time::Duration::from_secs(5), store.sync())
            .await
            .unwrap()
            .expect_err("the build panicked");
        assert!(store.lock().building.is_none());

        create(&writer, &entry(None, at(1), false)).await;
        changes.send(()).unwrap();
        let batch = next_batch(&mut rx).await;
        assert_eq!(batch.updates[0].subscription_id, roots.subscription_id);
    }

    /// `ActivityTemplateJoin` follows the template root's attributes live.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn activity_template_join_follows_an_attach(pool: SqlitePool) {
        let (client, mut rx) = client(pool).await;
        let store = client.live();
        let attribute = StandardLibrary::attributes().remove(0);
        client
            .run_action(CreateAttribute::from(attribute.clone()).into())
            .await
            .unwrap();
        let activity = Activity {
            id: Uuid::new_v4(),
            owner_id: DEFAULT_USER_ID,
            name: ActivityName::parse("Squat".to_string()).unwrap(),
            description: None,
            source_activity_id: None,
        };
        let root_id = Uuid::new_v4();
        client
            .run_action(activity.into_create_activity(root_id).into())
            .await
            .unwrap();
        store.sync().await.unwrap();

        let join = store
            .subscribe(ClientQuery::ActivityTemplateJoin(ActivityTemplateJoin {
                activity_id: activity.id,
            }))
            .await
            .unwrap();
        let LiveResult::Response(ClientQueryResponse::ActivityTemplateJoin(Some(first))) =
            &join.value
        else {
            panic!("expected a join, got {:?}", join.value)
        };
        assert_eq!(first.entry.id, root_id);
        assert!(first.attributes.is_empty());
        store.sync().await.unwrap();
        while rx.try_recv().is_ok() {}

        client
            .run_action(
                AttachValue {
                    actor_id: DEFAULT_USER_ID,
                    entry_id: root_id,
                    attribute_id: attribute.id,
                }
                .into(),
            )
            .await
            .unwrap();
        let batch = next_batch(&mut rx).await;
        let LiveResult::Response(ClientQueryResponse::ActivityTemplateJoin(Some(after))) =
            &batch.updates[0].value
        else {
            panic!("expected a join")
        };
        assert_eq!(after.attributes.len(), 1);

        let missing = store
            .read(ClientQuery::ActivityTemplateJoin(ActivityTemplateJoin {
                activity_id: Uuid::new_v4(),
            }))
            .await
            .unwrap();
        assert_eq!(missing, ClientQueryResponse::ActivityTemplateJoin(None));
    }

    /// `read` answers once against the latest version, from whichever backing the query has.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn read_once_uses_latest_version(pool: SqlitePool) {
        let (client, _rx) = client(pool).await;
        let store = client.live();
        let parent = entry(None, at(0), true);
        create(&client, &parent).await;
        // Forest-backed answers come from the latest *published* version, which trails a commit
        // until the refresh task builds it.
        store.sync().await.unwrap();
        let slots_sub = store.subscribe(child_slots_of(parent.id)).await.unwrap();
        let child = entry(
            Some(first_drop_position(&slots_sub.value)),
            Temporal::None,
            true,
        );
        create(&client, &child).await;
        store.sync().await.unwrap();

        let cycle = |entry_id, proposed_parent_id| {
            ClientQuery::WouldCreateCycle(WouldCreateCycle {
                entry_id,
                proposed_parent_id,
            })
        };
        assert_eq!(
            store.read(cycle(parent.id, child.id)).await.unwrap(),
            ClientQueryResponse::WouldCreateCycle(true)
        );
        assert_eq!(
            store.read(cycle(child.id, parent.id)).await.unwrap(),
            ClientQueryResponse::WouldCreateCycle(false)
        );
        assert_eq!(store.subscription_count(), 1, "read does not subscribe");
    }

    /// `SuggestedRootInsertionTime` (the spike's measured "new question"): forest-backed, read
    /// once. On a past day it lands one minute after the day's last root.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn suggested_root_insertion_time_reads_from_the_forest(pool: SqlitePool) {
        let (client, _rx) = client(pool).await;
        let store = client.live();
        let past_day = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        let suggested = || async {
            match store
                .read(ClientQuery::SuggestedRootInsertionTime(
                    SuggestedRootInsertionTime {
                        day_start: past_day,
                    },
                ))
                .await
                .unwrap()
            {
                ClientQueryResponse::SuggestedRootInsertionTime(t) => t,
                other => panic!("unexpected value {other:?}"),
            }
        };
        assert_eq!(
            suggested().await,
            past_day + Duration::hours(12),
            "empty day: noon"
        );
        let mut root = entry(None, at(0), false);
        root.temporal = Temporal::Start {
            start: past_day + Duration::hours(9),
        };
        create(&client, &root).await;
        store.sync().await.unwrap();
        assert_eq!(
            suggested().await,
            past_day + Duration::hours(9) + Duration::minutes(1)
        );
    }

    /// A query that errors is delivered as `Failed` instead of stalling the version for others.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn failing_query_does_not_stall_others(pool: SqlitePool) {
        let (client, mut rx) = client(pool).await;
        let store = client.live();
        // FindAncestors errors (NotFound) for a missing entry.
        let failing = store
            .subscribe(ClientQuery::FindAncestors(FindAncestors {
                entry_id: Uuid::new_v4(),
            }))
            .await
            .unwrap();
        assert!(matches!(failing.value, LiveResult::Failed { .. }));
        let roots = store.subscribe(day_roots()).await.unwrap();
        create(&client, &entry(None, at(3), false)).await;
        let batch = next_batch(&mut rx).await;
        assert_eq!(batch.updates.len(), 1);
        assert_eq!(batch.updates[0].subscription_id, roots.subscription_id);
    }

    /// The SQL-backed day-roots query agrees with the forest's `roots_in` on real SQLite,
    /// including end-anchored entries, children, and the half-open upper bound.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn sql_day_roots_match_forest_roots_in(pool: SqlitePool) {
        let (client, _rx) = client(pool).await;
        let next = day() + Duration::days(1);
        let parent = entry(None, at(9), true);
        let cases = vec![
            parent.clone(),
            entry(None, at(0), false),
            entry(None, at(24), false), // next midnight: excluded
            entry(
                None,
                Temporal::Start {
                    start: day() - Duration::milliseconds(1),
                },
                false,
            ),
            entry(
                None,
                Temporal::End {
                    end: day() + Duration::hours(7),
                },
                false,
            ),
            entry(
                None,
                Temporal::DurationAndEnd {
                    duration_ms: 2 * 3_600_000,
                    end: next + Duration::hours(1),
                },
                false,
            ),
            entry(
                None,
                Temporal::StartAndEnd {
                    start: day() + Duration::hours(23),
                    end: next + Duration::hours(2),
                },
                false,
            ),
        ];
        for e in &cases {
            create(&client, e).await;
        }
        // A child of an in-day root must not appear as a root.
        let fresh = Forest::from(client.run_query(AllEntries).await.unwrap());
        let fresh_slots = LiveResult::Response(ClientQueryResponse::ChildSlots(child_slots(
            &fresh, parent.id,
        )));
        create(
            &client,
            &entry(Some(first_drop_position(&fresh_slots)), at(1), false),
        )
        .await;

        let from_sql: Vec<Uuid> = client
            .run_query(RootEntriesInInterval {
                from: day(),
                to: next,
            })
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        let forest = Forest::from(client.run_query(AllEntries).await.unwrap());
        let mut from_forest = forest.roots_in(day()..next);
        from_forest.sort_by_key(|e| (e.temporal.canonical_instant(), e.id));
        let from_forest: Vec<Uuid> = from_forest.into_iter().map(|e| e.id).collect();
        assert_eq!(from_sql, from_forest);
        // parent, 00:00, 07:00 (end-only), 23:00 (start+end), 23:00 (duration+end).
        assert_eq!(from_sql.len(), 5);
    }
}

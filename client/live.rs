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
//! This sits beside `QueryStore` (the older ping-and-pull cache) during migration; both listen to
//! the same change broadcast.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use gv_core::{
    error::{DbErr, Result},
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
    query
        .execute(
            &mut ForestExecutor::new(forest),
            &mut SqliteQueryExecutor::new(conn),
        )
        .await
        .into()
}

// --- Store -----------------------------------------------------------------------------------

struct Sub {
    id: SubscriptionId,
    refcount: usize,
    value: LiveResult,
}

struct State {
    /// The latest published version. Every stored value belongs to it (see `subscribe` for the
    /// one transient exception).
    version: Version,
    /// The forest of the latest version; `None` until first needed.
    forest: Option<Arc<Forest>>,
    subs: HashMap<ClientQuery, Sub>,
    next_id: SubscriptionId,
    /// A version is being built.
    building: bool,
}

struct Inner {
    pool: SqlitePool,
    state: Mutex<State>,
    sink: Mutex<Option<BatchSink>>,
    /// Asks the refresh loop for another version without a data change (see `sync`).
    rebuild: Notify,
    /// The latest published version, for `sync`.
    published: watch::Sender<Version>,
    /// Lets a test subscribe at a known point in a build (see `pause_next_build`).
    #[cfg(test)]
    build_pause: Mutex<Option<BuildPause>>,
}

#[cfg(test)]
struct BuildPause {
    entry_id: Uuid,
    reached: tokio::sync::oneshot::Sender<()>,
    resume: tokio::sync::oneshot::Receiver<()>,
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
                    building: false,
                }),
                sink: Mutex::new(None),
                rebuild: Notify::new(),
                published: watch::channel(0).0,
                #[cfg(test)]
                build_pause: Mutex::new(None),
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
                if let Err(e) = loop_store.publish_next_version().await {
                    warn!(error = %e, "live: building a version failed; retrying on next change");
                }
            }
        });
        store
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap()
    }

    /// Where batches go. Called on the refresh task, in version order, one call per version that
    /// changed anything a subscriber holds.
    pub fn set_batch_sink(&self, sink: BatchSink) {
        *self.inner.sink.lock().unwrap() = Some(sink);
    }

    pub fn version(&self) -> Version {
        self.lock().version
    }

    pub fn subscription_count(&self) -> usize {
        self.lock().subs.len()
    }

    /// Wait until a version is published whose build started after this call, so it reflects
    /// every write committed before the call (read-your-writes for forest-backed answers).
    ///
    /// Not called on the write path today: `run_action` returns at commit and versions trail it.
    /// Calling this from `run_action` would give read-your-writes at the cost of making every
    /// write wait for a full rebuild.
    pub async fn sync(&self) {
        let target = {
            let st = self.lock();
            // A build in flight may have read the database before the caller's write.
            st.version + if st.building { 2 } else { 1 }
        };
        self.inner.rebuild.notify_one();
        let mut rx = self.inner.published.subscribe();
        let _ = rx.wait_for(|v| *v >= target).await;
    }

    /// Subscribe to a query and get its first value now.
    ///
    /// Identical queries share one subscription id (refcounted, like `QueryStore`); the id is
    /// never reused after the last handle drops.
    ///
    /// Coherence: the first value is answered against the latest published forest, and inserted
    /// only if that version is still the latest (otherwise it's answered again), so it can't be
    /// behind the returned version; a build in flight picks up the new subscription before
    /// publishing (`build_and_swap`). A forest-backed first value belongs exactly to the returned
    /// version. A SQL-backed one is read fresh and may already reflect a write whose version
    /// hasn't been published yet, so one hook can briefly be a version ahead of the others until
    /// the next batch.
    pub async fn subscribe(&self, query: ClientQuery) -> Result<Subscribed> {
        {
            let mut st = self.lock();
            if let Some(subscribed) = self.join_existing(&mut st, &query) {
                return Ok(subscribed);
            }
        }

        let mut conn = self.inner.pool.acquire().await.db_err()?;
        loop {
            let (forest, seen_version) = self.latest_forest().await?;
            let value = answer(query.clone(), &forest, &mut conn).await;

            let mut st = self.lock();
            if let Some(subscribed) = self.join_existing(&mut st, &query) {
                return Ok(subscribed);
            }
            if st.version != seen_version {
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
    /// published yet.
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
        let forest = Arc::clone(st.forest.get_or_insert(forest));
        Ok((forest, st.version))
    }

    /// Build the next version from one read transaction and publish what changed.
    async fn publish_next_version(&self) -> Result<()> {
        self.lock().building = true;
        let batch = match self.build_and_swap().await {
            Ok(batch) => batch,
            Err(e) => {
                self.lock().building = false;
                return Err(e);
            }
        };

        if !batch.updates.is_empty() {
            let sink = self.inner.sink.lock().unwrap().clone();
            if let Some(sink) = sink {
                sink(batch);
            }
        }
        Ok(())
    }

    /// Answer every subscription in one read transaction, so the forest and every SQL-backed
    /// answer see the same snapshot, then swap the results in. A subscription added while this
    /// runs is answered too, so the whole batch belongs to the new version: the swap happens only
    /// under a lock that finds nothing left to answer.
    async fn build_and_swap(&self) -> Result<LiveBatch> {
        let mut tx = self.inner.pool.begin().await.db_err()?;
        let entries = SqliteQueryExecutor::new(&mut *tx)
            .execute(AllEntries)
            .await?;
        let forest = Arc::new(Forest::from(entries));
        let mut values: HashMap<ClientQuery, LiveResult> = HashMap::new();
        let batch = loop {
            let missing: Vec<ClientQuery> = {
                let mut st = self.lock();
                let missing: Vec<ClientQuery> = st
                    .subs
                    .keys()
                    .filter(|query| !values.contains_key(*query))
                    .cloned()
                    .collect();
                if missing.is_empty() {
                    break self.swap_in(&mut st, forest, values);
                }
                missing
            };
            for query in missing {
                let value = answer(query.clone(), &forest, &mut *tx).await;
                values.insert(query, value);
            }
            #[cfg(test)]
            self.wait_at_build_pause(&forest).await;
        };
        // Read-only, and the version is already published: nothing here can undo it.
        if let Err(e) = tx.rollback().await {
            warn!(error = %e, "live: ending a build's read transaction failed");
        }
        Ok(batch)
    }

    /// Publish a built version. Clears `building` in the same step as the version bump: `sync`
    /// reads the two together.
    fn swap_in(
        &self,
        st: &mut State,
        forest: Arc<Forest>,
        values: HashMap<ClientQuery, LiveResult>,
    ) -> LiveBatch {
        st.building = false;
        st.version += 1;
        st.forest = Some(forest);
        let mut updates = Vec::new();
        for (query, value) in values {
            // Skip keys whose last subscriber dropped while building.
            if let Some(sub) = st.subs.get_mut(&query) {
                if sub.value != value {
                    sub.value = value.clone();
                    updates.push(LiveUpdate {
                        subscription_id: sub.id,
                        value,
                    });
                }
            }
        }
        self.inner.published.send_replace(st.version);
        LiveBatch {
            version: st.version,
            updates,
        }
    }

    /// Pause the first build that can see `entry_id`, after it answers the subscriptions it
    /// found. Earlier builds must run on: the paused read transaction would block the write
    /// that creates the entry. Resolves `reached` when it gets there; it continues when `resume`
    /// is sent to.
    #[cfg(test)]
    fn pause_build_seeing(
        &self,
        entry_id: Uuid,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (reached_tx, reached_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
        *self.inner.build_pause.lock().unwrap() = Some(BuildPause {
            entry_id,
            reached: reached_tx,
            resume: resume_rx,
        });
        (reached_rx, resume_tx)
    }

    #[cfg(test)]
    async fn wait_at_build_pause(&self, forest: &Forest) {
        let pause = {
            let mut pause = self.inner.build_pause.lock().unwrap();
            match &*pause {
                Some(p) if forest.entry(p.entry_id).is_some() => pause.take(),
                _ => None,
            }
        };
        if let Some(pause) = pause {
            let _ = pause.reached.send(());
            let _ = pause.resume.await;
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
        actions::{CreateEntry, CreateUser, MoveEntry},
        io::SystemIo,
        models::{
            entry::{Entry, Position, Temporal},
            user::User,
        },
        queries::{FindAncestors, FindDescendants, FindEntryJoinById, RootEntriesInInterval},
    };
    use gv_ui::{ChildSlot, ChildSlots, SuggestedRootInsertionTime, WouldCreateCycle, child_slots};
    use std::collections::HashSet;
    use tokio::sync::mpsc;

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
        store.sync().await;

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
        store.sync().await;
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
        store.sync().await;
        assert!(store.version() > before);
        assert!(rx.try_recv().is_err(), "no subscribers, no batch");

        // Re-subscribing gets a fresh id, so a stale route can never match.
        let c = store.subscribe(day_roots()).await.unwrap();
        assert_ne!(c.subscription_id, a.subscription_id);
    }

    /// A subscription added mid-build is part of the version being built, so the whole batch
    /// belongs to one version, with no follow-up rebuild needed.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn subscribing_mid_build_joins_the_batch(pool: SqlitePool) {
        let (client, mut rx) = client(pool).await;
        let store = client.live();
        let parent = entry(None, at(9), true);
        create(&client, &parent).await;
        store.sync().await;
        let existing = store.subscribe(child_slots_of(parent.id)).await.unwrap();
        store.sync().await;

        // Hold the build for this write open, after it has answered `existing`.
        let child = entry(
            Some(first_drop_position(&existing.value)),
            Temporal::None,
            true,
        );
        let (reached, resume) = store.pause_build_seeing(child.id);
        create(&client, &child).await;
        reached.await.unwrap();
        // Nothing can publish while the refresh task is paused.
        let before = store.version();
        while rx.try_recv().is_ok() {}

        // Forest-backed: answered from the published forest, which doesn't have the child yet.
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
        // SQL-backed: read fresh, so it's already ahead of the published version.
        let descendants = store
            .subscribe(ClientQuery::FindDescendants(FindDescendants {
                entry_id: parent.id,
            }))
            .await
            .unwrap();
        assert!(matches!(
            &descendants.value,
            LiveResult::Response(ClientQueryResponse::FindDescendants(es))
                if es.iter().any(|e| e.id == child.id)
        ));

        resume.send(()).unwrap();
        let batch = next_batch(&mut rx).await;
        assert_eq!(batch.version, before + 1);
        let changed: HashSet<_> = batch.updates.iter().map(|u| u.subscription_id).collect();
        assert_eq!(
            changed,
            HashSet::from([existing.subscription_id, cycle.subscription_id]),
            "the forest-backed subscriber moved with the batch; the SQL one had nothing to change"
        );
        let cycle_value = &batch
            .updates
            .iter()
            .find(|u| u.subscription_id == cycle.subscription_id)
            .unwrap()
            .value;
        assert_eq!(
            *cycle_value,
            LiveResult::Response(ClientQueryResponse::WouldCreateCycle(true))
        );
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
        store.sync().await;
        let slots_sub = store.subscribe(child_slots_of(parent.id)).await.unwrap();
        let child = entry(
            Some(first_drop_position(&slots_sub.value)),
            Temporal::None,
            true,
        );
        create(&client, &child).await;
        store.sync().await;

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
        store.sync().await;
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

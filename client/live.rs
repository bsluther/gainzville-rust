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
//! is invisible to the consumer.
//!
//! This sits beside `QueryStore` (the older ping-and-pull cache) during migration; both listen to
//! the same change broadcast.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, MutexGuard},
};

use gv_core::{
    error::{DbErr, Result},
    forest::Forest,
    queries::AllEntries,
    query_executor::QueryExecutor,
};
use gv_sql::sqlite::SqliteQueryExecutor;
use sqlx::{SqliteConnection, SqlitePool};
use tokio::sync::{Notify, broadcast, watch};
use tracing::warn;

use crate::queries::{ClientQuery, ClientQueryResponse};
use crate::query_store::next_change;

pub type SubscriptionId = u64;
pub type Version = u64;

/// What a subscription holds: the answer, or why there isn't one.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveValue {
    Answer(ClientQueryResponse),
    /// The query itself failed (e.g. a lookup of an entry that no longer exists). Delivered like
    /// a value so one failing query can't stall the version for everyone; consumers keep their
    /// last good value and surface this on a side channel.
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveUpdate {
    pub subscription_id: SubscriptionId,
    pub value: LiveValue,
}

/// Everything that changed in one version. Batches are deltas: a consumer applies them in order
/// on top of the first values `subscribe` returned.
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
    pub value: LiveValue,
    pub handle: LiveSubscription,
}

// --- Backing ---------------------------------------------------------------------------------

// Which backing answers a query is declared in `ClientQuery` (`is_forest_backed`); these adapt
// the generated dispatch to `LiveValue`.

fn answer_from_forest(query: &ClientQuery, forest: &Forest) -> LiveValue {
    match query.answer_from_forest(forest) {
        Some(response) => LiveValue::Answer(response),
        None => LiveValue::Failed {
            message: "not a forest-backed query".to_string(),
        },
    }
}

async fn answer_from_sql(query: &ClientQuery, conn: &mut SqliteConnection) -> LiveValue {
    match query
        .clone()
        .execute(&mut SqliteQueryExecutor::new(conn))
        .await
    {
        Some(Ok(response)) => LiveValue::Answer(response),
        Some(Err(e)) => LiveValue::Failed {
            message: e.to_string(),
        },
        None => LiveValue::Failed {
            message: "not a SQL-backed query".to_string(),
        },
    }
}

async fn answer(query: &ClientQuery, forest: &Forest, conn: &mut SqliteConnection) -> LiveValue {
    if query.is_forest_backed() {
        answer_from_forest(query, forest)
    } else {
        answer_from_sql(query, conn).await
    }
}

// --- Store -----------------------------------------------------------------------------------

struct Sub {
    id: SubscriptionId,
    refcount: usize,
    value: LiveValue,
}

struct State {
    /// The latest published version. Every stored value belongs to it (see `subscribe` for the
    /// one transient exception).
    version: Version,
    /// The forest of the latest version; `None` until first needed.
    forest: Option<Arc<Forest>>,
    subs: HashMap<ClientQuery, Sub>,
    next_id: SubscriptionId,
    /// A version is being built (its key set is already fixed).
    building: bool,
}

struct Inner {
    pool: SqlitePool,
    state: Mutex<State>,
    sink: Mutex<Option<BatchSink>>,
    /// Asks the refresh loop for another version without a data change (see `subscribe`).
    rebuild: Notify,
    /// The latest published version, for `sync`.
    published: watch::Sender<Version>,
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
    /// Coherence: a forest-backed first value is computed from the latest published forest, so it
    /// belongs to the returned version. A SQL-backed first value is read fresh and may already
    /// reflect a write whose version hasn't been published yet — one hook can briefly be a
    /// version ahead of the others until the next batch. If the read may instead be *behind*
    /// (a version was building or was published while it ran), another version is requested.
    pub async fn subscribe(&self, query: ClientQuery) -> Result<Subscribed> {
        let seen_version = {
            let mut st = self.lock();
            if let Some(subscribed) = self.join_existing(&mut st, &query) {
                return Ok(subscribed);
            }
            st.version
        };

        let sql_value = if query.is_forest_backed() {
            self.ensure_forest().await?;
            None
        } else {
            let mut conn = self.inner.pool.acquire().await.db_err()?;
            Some(answer_from_sql(&query, &mut conn).await)
        };

        let mut st = self.lock();
        if let Some(subscribed) = self.join_existing(&mut st, &query) {
            return Ok(subscribed);
        }
        let value = match sql_value {
            Some(value) => {
                if st.building || st.version != seen_version {
                    self.inner.rebuild.notify_one();
                }
                value
            }
            None => {
                let forest = st
                    .forest
                    .clone()
                    .unwrap_or_else(|| Arc::new(Forest::from(Vec::new())));
                answer_from_forest(&query, &forest)
            }
        };
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
        Ok(Subscribed {
            subscription_id: id,
            version: st.version,
            value,
            handle: LiveSubscription {
                inner: Arc::clone(&self.inner),
                query,
                id,
            },
        })
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
    pub async fn read(&self, query: ClientQuery) -> Result<LiveValue> {
        if query.is_forest_backed() {
            let forest = self.ensure_forest().await?;
            Ok(answer_from_forest(&query, &forest))
        } else {
            let mut conn = self.inner.pool.acquire().await.db_err()?;
            Ok(answer_from_sql(&query, &mut conn).await)
        }
    }

    async fn ensure_forest(&self) -> Result<Arc<Forest>> {
        if let Some(forest) = self.lock().forest.clone() {
            return Ok(forest);
        }
        let mut conn = self.inner.pool.acquire().await.db_err()?;
        let entries = SqliteQueryExecutor::new(&mut conn)
            .execute(AllEntries)
            .await?;
        let forest = Arc::new(Forest::from(entries));
        Ok(self.lock().forest.get_or_insert(forest).clone())
    }

    /// Build the next version from one read transaction and publish what changed.
    async fn publish_next_version(&self) -> Result<()> {
        let keys: Vec<ClientQuery> = {
            let mut st = self.lock();
            st.building = true;
            st.subs.keys().cloned().collect()
        };
        let built = self.build(&keys).await;
        let (forest, values) = match built {
            Ok(built) => built,
            Err(e) => {
                self.lock().building = false;
                return Err(e);
            }
        };

        let batch = {
            let mut st = self.lock();
            st.building = false;
            st.version += 1;
            st.forest = Some(Arc::clone(&forest));
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
            // Forest-backed subscriptions added mid-build were answered from the previous forest;
            // answer them from this one so the whole batch belongs to this version.
            let built_keys: HashSet<&ClientQuery> = keys.iter().collect();
            for (query, sub) in st.subs.iter_mut() {
                if built_keys.contains(query) || !query.is_forest_backed() {
                    continue;
                }
                let value = answer_from_forest(query, &forest);
                if sub.value != value {
                    sub.value = value.clone();
                    updates.push(LiveUpdate {
                        subscription_id: sub.id,
                        value,
                    });
                }
            }
            self.inner.published.send_replace(st.version);
            LiveBatch {
                version: st.version,
                updates,
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

    async fn build(
        &self,
        keys: &[ClientQuery],
    ) -> Result<(Arc<Forest>, Vec<(ClientQuery, LiveValue)>)> {
        // One read transaction: the forest and every SQL-backed answer see the same snapshot.
        let mut tx = self.inner.pool.begin().await.db_err()?;
        let entries = SqliteQueryExecutor::new(&mut *tx)
            .execute(AllEntries)
            .await?;
        let forest = Arc::new(Forest::from(entries));
        let mut values = Vec::with_capacity(keys.len());
        for query in keys {
            values.push((query.clone(), answer(query, &forest, &mut *tx).await));
        }
        tx.rollback().await.db_err()?;
        Ok((forest, values))
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
    use gv_ui::{ChildSlot, ChildSlots, WouldCreateCycle, child_slots};
    use tokio::sync::mpsc;
    use uuid::Uuid;

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
        RootEntriesInInterval {
            from: day(),
            to: day() + Duration::days(1),
        }
        .into()
    }

    fn child_slots_of(parent_id: Uuid) -> ClientQuery {
        ChildSlots { parent_id }.into()
    }

    fn slots(value: &LiveValue) -> &[ChildSlot] {
        match value {
            LiveValue::Answer(ClientQueryResponse::ChildSlots(slots)) => slots,
            other => panic!("expected child slots, got {other:?}"),
        }
    }

    fn root_ids(value: &LiveValue) -> Vec<Uuid> {
        match value {
            LiveValue::Answer(ClientQueryResponse::RootEntriesInInterval(es)) => {
                es.iter().map(|e| e.id).collect()
            }
            other => panic!("unexpected value {other:?}"),
        }
    }

    fn first_drop_position(value: &LiveValue) -> Position {
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
            .subscribe(
                FindDescendants {
                    entry_id: parent.id,
                }
                .into(),
            )
            .await
            .unwrap();
        let _unrelated = store
            .subscribe(FindEntryJoinById { entry_id: other.id }.into())
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
            ClientQuery::from(WouldCreateCycle {
                entry_id,
                proposed_parent_id,
            })
        };
        assert_eq!(
            store.read(cycle(parent.id, child.id)).await.unwrap(),
            LiveValue::Answer(ClientQueryResponse::WouldCreateCycle(true))
        );
        assert_eq!(
            store.read(cycle(child.id, parent.id)).await.unwrap(),
            LiveValue::Answer(ClientQueryResponse::WouldCreateCycle(false))
        );
        assert_eq!(store.subscription_count(), 1, "read does not subscribe");
    }

    /// A query that errors is delivered as `Failed` instead of stalling the version for others.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn failing_query_does_not_stall_others(pool: SqlitePool) {
        let (client, mut rx) = client(pool).await;
        let store = client.live();
        // FindAncestors errors (NotFound) for a missing entry.
        let failing = store
            .subscribe(
                FindAncestors {
                    entry_id: Uuid::new_v4(),
                }
                .into(),
            )
            .await
            .unwrap();
        assert!(matches!(failing.value, LiveValue::Failed { .. }));
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
        let fresh_slots = LiveValue::Answer(ClientQueryResponse::ChildSlots(child_slots(
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

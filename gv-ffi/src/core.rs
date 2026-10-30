use std::sync::{Arc, LazyLock, Once};

use chrono::{DateTime, Utc};
use generation::{Arbitrary, Opts, SimulationContext};
use gv_client::{
    client::SqliteClient,
    live::{LiveBatch, LiveSubscription, LiveValue},
    queries::ClientQuery,
    query_store::{QuerySubscription, next_change},
};
use gv_core::{
    actions::{Action, CreateAttribute, CreateEntry, CreateValue},
    forest::Forest,
    models::entry::{Entry, Position},
    queries::{AllEntries, DomainQuery, DomainQueryResponse, SnapshotAll},
    std_lib::StandardLibrary,
};
use tokio::runtime::Runtime;
use tracing::info;
use uuid::Uuid;

use crate::types::FfiError;

static LOGGING: Once = Once::new();

fn init_logging() {
    LOGGING.call_once(|| {
        use tracing_oslog::OsLogger;
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
        tracing_subscriber::registry()
            .with(OsLogger::new("com.gainzville", "rust"))
            .init();
    });
}

// Single shared runtime for all FFI calls. Swift calls into Rust without a tokio
// context, so we drive all async work through this runtime via block_on.
static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
});

/// Swift implements this protocol to receive change notifications.
#[uniffi::export(with_foreign)]
pub trait CoreListener: Send + Sync {
    /// Called after any successful `run_action`. Swift reads from the cache
    /// synchronously via `read_query` after receiving this callback.
    fn on_data_changed(&self);
}

/// Swift implements this to receive live-query batches: one call per data version, carrying only
/// the subscriptions whose result changed. Called on a Rust thread, in version order; the
/// implementation should hop to the main actor and apply the whole batch in one step.
#[uniffi::export(with_foreign)]
pub trait LiveListener: Send + Sync {
    fn on_batch(&self, batch: LiveBatch);
}

/// Keeps a live subscription alive; releasing it unsubscribes (refcounted per identical query).
#[derive(uniffi::Object)]
pub struct LiveSubscriptionHandle(#[allow(dead_code)] LiveSubscription);

/// Result of `live_subscribe`: the first value now, the version it belongs to, and the handle.
#[derive(uniffi::Record)]
pub struct LiveSubscribed {
    pub subscription_id: u64,
    pub version: u64,
    pub value: LiveValue,
    pub handle: Arc<LiveSubscriptionHandle>,
}

/// UniFFI-compatible wrapper around `QuerySubscription`. Dropping this value
/// (when Swift releases its reference) automatically unsubscribes the query.
/// The inner Arc is held only for its Drop side-effect.
#[derive(uniffi::Object)]
pub struct FfiQuerySubscription(#[allow(dead_code)] Arc<QuerySubscription>);

/// The main entry point for Swift. Wraps `SqliteClient` with a static tokio
/// runtime and synchronous UniFFI-exported methods.
#[derive(uniffi::Object)]
pub struct GainzvilleCore {
    client: SqliteClient,
    actor_id: Uuid,
}

#[uniffi::export]
impl GainzvilleCore {
    /// Initialise the database at `db_path` and return a ready-to-use core.
    ///
    /// - `db_path`: SQLite connection string, e.g. `"sqlite:///path/to/db.sqlite"`.
    /// - `actor_id`: UUID identifying the current user's actor.
    /// - `listener`: Swift-side callback object for change notifications.
    #[uniffi::constructor]
    pub fn new(
        db_path: String,
        actor_id: Uuid,
        listener: Arc<dyn CoreListener>,
    ) -> Result<Arc<Self>, FfiError> {
        init_logging();
        let client = RUNTIME
            .block_on(SqliteClient::init(&db_path))
            .map_err(FfiError::from)?;

        // Wire the CoreListener: subscribe to cache-ready events and call
        // on_data_changed() from a background task each time the cache updates.
        let mut cache_ready_rx = client.subscribe_cache_ready();
        RUNTIME.spawn(async move {
            // `next_change`: Lagged is a (coalesced) change, not the end of the loop.
            while next_change(&mut cache_ready_rx).await {
                listener.on_data_changed();
            }
        });

        Ok(Arc::new(GainzvilleCore { client, actor_id }))
    }

    /// Execute a write action. Returns once the write has committed; the cache
    /// refresh and `on_data_changed()` callback happen asynchronously afterward.
    pub fn run_action(&self, action: Action) -> Result<(), FfiError> {
        RUNTIME
            .block_on(self.client.run_action(action))
            .map(|_| ())
            .map_err(|e| {
                info!(error = %e, "run_action failed");
                FfiError::from(e)
            })
    }

    /// Subscribe to a query. Runs the initial query immediately, populates the
    /// cache, and returns a `FfiQuerySubscription` handle. Dropping the handle
    /// (Swift releasing the reference) auto-removes the query from the cache.
    ///
    /// Legacy (ping-and-pull) path, keyed by the domain set; superseded by
    /// `live_subscribe` over the client set and retired screen by screen.
    pub fn subscribe_query(
        &self,
        query: DomainQuery,
    ) -> Result<Arc<FfiQuerySubscription>, FfiError> {
        let subscription = RUNTIME
            .block_on(self.client.subscribe_query(query))
            .map_err(FfiError::from)?;
        Ok(Arc::new(FfiQuerySubscription(subscription)))
    }

    /// Read the current cached result for a query. Returns `None` if the query
    /// is not subscribed. Swift calls this synchronously from the main thread
    /// after receiving `on_data_changed()`.
    pub fn read_query(&self, query: DomainQuery) -> Option<DomainQueryResponse> {
        self.client.read_cached_query(query)
    }

    // -------------------------------------------------------------------------
    // Live queries
    //
    // The successor to subscribe_query/read_query/forest_*/on_data_changed for
    // migrated views. Swift registers one LiveListener, subscribes per view
    // (first value returned synchronously), and receives one batch per data
    // version with only the changed subscriptions. Queries come from the client
    // set; how each is answered (in-memory forest or SQL) is declared in
    // gv_client::queries.
    // -------------------------------------------------------------------------

    /// Register the batch listener. Call once, before the first `live_subscribe`: batches are
    /// deltas on top of first values, so a subscriber that misses one stays stale.
    pub fn set_live_listener(&self, listener: Arc<dyn LiveListener>) {
        self.client
            .live()
            .set_batch_sink(Arc::new(move |batch| listener.on_batch(batch)));
    }

    /// Subscribe to a live query. Returns the first value immediately.
    pub fn live_subscribe(&self, query: ClientQuery) -> Result<LiveSubscribed, FfiError> {
        let subscribed = RUNTIME
            .block_on(self.client.live().subscribe(query))
            .map_err(FfiError::from)?;
        Ok(LiveSubscribed {
            subscription_id: subscribed.subscription_id,
            version: subscribed.version,
            value: subscribed.value,
            handle: Arc::new(LiveSubscriptionHandle(subscribed.handle)),
        })
    }

    /// Answer a live query once against the latest version, without subscribing.
    pub fn live_read(&self, query: ClientQuery) -> Result<LiveValue, FfiError> {
        RUNTIME
            .block_on(self.client.live().read(query))
            .map_err(FfiError::from)
    }

    /// Spawn a background task that creates a new activity every 10 seconds.
    /// Cache refresh and `on_data_changed()` fire automatically via the change
    /// broadcast — no manual wiring needed here.
    pub fn start_background_ticker(&self) {
        let _guard = RUNTIME.enter();
        self.client.start_background_ticker(self.actor_id);
    }

    // -------------------------------------------------------------------------
    // Forest
    //
    // The forest is backed by the AllEntries query cache. Call `subscribe_forest`
    // once to start maintaining the cache; the returned token keeps it alive.
    // After each `on_data_changed` callback, call any of the synchronous
    // `forest_*` methods to read the current traversal state — no async needed.
    // Adding new traversal methods is: call `forest_snapshot()`, call the Forest
    // method, map results.
    // -------------------------------------------------------------------------

    /// Subscribe to the forest cache. Internally subscribes to AllEntries.
    /// Hold the returned token for the lifetime of the subscriber; dropping it
    /// unsubscribes automatically, same as `subscribe_query`.
    pub fn subscribe_forest(&self) -> Result<Arc<FfiQuerySubscription>, FfiError> {
        let subscription = RUNTIME
            .block_on(
                self.client
                    .subscribe_query(DomainQuery::AllEntries(AllEntries {})),
            )
            .map_err(FfiError::from)?;
        Ok(Arc::new(FfiQuerySubscription(subscription)))
    }

    /// All root entries (no parent), sorted by canonical instant.
    pub fn forest_roots(&self) -> Vec<Entry> {
        self.forest_snapshot()
            .map(|f| f.roots().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Root entries whose canonical instant falls within `[from, to)`, sorted by time.
    pub fn forest_roots_in(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Entry> {
        self.forest_snapshot()
            .map(|f| f.roots_in(from..to).into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Direct children of `parent_id`, sorted by fractional index.
    pub fn forest_children(&self, parent_id: Uuid) -> Vec<Entry> {
        self.forest_snapshot()
            .map(|f| f.children(parent_id).into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// The root template entry for an activity, if present in the forest cache.
    /// Backs the library's activity-template editing: the activity detail view
    /// resolves the root here, then reuses the existing forest traversal +
    /// per-entry join subscriptions to render and edit the template.
    pub fn forest_activity_template_root(&self, activity_id: Uuid) -> Option<Entry> {
        self.forest_snapshot()
            .and_then(|f| f.template_root(activity_id).cloned())
    }

    /// Ancestors of `entry_id` from immediate parent up to the root.
    pub fn forest_ancestors(&self, entry_id: Uuid) -> Vec<Entry> {
        self.forest_snapshot()
            .map(|f| f.ancestors(entry_id).into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Position immediately after the last child of `parent_id`.
    /// Returns `None` if the parent is not found in the current snapshot.
    /// Caller must ensure `parent_id` refers to a sequence entry.
    pub fn forest_position_after_children(&self, parent_id: Uuid) -> Option<Position> {
        self.forest_snapshot()
            .and_then(|f| f.position_after_children(parent_id))
    }

    /// Returns true if moving `entry_id` under `proposed_parent_id` would create a cycle.
    pub fn forest_would_create_cycle(&self, entry_id: Uuid, proposed_parent_id: Uuid) -> bool {
        self.forest_snapshot()
            .map(|f| f.would_create_cycle(entry_id, proposed_parent_id))
            .unwrap_or(false)
    }

    /// Position between two adjacent children of a sequence.
    /// `pred_id` and `succ_id` are the IDs of the predecessor and successor entries;
    /// pass `None` for the start or end of the child list.
    /// Caller must ensure `parent_id` refers to a sequence entry.
    pub fn forest_position_between(
        &self,
        parent_id: Uuid,
        pred_id: Option<Uuid>,
        succ_id: Option<Uuid>,
    ) -> Option<Position> {
        self.forest_snapshot()
            .and_then(|f| f.position_between(parent_id, pred_id, succ_id))
    }

    // (Suggested root insertion time moved to the client set:
    // `ClientQuery::SuggestedRootInsertionTime`, read once via `live_read`.)

    // -------------------------------------------------------------------------
    // Dev / debug utilities
    //
    // These methods exist for testing and data seeding from the Swift app.
    // They are not part of the production API.
    // -------------------------------------------------------------------------

    /// Seed the standard-library attributes (Reps, Load, Outcome, YDS Grade).
    /// Safe to call multiple times — will create duplicates, so call once per fresh DB.
    pub fn dev_seed_std_lib(&self) -> Result<(), FfiError> {
        for attr in StandardLibrary::attributes() {
            let action: CreateAttribute = attr.into();
            RUNTIME
                .block_on(self.client.run_action(action.into()))
                .map_err(FfiError::from)?;
        }
        Ok(())
    }

    /// Create up to `count` arbitrary attribute values, each pairing an existing
    /// entry with an existing attribute (matched by owner). Requires at least one
    /// entry and one attribute; does nothing otherwise. Pairs are deduplicated so
    /// repeat calls converge instead of erroring on PK conflicts.
    pub fn dev_create_arbitrary_values(&self, count: u32) -> Result<(), FfiError> {
        let snapshot = RUNTIME
            .block_on(self.client.run_query(SnapshotAll))
            .map_err(FfiError::from)?;
        if snapshot.entries.is_empty() || snapshot.attributes.is_empty() {
            return Ok(());
        }
        let mut context = SimulationContext::with_opts(Opts::time_now_tight_std());
        context.load_snapshot(snapshot);
        let mut rng = rand::rng();
        let mut seen: std::collections::HashSet<(uuid::Uuid, uuid::Uuid)> =
            std::collections::HashSet::new();

        let max_attempts = count.saturating_mul(5).max(count);
        let mut attempts = 0u32;
        let mut created = 0u32;
        while created < count && attempts < max_attempts {
            attempts += 1;
            let action = CreateValue::arbitrary(&mut rng, &context);
            if !seen.insert((action.value.entry_id, action.value.attribute_id)) {
                continue;
            }
            // Best-effort: a duplicate row from a prior call (or any other
            // mutator rejection) is silently skipped so seeding stays idempotent.
            if RUNTIME
                .block_on(self.client.run_action(action.into()))
                .is_ok()
            {
                created += 1;
            }
        }
        Ok(())
    }

    /// Create `count` arbitrary entries drawn from the current activities and entries in the DB.
    /// Entries are clustered around the current time.
    pub fn dev_create_arbitrary_entries(&self, count: u32) -> Result<(), FfiError> {
        // Hydrate the simulation model from the live DB so generated entries
        // draw from the activities/entries currently present.
        let mut context = SimulationContext::with_opts(Opts::time_now_tight_std());
        let snapshot = RUNTIME
            .block_on(self.client.run_query(SnapshotAll))
            .map_err(FfiError::from)?;
        context.load_snapshot(snapshot);

        let mut rng = rand::rng();

        for _ in 0..count {
            let entry = Entry::arbitrary(&mut rng, &context);
            let action: CreateEntry = entry.clone().into();
            let mx = RUNTIME
                .block_on(self.client.run_action(action.into()))
                .map_err(FfiError::from)?;
            // Update the model.
            RUNTIME
                .block_on(context.apply_mutation(mx))
                .map_err(FfiError::from)?;
        }
        Ok(())
    }
}

impl GainzvilleCore {
    /// Read the AllEntries cache and wrap it in a Forest for synchronous traversal.
    /// Returns None if the forest has not been subscribed yet.
    fn forest_snapshot(&self) -> Option<Forest> {
        match self
            .client
            .read_cached_query(DomainQuery::AllEntries(AllEntries {}))?
        {
            DomainQueryResponse::AllEntries(entries) => Some(Forest::from(entries)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gv_client::queries::ClientQueryResponse;
    use gv_core::{DEFAULT_USER_ID, models::entry::Temporal};
    use gv_ui::{ChildSlot, ChildSlots, WouldCreateCycle};
    use std::sync::Mutex;

    struct NoopListener;
    impl CoreListener for NoopListener {
        fn on_data_changed(&self) {}
    }

    #[derive(Default)]
    struct CollectBatches(Mutex<Vec<LiveBatch>>);
    impl LiveListener for CollectBatches {
        fn on_batch(&self, batch: LiveBatch) {
            self.0.lock().unwrap().push(batch);
        }
    }

    fn entry(position: Option<Position>, is_sequence: bool) -> Entry {
        let temporal = if position.is_none() {
            Temporal::Start { start: Utc::now() }
        } else {
            Temporal::None
        };
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

    /// The exported surface end to end, as Swift drives it: register the listener, subscribe
    /// (first value synchronously), write, receive one batch, release the handle.
    #[test]
    fn live_queries_round_trip_through_the_ffi_object() {
        let path = std::env::temp_dir().join(format!("gv-ffi-live-{}.sqlite", Uuid::new_v4()));
        let core = GainzvilleCore::new(
            format!("sqlite://{}?mode=rwc", path.display()),
            DEFAULT_USER_ID,
            Arc::new(NoopListener),
        )
        .unwrap();
        let listener = Arc::new(CollectBatches::default());
        core.set_live_listener(listener.clone());

        let parent = entry(None, true);
        core.run_action(CreateEntry::from(parent.clone()).into())
            .unwrap();
        // Versions trail commits; wait for one that includes the parent.
        RUNTIME.block_on(core.client.live().sync());

        let sub = core
            .live_subscribe(
                ChildSlots {
                    parent_id: parent.id,
                }
                .into(),
            )
            .unwrap();
        let LiveValue::Answer(ClientQueryResponse::ChildSlots(slots)) = &sub.value else {
            panic!("expected child slots")
        };
        let ChildSlot::Drop { position, .. } = slots[0].clone() else {
            panic!("expected a drop slot")
        };

        let child = entry(Some(position), false);
        core.run_action(CreateEntry::from(child.clone()).into())
            .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let batch = loop {
            if let Some(batch) = listener.0.lock().unwrap().iter().find(|b| {
                b.updates
                    .iter()
                    .any(|u| u.subscription_id == sub.subscription_id)
            }) {
                break batch.clone();
            }
            assert!(std::time::Instant::now() < deadline, "no batch");
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(batch.version > sub.version);
        let LiveValue::Answer(ClientQueryResponse::ChildSlots(after)) = &batch.updates[0].value
        else {
            panic!("expected child slots")
        };
        assert!(matches!(&after[1], ChildSlot::Entry { entry } if entry.id == child.id));

        // Read once: moving the parent under its child would be a cycle.
        assert_eq!(
            core.live_read(
                WouldCreateCycle {
                    entry_id: parent.id,
                    proposed_parent_id: child.id,
                }
                .into()
            )
            .unwrap(),
            LiveValue::Answer(ClientQueryResponse::WouldCreateCycle(true))
        );

        // Releasing the handle (Swift dropping its reference) unsubscribes.
        drop(sub);
        assert_eq!(core.client.live().subscription_count(), 0);
        let _ = std::fs::remove_file(&path);
    }
}

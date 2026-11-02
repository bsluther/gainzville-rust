use std::sync::{Arc, LazyLock, Once};

use generation::{Arbitrary, Opts, SimulationContext};
use gv_client::{
    client::SqliteClient,
    live::{LiveBatch, LiveResult, LiveSubscription},
    queries::{ClientQuery, ClientQueryResponse},
};
use gv_core::{
    actions::{Action, CreateAttribute, CreateEntry, CreateValue},
    models::entry::Entry,
    queries::SnapshotAll,
    std_lib::StandardLibrary,
};
use tokio::runtime::Runtime;
use tracing::{info, warn};
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

/// Swift implements this to receive live-query batches: one call per data version, carrying only
/// the subscriptions whose result changed. Called on a Rust thread, in version order, before any
/// `live_subscribe` can report that version. The implementation must only enqueue the batch, then
/// apply it on the main actor in one step.
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
    pub value: LiveResult,
    pub handle: Arc<LiveSubscriptionHandle>,
}

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
    #[uniffi::constructor]
    pub fn new(db_path: String, actor_id: Uuid) -> Result<Arc<Self>, FfiError> {
        init_logging();
        let client = RUNTIME
            .block_on(SqliteClient::init(&db_path))
            .map_err(FfiError::from)?;

        // Load the forest off the caller's thread, so the first forest-backed subscribe doesn't.
        let live = client.live().clone();
        RUNTIME.spawn(async move {
            if let Err(e) = live.warm().await {
                warn!(error = %e, "warming the live forest failed");
            }
        });

        Ok(Arc::new(GainzvilleCore { client, actor_id }))
    }

    /// Execute a write action. Returns once the write has committed; the batch
    /// reflecting it arrives asynchronously afterward.
    pub fn run_action(&self, action: Action) -> Result<(), FfiError> {
        RUNTIME
            .block_on(self.client.run_action(action))
            .map(|_| ())
            .map_err(|e| {
                info!(error = %e, "run_action failed");
                FfiError::from(e)
            })
    }

    // -------------------------------------------------------------------------
    // Live queries
    //
    // Swift registers one LiveListener, subscribes per view (first value returned
    // synchronously), and receives one batch per data version with only the
    // changed subscriptions. Queries come from the client set; how each is
    // answered (in-memory forest or SQL) is declared in gv_client::queries.
    // -------------------------------------------------------------------------

    /// Register the batch listener. Call once, before the first `live_subscribe`: batches are
    /// deltas on top of first values, so a subscriber that misses one stays stale. `on_batch` runs
    /// inside the step that publishes the version, so it must only enqueue: it must not block or
    /// call back into this object.
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
    pub fn live_read(&self, query: ClientQuery) -> Result<ClientQueryResponse, FfiError> {
        RUNTIME
            .block_on(self.client.live().read(query))
            .map_err(FfiError::from)
    }

    /// Spawn a background task that creates a new activity every 10 seconds.
    /// Live queries update automatically via the change broadcast — no manual
    /// wiring needed here.
    pub fn start_background_ticker(&self) {
        let _guard = RUNTIME.enter();
        self.client.start_background_ticker(self.actor_id);
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use gv_core::{
        DEFAULT_USER_ID,
        models::entry::{Position, Temporal},
    };
    use gv_ui::{ChildSlot, ChildSlots, WouldCreateCycle};
    use std::sync::Mutex;

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

    /// The seeder stops at the first rejected entry, so one bad pick loses the rest. A fresh
    /// database has the standard library's templates, which must not be picked as parents.
    #[test]
    fn seeding_arbitrary_entries_creates_them_all() {
        let path = std::env::temp_dir().join(format!("gv-ffi-seed-{}.sqlite", Uuid::new_v4()));
        let core = GainzvilleCore::new(
            format!("sqlite://{}?mode=rwc", path.display()),
            DEFAULT_USER_ID,
        )
        .unwrap();
        core.dev_create_arbitrary_entries(200).unwrap();
        let snapshot = RUNTIME
            .block_on(core.client.run_query(SnapshotAll))
            .unwrap();
        let log_entries = snapshot.entries.iter().filter(|e| !e.is_template).count();
        assert_eq!(log_entries, 200);
        let _ = std::fs::remove_file(&path);
    }

    /// The exported surface end to end, as Swift drives it: register the listener, subscribe
    /// (first value synchronously), write, receive one batch, release the handle.
    #[test]
    fn live_queries_round_trip_through_the_ffi_object() {
        let path = std::env::temp_dir().join(format!("gv-ffi-live-{}.sqlite", Uuid::new_v4()));
        let core = GainzvilleCore::new(
            format!("sqlite://{}?mode=rwc", path.display()),
            DEFAULT_USER_ID,
        )
        .unwrap();
        let listener = Arc::new(CollectBatches::default());
        core.set_live_listener(listener.clone());

        let parent = entry(None, true);
        core.run_action(CreateEntry::from(parent.clone()).into())
            .unwrap();
        // Versions trail commits; wait for one that includes the parent.
        RUNTIME.block_on(core.client.live().sync()).unwrap();

        let sub = core
            .live_subscribe(ClientQuery::ChildSlots(ChildSlots {
                parent_id: parent.id,
            }))
            .unwrap();
        let LiveResult::Response(ClientQueryResponse::ChildSlots(slots)) = &sub.value else {
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
        let LiveResult::Response(ClientQueryResponse::ChildSlots(after)) = &batch.updates[0].value
        else {
            panic!("expected child slots")
        };
        assert!(matches!(&after[1], ChildSlot::Entry { entry } if entry.id == child.id));

        // Read once: moving the parent under its child would be a cycle.
        assert_eq!(
            core.live_read(ClientQuery::WouldCreateCycle(WouldCreateCycle {
                entry_id: parent.id,
                proposed_parent_id: child.id,
            }))
            .unwrap(),
            ClientQueryResponse::WouldCreateCycle(true)
        );

        // Releasing the handle (Swift dropping its reference) unsubscribes.
        drop(sub);
        assert_eq!(core.client.live().subscription_count(), 0);
        let _ = std::fs::remove_file(&path);
    }
}

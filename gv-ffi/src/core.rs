use std::sync::{Arc, LazyLock};

use gv_client::{client::SqliteClient, query_store::QuerySubscription};
use tokio::runtime::Runtime;
use uuid::Uuid;

use generation::{ArbitraryFrom, Opts, SimulationContext};
use gv_core::{
    actions::{CreateAttribute, CreateEntry, CreateValue},
    forest::Forest,
    models::entry::Entry,
    queries::{AllActivities, AllAttributes, AllEntries, AnyQuery, AnyQueryResponse},
    std_lib::StandardLibrary,
};

use crate::types::{
    FfiAction, FfiAnyQuery, FfiAnyQueryResponse, FfiEntry, FfiError, FfiPosition,
    ffi_action_to_core, parse_timestamp_ms, parse_uuid,
};

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
    /// - `actor_id`: UUID string identifying the current user's actor.
    /// - `listener`: Swift-side callback object for change notifications.
    #[uniffi::constructor]
    pub fn new(
        db_path: String,
        actor_id: String,
        listener: Arc<dyn CoreListener>,
    ) -> Result<Arc<Self>, FfiError> {
        let actor_id = parse_uuid(&actor_id)?;
        let client = RUNTIME
            .block_on(SqliteClient::init(&db_path))
            .map_err(FfiError::from)?;

        // Wire the CoreListener: subscribe to cache-ready events and call
        // on_data_changed() from a background task each time the cache updates.
        let mut cache_ready_rx = client.subscribe_cache_ready();
        RUNTIME.spawn(async move {
            while let Ok(()) = cache_ready_rx.recv().await {
                listener.on_data_changed();
            }
        });

        Ok(Arc::new(GainzvilleCore { client, actor_id }))
    }

    /// Execute a write action. Returns once the write has committed; the cache
    /// refresh and `on_data_changed()` callback happen asynchronously afterward.
    pub fn run_action(&self, action: FfiAction) -> Result<(), FfiError> {
        let core_action = ffi_action_to_core(action, self.actor_id)?;
        RUNTIME
            .block_on(self.client.run_action(core_action))
            .map_err(FfiError::from)
    }

    /// Subscribe to a query. Runs the initial query immediately, populates the
    /// cache, and returns a `FfiQuerySubscription` handle. Dropping the handle
    /// (Swift releasing the reference) auto-removes the query from the cache.
    pub fn subscribe_query(
        &self,
        query: FfiAnyQuery,
    ) -> Result<Arc<FfiQuerySubscription>, FfiError> {
        let core_query = AnyQuery::try_from(query)?;
        let subscription = RUNTIME
            .block_on(self.client.subscribe_query(core_query))
            .map_err(FfiError::from)?;
        Ok(Arc::new(FfiQuerySubscription(subscription)))
    }

    /// Read the current cached result for a query. Returns `None` if the query
    /// is not subscribed or if the query parameters are invalid. Swift calls
    /// this synchronously from the main thread after receiving `on_data_changed()`.
    pub fn read_query(&self, query: FfiAnyQuery) -> Option<FfiAnyQueryResponse> {
        let core_query = AnyQuery::try_from(query).ok()?;
        self.client
            .read_cached_query(core_query)
            .map(FfiAnyQueryResponse::from)
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
            .block_on(self.client.subscribe_query(AnyQuery::AllEntries(AllEntries {})))
            .map_err(FfiError::from)?;
        Ok(Arc::new(FfiQuerySubscription(subscription)))
    }

    /// All root entries (no parent), sorted by canonical instant.
    pub fn forest_roots(&self) -> Vec<FfiEntry> {
        self.forest_snapshot()
            .map(|f| f.roots().into_iter().map(|e| FfiEntry::from(e.clone())).collect())
            .unwrap_or_default()
    }

    /// Root entries whose canonical instant falls within `[from, to)` (Unix ms), sorted by time.
    pub fn forest_roots_in(&self, from: i64, to: i64) -> Vec<FfiEntry> {
        let Ok(from_dt) = parse_timestamp_ms(from) else { return vec![] };
        let Ok(to_dt) = parse_timestamp_ms(to) else { return vec![] };
        self.forest_snapshot()
            .map(|f| {
                f.roots_in(from_dt..to_dt)
                    .into_iter()
                    .map(|e| FfiEntry::from(e.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Direct children of `parent_id`, sorted by fractional index.
    pub fn forest_children(&self, parent_id: String) -> Vec<FfiEntry> {
        let Ok(id) = parse_uuid(&parent_id) else { return vec![] };
        self.forest_snapshot()
            .map(|f| f.children(id).into_iter().map(|e| FfiEntry::from(e.clone())).collect())
            .unwrap_or_default()
    }

    /// Ancestors of `entry_id` from immediate parent up to the root.
    pub fn forest_ancestors(&self, entry_id: String) -> Vec<FfiEntry> {
        let Ok(id) = parse_uuid(&entry_id) else { return vec![] };
        self.forest_snapshot()
            .map(|f| f.ancestors(id).into_iter().map(|e| FfiEntry::from(e.clone())).collect())
            .unwrap_or_default()
    }

    /// Position immediately after the last child of `parent_id`.
    /// Returns `None` if the parent is not found in the current snapshot.
    /// Caller must ensure `parent_id` refers to a sequence entry.
    pub fn forest_position_after_children(&self, parent_id: String) -> Option<FfiPosition> {
        let Ok(id) = parse_uuid(&parent_id) else { return None };
        self.forest_snapshot()
            .and_then(|f| f.position_after_children(id))
            .map(FfiPosition::from)
    }

    /// Returns true if moving `entry_id` under `proposed_parent_id` would create a cycle.
    pub fn forest_would_create_cycle(&self, entry_id: String, proposed_parent_id: String) -> bool {
        let (Ok(entry_id), Ok(proposed_parent_id)) =
            (parse_uuid(&entry_id), parse_uuid(&proposed_parent_id))
        else {
            return false;
        };
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
        parent_id: String,
        pred_id: Option<String>,
        succ_id: Option<String>,
    ) -> Option<FfiPosition> {
        let Ok(parent_id) = parse_uuid(&parent_id) else { return None };
        let pred_id = match pred_id.as_deref().map(parse_uuid) {
            Some(Ok(id)) => Some(id),
            Some(Err(_)) => return None,
            None => None,
        };
        let succ_id = match succ_id.as_deref().map(parse_uuid) {
            Some(Ok(id)) => Some(id),
            Some(Err(_)) => return None,
            None => None,
        };
        self.forest_snapshot()
            .map(|f| f.position_between(parent_id, pred_id, succ_id))
            .map(FfiPosition::from)
    }

    /// Suggested start time (Unix ms) for a new root-level entry on the given day.
    /// `day_start` is the start of the day in Unix ms. Returns now if today,
    /// one minute after the last existing root entry otherwise, or noon as a fallback.
    pub fn forest_suggested_root_day_insertion_time(&self, day_start: i64) -> i64 {
        let Ok(day_start_dt) = parse_timestamp_ms(day_start) else { return day_start };
        let forest = self.forest_snapshot().unwrap_or_else(|| Forest::from(vec![]));
        forest.suggested_root_day_insertion_time(day_start_dt).timestamp_millis()
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
        let attributes = RUNTIME
            .block_on(self.client.run_query(AllAttributes {}))
            .map_err(FfiError::from)?;
        let entries: Vec<Entry> = RUNTIME
            .block_on(self.client.run_query(AllEntries {}))
            .map_err(FfiError::from)?;
        if attributes.is_empty() || entries.is_empty() {
            return Ok(());
        }

        let context = SimulationContext::with_opts(Opts::time_now_tight_std());
        let mut rng = rand::rng();
        let mut seen: std::collections::HashSet<(uuid::Uuid, uuid::Uuid)> =
            std::collections::HashSet::new();

        let max_attempts = count.saturating_mul(5).max(count);
        let mut attempts = 0u32;
        let mut created = 0u32;
        while created < count && attempts < max_attempts {
            attempts += 1;
            let action = CreateValue::arbitrary_from(
                &mut rng,
                &context,
                (entries.as_slice(), attributes.as_slice()),
            );
            if !seen.insert((action.value.entry_id, action.value.attribute_id)) {
                continue;
            }
            // Best-effort: a duplicate row from a prior call (or any other
            // mutator rejection) is silently skipped so seeding stays idempotent.
            if RUNTIME.block_on(self.client.run_action(action.into())).is_ok() {
                created += 1;
            }
        }
        Ok(())
    }

    /// Create `count` arbitrary entries drawn from the current activities and entries in the DB.
    /// Entries are clustered around the current time. Requires at least one activity to exist;
    /// does nothing if there are none.
    pub fn dev_create_arbitrary_entries(&self, count: u32) -> Result<(), FfiError> {
        let activities = RUNTIME
            .block_on(self.client.run_query(AllActivities {}))
            .map_err(FfiError::from)?;
        if activities.is_empty() {
            return Ok(());
        }

        let mut entries: Vec<Entry> = RUNTIME
            .block_on(self.client.run_query(AllEntries {}))
            .map_err(FfiError::from)?;

        let actor_ids = vec![self.actor_id];
        let context = SimulationContext::with_opts(Opts::time_now_tight_std());
        let mut rng = rand::rng();

        for _ in 0..count {
            let entry = Entry::arbitrary_from(
                &mut rng,
                &context,
                (actor_ids.as_slice(), activities.as_slice(), entries.as_slice()),
            );
            let action: CreateEntry = entry.clone().into();
            RUNTIME
                .block_on(self.client.run_action(action.into()))
                .map_err(FfiError::from)?;
            // Include the new entry so subsequent entries can nest inside it.
            entries.push(entry);
        }
        Ok(())
    }
}

impl GainzvilleCore {
    /// Read the AllEntries cache and wrap it in a Forest for synchronous traversal.
    /// Returns None if the forest has not been subscribed yet.
    fn forest_snapshot(&self) -> Option<Forest> {
        match self.client.read_cached_query(AnyQuery::AllEntries(AllEntries {}))? {
            AnyQueryResponse::AllEntries(entries) => Some(Forest::from(entries)),
            _ => None,
        }
    }
}

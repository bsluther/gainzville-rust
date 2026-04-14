use std::sync::{Arc, LazyLock};

use gv_client::{client::SqliteClient, query_store::QuerySubscription};
use tokio::runtime::Runtime;
use uuid::Uuid;

use crate::types::{FfiAction, FfiAnyQuery, FfiAnyQueryResponse, FfiError, ffi_action_to_core, parse_uuid};

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
        let subscription = RUNTIME
            .block_on(self.client.subscribe_query(query.into()))
            .map_err(FfiError::from)?;
        Ok(Arc::new(FfiQuerySubscription(subscription)))
    }

    /// Read the current cached result for a query. Returns `None` if the query
    /// is not subscribed. Swift calls this synchronously from the main thread
    /// after receiving `on_data_changed()`.
    pub fn read_query(&self, query: FfiAnyQuery) -> Option<FfiAnyQueryResponse> {
        self.client
            .read_cached_query(query.into())
            .map(FfiAnyQueryResponse::from)
    }

    /// Spawn a background task that creates a new activity every 10 seconds.
    /// Cache refresh and `on_data_changed()` fire automatically via the change
    /// broadcast — no manual wiring needed here.
    pub fn start_background_ticker(&self) {
        self.client.start_background_ticker(self.actor_id);
    }
}

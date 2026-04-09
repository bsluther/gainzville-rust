use std::sync::{Arc, LazyLock};

use gv_core::queries::AllActivities;
use gv_core::query_executor::QueryExecutor;
use gv_sqlite::{client::SqliteClient, sqlite_executor::SqliteQueryExecutor};
use tokio::runtime::Runtime;
use uuid::Uuid;

use crate::types::{FfiAction, FfiActivity, FfiError, ffi_action_to_core, parse_uuid};

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
    /// Called after any successful `run_action`. Swift should read fresh data
    /// synchronously after receiving this callback.
    fn on_data_changed(&self);
}

/// The main entry point for Swift. Wraps `SqliteClient` with a static tokio
/// runtime and synchronous UniFFI-exported methods.
#[derive(uniffi::Object)]
pub struct GainzvilleCore {
    client: SqliteClient,
    actor_id: Uuid,
    listener: Arc<dyn CoreListener>,
}

#[uniffi::export]
impl GainzvilleCore {
    /// Initialise the database at `db_path` and return a ready-to-use core.
    ///
    /// - `db_path`: SQLite connection string, e.g. `"sqlite:///path/to/db.sqlite"` or `":memory:"`.
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
        Ok(Arc::new(GainzvilleCore {
            client,
            actor_id,
            listener,
        }))
    }

    /// Execute a write action. Synchronous at the FFI boundary; async work runs
    /// on the internal runtime. Calls `listener.on_data_changed()` on success.
    pub fn run_action(&self, action: FfiAction) -> Result<(), FfiError> {
        let core_action = ffi_action_to_core(action, self.actor_id)?;
        RUNTIME
            .block_on(self.client.run_action(core_action))
            .map_err(FfiError::from)?;
        self.listener.on_data_changed();
        Ok(())
    }

    /// Return a snapshot of all activities. Synchronous; performs a live SQLite
    /// query on the internal runtime (no in-memory cache yet).
    pub fn get_activities(&self) -> Vec<FfiActivity> {
        RUNTIME
            .block_on(async {
                let mut conn = self.client.pool.acquire().await?;
                SqliteQueryExecutor::new(&mut *conn)
                    .execute(AllActivities {})
                    .await
            })
            .unwrap_or_default()
            .into_iter()
            .map(FfiActivity::from)
            .collect()
    }
}

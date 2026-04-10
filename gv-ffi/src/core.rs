use std::sync::{Arc, LazyLock};
use std::time::Duration;

use futures_util::StreamExt;
use gv_core::actions::{Action, CreateActivity};
use gv_core::models::activity::{Activity, ActivityName};
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

/// Swift implements this protocol to receive live activity list updates.
/// Called on a background thread — dispatch to the main thread before touching UI.
#[uniffi::export(with_foreign)]
pub trait ActivitiesListener: Send + Sync {
    fn on_activities_changed(&self, activities: Vec<FfiActivity>);
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

    /// Subscribe to live activity updates. Drives `client.stream_activities()` on the
    /// internal runtime. The listener is called immediately with the current list, then
    /// again after every write that touches activities.
    ///
    /// Called on a background thread — the Swift listener must dispatch to the main
    /// thread before touching UI state.
    pub fn subscribe_activities(&self, listener: Arc<dyn ActivitiesListener>) {
        let stream = self.client.stream_activities();
        RUNTIME.spawn(async move {
            futures_util::pin_mut!(stream);
            while let Some(result) = stream.next().await {
                if let Ok(activities) = result {
                    let ffi = activities.into_iter().map(FfiActivity::from).collect();
                    listener.on_activities_changed(ffi);
                }
            }
        });
    }

    /// Spawn a background task that creates a new activity every 10 seconds.
    /// Each write broadcasts on the internal change channel, which automatically
    /// wakes any active `subscribe_activities` subscribers.
    pub fn start_background_ticker(&self) {
        let client = self.client.clone();
        let actor_id = self.actor_id;
        RUNTIME.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            interval.tick().await; // skip the immediate first tick
            let mut counter = 0u64;
            loop {
                interval.tick().await;
                counter += 1;
                let activity = Activity {
                    id: Uuid::new_v4(),
                    owner_id: actor_id,
                    name: ActivityName::parse(format!("Auto Activity {counter}"))
                        .unwrap_or_else(|_| ActivityName::parse("Auto".to_string()).unwrap()),
                    description: Some(format!("Created by background ticker (tick #{counter})")),
                    source_activity_id: None,
                };
                let action: Action = CreateActivity { actor_id, activity }.into();
                // run_action broadcasts on change_transmitter → wakes stream_activities
                let _ = client.run_action(action).await;
            }
        });
    }
}

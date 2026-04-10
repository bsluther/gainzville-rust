use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use gv_core::actions::{Action, CreateActivity};
use gv_core::models::activity::{Activity, ActivityName};
use gv_core::queries::AllActivities;
use gv_core::query_executor::QueryExecutor;
use gv_sqlite::{client::SqliteClient, sqlite_executor::SqliteQueryExecutor};
use tokio::runtime::Runtime;
use uuid::Uuid;

use crate::types::{FfiAction, FfiActivity, FfiError, FfiQuery, FfiQueryResult, ffi_action_to_core, parse_uuid};

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

// --- Internal cache types (not exposed via FFI) ---

#[derive(Hash, Eq, PartialEq, Clone)]
enum CacheKey {
    AllActivities,
}

impl From<FfiQuery> for CacheKey {
    fn from(q: FfiQuery) -> Self {
        match q {
            FfiQuery::AllActivities => CacheKey::AllActivities,
        }
    }
}

// Core types are stored in the cache; conversion to FFI types happens only on read_query.
enum CachedResult {
    Activities(Vec<Activity>),
}

struct QueryCache {
    entries: HashMap<CacheKey, CachedResult>,
}

impl QueryCache {
    fn new() -> Self {
        QueryCache {
            entries: HashMap::new(),
        }
    }
}

// --- Shared async helpers ---

/// Run a single query for one CacheKey against the database.
async fn run_query_for_key(client: &SqliteClient, key: &CacheKey) -> Result<CachedResult, FfiError> {
    match key {
        CacheKey::AllActivities => {
            let mut conn = client.pool.acquire().await.map_err(|e| FfiError::Generic(e.to_string()))?;
            let activities = SqliteQueryExecutor::new(&mut *conn)
                .execute(AllActivities {})
                .await
                .map_err(FfiError::from)?;
            Ok(CachedResult::Activities(activities))
        }
    }
}

/// Refresh all keys currently present in the cache.
///
/// Uses std::sync::Mutex; the lock is dropped before each await point so this
/// is safe to call from async contexts.
async fn refresh_subscribed_queries(
    client: &SqliteClient,
    cache: &Mutex<QueryCache>,
) -> Result<(), FfiError> {
    let keys: Vec<CacheKey> = cache.lock().unwrap().entries.keys().cloned().collect();
    for key in keys {
        let result = run_query_for_key(client, &key).await?;
        cache.lock().unwrap().entries.insert(key, result);
    }
    Ok(())
}

// --- QuerySubscription ---

/// Opaque handle returned by `subscribe_query`. Dropping this value (when Swift
/// releases its reference) automatically removes the query from the cache via
/// the Drop impl — no manual unsubscribe call needed.
#[derive(uniffi::Object)]
pub struct QuerySubscription {
    key: CacheKey,
    cache: Arc<Mutex<QueryCache>>,
}

impl Drop for QuerySubscription {
    fn drop(&mut self) {
        if let Ok(mut c) = self.cache.lock() {
            c.entries.remove(&self.key);
        }
    }
}

// --- GainzvilleCore ---

/// The main entry point for Swift. Wraps `SqliteClient` with a static tokio
/// runtime and synchronous UniFFI-exported methods.
#[derive(uniffi::Object)]
pub struct GainzvilleCore {
    client: SqliteClient,
    actor_id: Uuid,
    listener: Arc<dyn CoreListener>,
    cache: Arc<Mutex<QueryCache>>,
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
        Ok(Arc::new(GainzvilleCore {
            client,
            actor_id,
            listener,
            cache: Arc::new(Mutex::new(QueryCache::new())),
        }))
    }

    /// Execute a write action. Synchronous at the FFI boundary; async work runs
    /// on the internal runtime. After the write commits, all subscribed queries
    /// are refreshed in the cache, then `listener.on_data_changed()` is called once.
    pub fn run_action(&self, action: FfiAction) -> Result<(), FfiError> {
        let core_action = ffi_action_to_core(action, self.actor_id)?;
        RUNTIME
            .block_on(self.client.run_action(core_action))
            .map_err(FfiError::from)?;
        RUNTIME.block_on(refresh_subscribed_queries(&self.client, &self.cache))?;
        self.listener.on_data_changed();
        Ok(())
    }

    /// Subscribe to a query. Runs the initial query immediately, populates the
    /// cache, and returns a `QuerySubscription` handle. Dropping the handle
    /// (Swift releasing the reference) auto-removes the query from the cache.
    pub fn subscribe_query(&self, query: FfiQuery) -> Result<Arc<QuerySubscription>, FfiError> {
        let key = CacheKey::from(query);
        let initial = RUNTIME.block_on(run_query_for_key(&self.client, &key))?;
        self.cache.lock().unwrap().entries.insert(key.clone(), initial);
        Ok(Arc::new(QuerySubscription {
            key,
            cache: Arc::clone(&self.cache),
        }))
    }

    /// Read the current cached result for a query. Returns `None` if the query
    /// is not subscribed. Swift calls this synchronously from the main thread
    /// after receiving `on_data_changed()`.
    pub fn read_query(&self, query: FfiQuery) -> Option<FfiQueryResult> {
        let key = CacheKey::from(query);
        self.cache.lock().unwrap().entries.get(&key).map(|r| match r {
            CachedResult::Activities(v) => {
                FfiQueryResult::Activities(v.iter().cloned().map(FfiActivity::from).collect())
            }
        })
    }

    /// Spawn a background task that creates a new activity every 10 seconds.
    /// After each write the cache is refreshed and `on_data_changed()` is fired,
    /// matching the same notification path as `run_action`.
    pub fn start_background_ticker(&self) {
        let client = self.client.clone();
        let actor_id = self.actor_id;
        let cache = Arc::clone(&self.cache);
        let listener = Arc::clone(&self.listener);
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
                if client.run_action(action).await.is_ok() {
                    let _ = refresh_subscribed_queries(&client, &cache).await;
                    listener.on_data_changed();
                }
            }
        });
    }
}

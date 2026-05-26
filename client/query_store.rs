use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use gv_core::{
    error::{DbErr, Result},
    queries::{AnyQuery, AnyQueryResponse, Query},
    query_executor::QueryExecutor,
};
use sqlx::SqlitePool;
use tokio::sync::broadcast;

use gv_sql::sqlite::SqliteQueryExecutor;

/// Shared cache state. `cache` holds the latest result per subscribed query;
/// `refcounts` tracks how many live `QuerySubscription` handles reference each
/// key. A key is evicted only when its last subscriber drops — multiple views
/// can subscribe to the same query (e.g. two views observing the same
/// `FindEntryJoinById`) without one's teardown starving the others.
#[derive(Debug, Default)]
struct CacheState {
    cache: HashMap<AnyQuery, AnyQueryResponse>,
    refcounts: HashMap<AnyQuery, usize>,
}

type SharedCache = Arc<Mutex<CacheState>>;

#[derive(Clone, Debug)]
pub struct QueryStore {
    pool: SqlitePool,
    state: SharedCache,
}

impl QueryStore {
    pub fn new(
        pool: SqlitePool,
        change_transmitter: broadcast::Sender<()>,
        on_cache_ready: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let store = QueryStore {
            pool,
            state: Arc::new(Mutex::new(CacheState::default())),
        };

        // Whenever the database changes re-run all subscribed queries and notify consumers that the
        // cache has been updated.
        let mut rx = change_transmitter.subscribe();
        let thread_store = store.clone();
        let _ = tokio::spawn(async move {
            while let Ok(()) = rx.recv().await {
                let _ = thread_store.refresh_subscribed_queries().await;
                on_cache_ready();
            }
        });
        store
    }

    /// Run a query once, directly against sqlite, and return the result.
    pub async fn run_query<Q: Query>(&self, query: Q) -> Result<Q::Response>
    where
        for<'c> SqliteQueryExecutor<'c>: QueryExecutor<Q>,
    {
        let mut conn = self.pool.acquire().await.db_err()?;
        SqliteQueryExecutor::new(&mut conn).execute(query).await
    }

    /// Type-erased version of run_query.
    pub async fn run_any_query(&self, query: AnyQuery) -> Result<AnyQueryResponse> {
        match query {
            // Auth
            AnyQuery::IsEmailRegistered(q) => Ok(AnyQueryResponse::IsEmailRegistered(
                self.run_query(q).await?,
            )),
            AnyQuery::FindUserById(q) => {
                Ok(AnyQueryResponse::FindUserById(self.run_query(q).await?))
            }
            AnyQuery::FindUserByUsername(q) => Ok(AnyQueryResponse::FindUserByUsername(
                self.run_query(q).await?,
            )),
            AnyQuery::AllActorIds(q) => Ok(AnyQueryResponse::AllActorIds(self.run_query(q).await?)),
            // Activity
            AnyQuery::FindActivityById(q) => {
                Ok(AnyQueryResponse::FindActivityById(self.run_query(q).await?))
            }
            AnyQuery::AllActivities(q) => {
                Ok(AnyQueryResponse::AllActivities(self.run_query(q).await?))
            }
            AnyQuery::FindActivityTemplateRoot(q) => Ok(
                AnyQueryResponse::FindActivityTemplateRoot(self.run_query(q).await?),
            ),
            // Entry
            AnyQuery::AllEntries(q) => Ok(AnyQueryResponse::AllEntries(self.run_query(q).await?)),
            AnyQuery::EntriesRootedInTimeInterval(q) => Ok(
                AnyQueryResponse::EntriesRootedInTimeInterval(self.run_query(q).await?),
            ),
            AnyQuery::FindAncestors(q) => {
                Ok(AnyQueryResponse::FindAncestors(self.run_query(q).await?))
            }
            AnyQuery::FindEntryById(q) => {
                Ok(AnyQueryResponse::FindEntryById(self.run_query(q).await?))
            }
            AnyQuery::FindEntryJoinById(q) => Ok(AnyQueryResponse::FindEntryJoinById(
                self.run_query(q).await?,
            )),
            AnyQuery::FindDescendants(q) => {
                Ok(AnyQueryResponse::FindDescendants(self.run_query(q).await?))
            }
            // Attribute
            AnyQuery::FindAttributeById(q) => Ok(AnyQueryResponse::FindAttributeById(
                self.run_query(q).await?,
            )),
            AnyQuery::AllAttributes(q) => {
                Ok(AnyQueryResponse::AllAttributes(self.run_query(q).await?))
            }
            AnyQuery::FindAttributesByOwner(q) => Ok(AnyQueryResponse::FindAttributesByOwner(
                self.run_query(q).await?,
            )),
            // Value
            AnyQuery::FindValueByKey(q) => {
                Ok(AnyQueryResponse::FindValueByKey(self.run_query(q).await?))
            }
            AnyQuery::FindValuesForEntry(q) => Ok(AnyQueryResponse::FindValuesForEntry(
                self.run_query(q).await?,
            )),
            AnyQuery::FindValuesForEntries(q) => Ok(AnyQueryResponse::FindValuesForEntries(
                self.run_query(q).await?,
            )),
            AnyQuery::FindAttributePairsForEntry(q) => Ok(
                AnyQueryResponse::FindAttributePairsForEntry(self.run_query(q).await?),
            ),
        }
    }

    /// Subscribe to a query. Runs the initial query immediately, populates the
    /// cache, increments the key's refcount, and returns a `QuerySubscription`
    /// handle. Dropping the handle decrements the refcount; the cache entry is
    /// removed only when the last subscriber for that key drops.
    pub async fn subscribe_query(&self, query: AnyQuery) -> Result<Arc<QuerySubscription>> {
        let initial = self.run_any_query(query.clone()).await?;
        {
            let mut state = self.state.lock().unwrap();
            state.cache.insert(query.clone(), initial);
            *state.refcounts.entry(query.clone()).or_insert(0) += 1;
        }
        Ok(Arc::new(QuerySubscription {
            key: query,
            state: Arc::clone(&self.state),
        }))
    }

    /// Read the current cached result for a query. Returns `None` if the query
    /// is not subscribed. Swift calls this synchronously from the main thread
    /// after receiving `on_data_changed()`.
    pub fn read_cached_query(&self, query: AnyQuery) -> Option<AnyQueryResponse> {
        self.state.lock().unwrap().cache.get(&query).cloned()
    }

    /// Refresh all keys currently present in the cache.
    ///
    /// Uses std::sync::Mutex; the lock is dropped before each await point so this
    /// is safe to call from async contexts. A key that was unsubscribed during
    /// an await is not resurrected.
    async fn refresh_subscribed_queries(&self) -> Result<()> {
        let queries: Vec<AnyQuery> = self.state.lock().unwrap().cache.keys().cloned().collect();
        for query in queries {
            let result = self.run_any_query(query.clone()).await?;
            let mut state = self.state.lock().unwrap();
            // Don't resurrect a key whose last subscriber dropped mid-refresh.
            if state.refcounts.contains_key(&query) {
                state.cache.insert(query, result);
            }
        }
        Ok(())
    }
}

/// Opaque handle returned by `subscribe_query`. Dropping this value decrements
/// the key's refcount and removes it from the cache once no subscribers remain —
/// no manual unsubscribe call needed.
pub struct QuerySubscription {
    key: AnyQuery,
    state: SharedCache,
}

impl Drop for QuerySubscription {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(count) = state.refcounts.get_mut(&self.key) {
                *count -= 1;
                if *count == 0 {
                    state.refcounts.remove(&self.key);
                    state.cache.remove(&self.key);
                }
            }
        }
    }
}

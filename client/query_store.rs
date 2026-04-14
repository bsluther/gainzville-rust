use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use gv_core::{
    error::{DomainError, Result},
    queries::{AnyQuery, AnyQueryResponse, Query},
    query_executor::QueryExecutor,
};
use sqlx::SqlitePool;
use tokio::sync::broadcast;

use crate::sqlite_executor::SqliteQueryExecutor;

type QueryCache = Arc<Mutex<HashMap<AnyQuery, AnyQueryResponse>>>;

#[derive(Clone, Debug)]
pub struct QueryStore {
    pool: SqlitePool,
    cache: QueryCache,
}

impl QueryStore {
    pub fn new(
        pool: SqlitePool,
        change_transmitter: broadcast::Sender<()>,
        on_cache_ready: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let cache: QueryCache = Arc::new(Mutex::new(HashMap::new()));
        let store = QueryStore { pool, cache };

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
        let mut conn = self.pool.acquire().await?;
        SqliteQueryExecutor::new(&mut conn).execute(query).await
    }

    /// Type-erased version of run_query.
    pub async fn run_any_query(&self, query: AnyQuery) -> Result<AnyQueryResponse> {
        match query {
            AnyQuery::AllActivities(q) => {
                Ok(AnyQueryResponse::AllActivities(self.run_query(q).await?))
            }
            _ => Err(DomainError::Other("Unimplemented query.".into())),
        }
    }

    /// Subscribe to a query. Runs the initial query immediately, populates the
    /// cache, and returns a `QuerySubscription` handle. Dropping the handle
    /// (Swift releasing the reference) auto-removes the query from the cache.
    pub async fn subscribe_query(&self, query: AnyQuery) -> Result<Arc<QuerySubscription>> {
        let initial = self.run_any_query(query.clone()).await?;
        self.cache.lock().unwrap().insert(query.clone(), initial);
        Ok(Arc::new(QuerySubscription {
            key: query,
            cache: Arc::clone(&self.cache),
        }))
    }

    /// Read the current cached result for a query. Returns `None` if the query
    /// is not subscribed. Swift calls this synchronously from the main thread
    /// after receiving `on_data_changed()`.
    pub fn read_cached_query(&self, query: AnyQuery) -> Option<AnyQueryResponse> {
        self.cache.lock().unwrap().get(&query).cloned()
    }

    /// Refresh all keys currently present in the cache.
    ///
    /// Uses std::sync::Mutex; the lock is dropped before each await point so this
    /// is safe to call from async contexts.
    async fn refresh_subscribed_queries(&self) -> Result<()> {
        let queries: Vec<AnyQuery> = self.cache.lock().unwrap().keys().cloned().collect();
        for query in queries {
            let result = self.run_any_query(query.clone()).await?;
            self.cache.lock().unwrap().insert(query, result);
        }
        Ok(())
    }
}

/// Opaque handle returned by `subscribe_query`. Dropping this value automatically removes the query
/// from the cache via the Drop impl — no manual unsubscribe call needed.
pub struct QuerySubscription {
    key: AnyQuery,
    cache: QueryCache,
}

impl Drop for QuerySubscription {
    fn drop(&mut self) {
        if let Ok(mut c) = self.cache.lock() {
            c.remove(&self.key);
        }
    }
}

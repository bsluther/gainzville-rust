use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use gv_core::{
    error::{DbErr, Result},
    queries::{DomainQuery, DomainQueryResponse, Query},
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
    cache: HashMap<DomainQuery, DomainQueryResponse>,
    refcounts: HashMap<DomainQuery, usize>,
}

type SharedCache = Arc<Mutex<CacheState>>;

/// The legacy ping-and-pull cache, keyed by the domain set (`DomainQuery`). Superseded by
/// `LiveStore` over the client set; retired screen by screen.
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
            // `next_change` treats `Lagged` as "changed" (coalesced) rather than exiting; the old
            // `while let Ok(()) = rx.recv().await` silently stopped refreshing after a burst.
            while next_change(&mut rx).await {
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

    /// Type-erased version of run_query, over the domain set.
    pub async fn run_domain_query(&self, query: DomainQuery) -> Result<DomainQueryResponse> {
        let mut conn = self.pool.acquire().await.db_err()?;
        query
            .execute(&mut SqliteQueryExecutor::new(&mut conn))
            .await
    }

    /// Subscribe to a query. Runs the initial query immediately, populates the
    /// cache, increments the key's refcount, and returns a `QuerySubscription`
    /// handle. Dropping the handle decrements the refcount; the cache entry is
    /// removed only when the last subscriber for that key drops.
    pub async fn subscribe_query(&self, query: DomainQuery) -> Result<Arc<QuerySubscription>> {
        let initial = self.run_domain_query(query.clone()).await?;
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
    pub fn read_cached_query(&self, query: DomainQuery) -> Option<DomainQueryResponse> {
        self.state.lock().unwrap().cache.get(&query).cloned()
    }

    /// Refresh all keys currently present in the cache.
    ///
    /// Uses std::sync::Mutex; the lock is dropped before each await point so this
    /// is safe to call from async contexts. A key that was unsubscribed during
    /// an await is not resurrected.
    async fn refresh_subscribed_queries(&self) -> Result<()> {
        let queries: Vec<DomainQuery> = self.state.lock().unwrap().cache.keys().cloned().collect();
        for query in queries {
            let result = self.run_domain_query(query.clone()).await?;
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
    key: DomainQuery,
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

/// Wait for the next change notification on a broadcast channel. Returns `false` only when the
/// channel is closed.
///
/// `Lagged` means the receiver fell behind and missed notifications; for a "something changed"
/// signal that is still a change, so it returns `true` instead of ending the caller's loop (the
/// previous `while let Ok(()) = rx.recv().await` exited on `Lagged` and silently stopped
/// refreshing). Any further notifications already queued are drained, so a burst coalesces into
/// one refresh. Modeled on convex-rs, which skips lagged messages on purpose.
pub async fn next_change(rx: &mut broadcast::Receiver<()>) -> bool {
    match rx.recv().await {
        Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => {}
        Err(broadcast::error::RecvError::Closed) => return false,
    }
    loop {
        match rx.try_recv() {
            Ok(()) | Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(broadcast::error::TryRecvError::Empty) => return true,
            // Closed after delivering a change: report the change; the next call returns false.
            Err(broadcast::error::TryRecvError::Closed) => return true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn next_change_survives_lag_and_coalesces() {
        let (tx, mut rx) = broadcast::channel::<()>(2);
        // Overflow the channel so the receiver is lagged.
        for _ in 0..10 {
            tx.send(()).unwrap();
        }
        assert!(next_change(&mut rx).await, "Lagged must read as a change");
        // The burst was drained into that one wake-up.
        assert!(matches!(
            rx.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        // Still listening after the lag.
        tx.send(()).unwrap();
        assert!(next_change(&mut rx).await);
        drop(tx);
        assert!(!next_change(&mut rx).await, "Closed ends the loop");
    }
}

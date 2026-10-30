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

    /// Type-erased version of run_query.
    pub async fn run_any_query(&self, query: AnyQuery) -> Result<AnyQueryResponse> {
        let mut conn = self.pool.acquire().await.db_err()?;
        execute_any(&mut SqliteQueryExecutor::new(&mut conn), query).await
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

/// Run any query on an executor. Takes the executor (not a pool) so a caller can run many
/// queries inside one read transaction — `LiveStore` builds each version this way.
pub async fn execute_any(
    exec: &mut SqliteQueryExecutor<'_>,
    query: AnyQuery,
) -> Result<AnyQueryResponse> {
    match query {
        // Auth
        AnyQuery::IsEmailRegistered(q) => {
            Ok(AnyQueryResponse::IsEmailRegistered(exec.execute(q).await?))
        }
        AnyQuery::FindUserById(q) => Ok(AnyQueryResponse::FindUserById(exec.execute(q).await?)),
        AnyQuery::FindUserByUsername(q) => {
            Ok(AnyQueryResponse::FindUserByUsername(exec.execute(q).await?))
        }
        AnyQuery::AllActorIds(q) => Ok(AnyQueryResponse::AllActorIds(exec.execute(q).await?)),
        // Activity
        AnyQuery::FindActivityById(q) => {
            Ok(AnyQueryResponse::FindActivityById(exec.execute(q).await?))
        }
        AnyQuery::AllActivities(q) => Ok(AnyQueryResponse::AllActivities(exec.execute(q).await?)),
        AnyQuery::FindActivityTemplateRoot(q) => Ok(AnyQueryResponse::FindActivityTemplateRoot(
            exec.execute(q).await?,
        )),
        // Entry
        AnyQuery::AllEntries(q) => Ok(AnyQueryResponse::AllEntries(exec.execute(q).await?)),
        AnyQuery::EntriesRootedInTimeInterval(q) => Ok(
            AnyQueryResponse::EntriesRootedInTimeInterval(exec.execute(q).await?),
        ),
        AnyQuery::RootEntriesInInterval(q) => Ok(AnyQueryResponse::RootEntriesInInterval(
            exec.execute(q).await?,
        )),
        AnyQuery::FindAncestors(q) => Ok(AnyQueryResponse::FindAncestors(exec.execute(q).await?)),
        AnyQuery::FindEntryById(q) => Ok(AnyQueryResponse::FindEntryById(exec.execute(q).await?)),
        AnyQuery::FindEntryJoinById(q) => {
            Ok(AnyQueryResponse::FindEntryJoinById(exec.execute(q).await?))
        }
        AnyQuery::FindDescendants(q) => {
            Ok(AnyQueryResponse::FindDescendants(exec.execute(q).await?))
        }
        // Attribute
        AnyQuery::FindAttributeById(q) => {
            Ok(AnyQueryResponse::FindAttributeById(exec.execute(q).await?))
        }
        AnyQuery::AllAttributes(q) => Ok(AnyQueryResponse::AllAttributes(exec.execute(q).await?)),
        AnyQuery::FindAttributesByOwner(q) => Ok(AnyQueryResponse::FindAttributesByOwner(
            exec.execute(q).await?,
        )),
        // Value
        AnyQuery::FindValueByKey(q) => Ok(AnyQueryResponse::FindValueByKey(exec.execute(q).await?)),
        AnyQuery::FindValuesForEntry(q) => {
            Ok(AnyQueryResponse::FindValuesForEntry(exec.execute(q).await?))
        }
        AnyQuery::FindValuesForEntries(q) => Ok(AnyQueryResponse::FindValuesForEntries(
            exec.execute(q).await?,
        )),
        AnyQuery::FindAttributePairsForEntry(q) => Ok(
            AnyQueryResponse::FindAttributePairsForEntry(exec.execute(q).await?),
        ),
        AnyQuery::DistinctTextValuesForAttribute(q) => Ok(
            AnyQueryResponse::DistinctTextValuesForAttribute(exec.execute(q).await?),
        ),
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

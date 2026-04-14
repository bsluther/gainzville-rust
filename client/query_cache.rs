use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use gv_core::queries::{AnyQuery, AnyQueryResponse};

// Note: the hash map does not enforce that entries correctly pair as (Q: Query, Q::Response), that
// pairing must be enforced by the QueryCache and exposed via the public API, eg something like
// `run_query<Q: Query>(query: Q) -> Result<Q::Response>`.
struct QueryCache {
    entries: HashMap<AnyQuery, AnyQueryResponse>,
}

impl QueryCache {
    fn new() -> Self {
        QueryCache {
            entries: HashMap::new(),
        }
    }
}

// --- QuerySubscription ---

/// Opaque handle returned by `subscribe_query`. Dropping this value automatically removes the query
/// from the cache via the Drop impl — no manual unsubscribe call needed.
pub struct QuerySubscription {
    key: AnyQuery,
    cache: Arc<Mutex<QueryCache>>,
}

impl Drop for QuerySubscription {
    fn drop(&mut self) {
        if let Ok(mut c) = self.cache.lock() {
            c.entries.remove(&self.key);
        }
    }
}

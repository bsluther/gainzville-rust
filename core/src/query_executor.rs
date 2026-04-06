use crate::{error::Result, queries::Query};

/// Executes queries against some backing store. Implementations include database-backed executors
/// (wrapping a connection or transaction), in-memory model executors, mock executors, and recording
/// executors.
#[allow(async_fn_in_trait)]
pub trait QueryExecutor {
    async fn execute<Q: Query>(&mut self, query: Q) -> Result<Q::Response>;
}

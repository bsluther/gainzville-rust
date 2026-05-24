//! SQLite backend. Feature-gated behind `sqlite`.

pub mod delta_executor;
pub mod query_executor;

pub use delta_executor::SqliteDeltaExecutor;
pub use query_executor::SqliteQueryExecutor;

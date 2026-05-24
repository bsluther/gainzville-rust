//! Postgres backend. Feature-gated behind `postgres`.

pub mod delta_executor;
pub mod query_executor;

pub use delta_executor::PostgresDeltaExecutor;
pub use query_executor::PostgresQueryExecutor;

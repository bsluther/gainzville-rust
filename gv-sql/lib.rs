//! SQL persistence layer for Gainzville.
//!
//! Owns the DB boundary: `*Column` leaf encoders, `*Row` types that mirror
//! table shapes, `core ↔ Row` transforms, and the per-backend executors.
//! Keeps `gv_core` DB-agnostic.

pub mod columns;
pub mod error;
pub mod rows;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "postgres")]
pub mod postgres;

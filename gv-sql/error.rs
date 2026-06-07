//! Lifting `sqlx::Error` into `DomainError` at the write boundary.
//!
//! Most backend errors are infrastructure failures → `DomainError::Database`.
//! But a *constraint violation* is the database correctly refusing an invalid
//! action: a foreign-key violation means the action referenced a row that
//! doesn't exist. That's a domain rejection, not a bug — a simulator/PBT harness
//! continues past it rather than halting. `sql_err` classifies those and lifts
//! them to `Rejected`; everything else falls back to core's `db_err`
//! (`Database`, with the boundary warning).

use gv_core::error::{DbErr, DomainError, RejectReason, Result};
use sqlx::error::{DatabaseError, ErrorKind};

/// Like [`DbErr`], but inspects the `sqlx::Error` first and reports constraint
/// violations as `Rejected` rather than `Database`. Use at write sites (inserts,
/// updates, deletes) where the DB enforces referential integrity.
pub trait SqlErr<T> {
    fn sql_err(self) -> Result<T>;
}

impl<T> SqlErr<T> for std::result::Result<T, sqlx::Error> {
    fn sql_err(self) -> Result<T> {
        let e = match self {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        if let Some(db) = e.as_database_error() {
            // A foreign-key violation means the action pointed at a missing
            // entity — a correct refusal, surfaced by the DB instead of a
            // mutator pre-check. (Unique violations would map to a future
            // `Conflict`; they stay `Database` until one actually surfaces.)
            if matches!(db.kind(), ErrorKind::ForeignKeyViolation) {
                let detail = constraint_detail(db);
                return Err(DomainError::Rejected(RejectReason::NotFound(detail)));
            }
        }
        // Genuine infra/encoding failure — reuse core's boxing + boundary log.
        Err(e).db_err()
    }
}

fn constraint_detail(db: &dyn DatabaseError) -> String {
    db.constraint()
        .map(str::to_string)
        .unwrap_or_else(|| db.message().to_string())
}
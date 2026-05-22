//! SQLite round-trip tests for `*Column` types.
//!
//! Each test inserts a column value via `Encode` and reads it back via
//! `Decode`, asserting identity. Uses isolated single-column temp tables
//! so each leaf type is tested in isolation, free of schema concerns.

#![cfg(feature = "sqlite")]

use chrono::{TimeZone, Utc};
use fractional_index::FractionalIndex;
use gv_core::models::activity::ActivityName;
use gv_core::validation::{Email, Username};
use gv_sql::columns::{
    ActivityNameColumn, DateTimeColumn, EmailColumn, FractionalIndexColumn, UsernameColumn,
    UuidColumn,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

async fn fresh_pool() -> SqlitePool {
    SqlitePool::connect("sqlite::memory:")
        .await
        .expect("connect to in-memory sqlite")
}

/// Round-trip `value` through a single-column table whose column type is
/// `col_type` (e.g. `"TEXT"`, `"BLOB"`, `"DATETIME"`). Returns the decoded
/// value. Uses `Row::try_get` so the test matches the production read
/// path (`FromRow`-driven, lazy decoding) rather than the stricter tuple
/// `query_as` path.
async fn round_trip<T>(pool: &SqlitePool, col_type: &str, value: T) -> T
where
    T: for<'q> sqlx::Encode<'q, sqlx::Sqlite>
        + for<'r> sqlx::Decode<'r, sqlx::Sqlite>
        + sqlx::Type<sqlx::Sqlite>
        + Send
        + Unpin
        + 'static,
{
    sqlx::query("DROP TABLE IF EXISTS rt").execute(pool).await.unwrap();
    sqlx::query(&format!("CREATE TABLE rt (v {col_type})"))
        .execute(pool)
        .await
        .expect("create table");
    sqlx::query("INSERT INTO rt (v) VALUES (?)")
        .bind(value)
        .execute(pool)
        .await
        .expect("insert");
    let row = sqlx::query("SELECT v FROM rt LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("select");
    // Use `try_get` (with the strict type-compatibility check) — this is
    // what `#[derive(FromRow)]` generates. If a `*Column` type's
    // `compatible()` is stricter than its inner type's, the check fails
    // here, catching the regression at the leaf-test level.
    row.try_get::<T, _>("v").expect("decode")
}

#[tokio::test]
async fn email_column_round_trips() {
    let pool = fresh_pool().await;
    let email = Email::parse("alice@example.com".to_string()).unwrap();
    let got: EmailColumn = round_trip(&pool, "TEXT", EmailColumn(email.clone())).await;
    assert_eq!(got.0, email);
}

#[tokio::test]
async fn username_column_round_trips() {
    let pool = fresh_pool().await;
    let username = Username::parse("alice-1".to_string()).unwrap();
    let got: UsernameColumn = round_trip(&pool, "TEXT", UsernameColumn(username.clone())).await;
    assert_eq!(got.0, username);
}

#[tokio::test]
async fn activity_name_column_round_trips() {
    let pool = fresh_pool().await;
    let name = ActivityName::parse("Bench Press".to_string()).unwrap();
    let got: ActivityNameColumn =
        round_trip(&pool, "TEXT", ActivityNameColumn(name.clone())).await;
    assert_eq!(got.0, name);
}

#[tokio::test]
async fn fractional_index_column_round_trips() {
    let pool = fresh_pool().await;
    let f = FractionalIndex::default();
    let got: FractionalIndexColumn =
        round_trip(&pool, "TEXT", FractionalIndexColumn(f.clone())).await;
    assert_eq!(got.0, f);
}

#[tokio::test]
async fn uuid_column_round_trips() {
    let pool = fresh_pool().await;
    let u = Uuid::new_v4();
    // SQLite stores UUIDs as BLOB, matching the production migration schema.
    let got: UuidColumn = round_trip(&pool, "BLOB", UuidColumn(u)).await;
    assert_eq!(got.0, u);
}

#[tokio::test]
async fn date_time_column_round_trips() {
    let pool = fresh_pool().await;
    let dt = Utc.with_ymd_and_hms(2026, 5, 22, 12, 34, 56).unwrap();
    // Production schema uses TEXT for DateTime columns; sqlx encodes as
    // RFC3339 and reads it back via lazy decoding.
    let got: DateTimeColumn = round_trip(&pool, "TEXT", DateTimeColumn(dt)).await;
    assert_eq!(got.0, dt);
}
//! Property-style round-trip: arbitrary `Entry` through the real SQLite
//! executor pair (write via `SqliteDeltaExecutor`, read via
//! `SqliteQueryExecutor::execute(AllEntries)`).
//!
//! This catches regressions where `EntryRow`'s `FromRow`-derived decode
//! fails for non-`Temporal::None` entries — e.g. when a `*Column` type's
//! `compatible()` is stricter than the underlying sqlx type's, rejecting
//! valid SQL column types.

#![cfg(feature = "sqlite")]

use generation::{Arbitrary, SimulationContext};
use gv_core::{
    SYSTEM_ACTOR_ID,
    delta::Delta,
    delta_executor::DeltaExecutor,
    models::entry::Entry,
    queries::AllEntries,
    query_executor::QueryExecutor,
};
use gv_sql::sqlite::{SqliteDeltaExecutor, SqliteQueryExecutor};
use rand::SeedableRng;
use rand::rngs::ChaCha8Rng;
use sqlx::SqlitePool;
use std::collections::HashMap;

const N: usize = 30;

#[sqlx::test(migrations = "sqlite/migrations")]
async fn arbitrary_entries_round_trip_through_all_entries(pool: SqlitePool) {
    let mut rng = ChaCha8Rng::seed_from_u64(0xb0c0_dabad7e5);
    let context = SimulationContext::default();

    let mut conn = pool.acquire().await.expect("acquire");

    // Build N arbitrary entries as roots owned by SYSTEM_ACTOR_ID so the
    // FKs on owner_id / parent_id are trivially satisfied.
    let mut inserted: HashMap<uuid::Uuid, Entry> = HashMap::with_capacity(N);
    for _ in 0..N {
        let mut entry = Entry::arbitrary(&mut rng, &context);
        entry.owner_id = SYSTEM_ACTOR_ID;
        entry.activity_id = None;
        entry.position = None;
        inserted.insert(entry.id, entry);
    }

    {
        let mut delta_exec = SqliteDeltaExecutor::new(&mut conn);
        for entry in inserted.values() {
            delta_exec
                .apply_delta(Delta::Insert { new: entry.clone() })
                .await
                .expect("insert entry");
        }
    }

    // Read every entry back via AllEntries — this is what feeds the
    // forest cache in the live app.
    let mut query_exec = SqliteQueryExecutor::new(&mut conn);
    let entries = query_exec
        .execute(AllEntries {})
        .await
        .expect("AllEntries query must not fail");

    assert_eq!(
        entries.len(),
        N,
        "all inserted entries should come back through AllEntries"
    );

    for got in entries {
        let original = inserted.get(&got.id).expect("unknown id returned");
        assert_eq!(&got, original, "entry round-trip mismatch");
    }
}

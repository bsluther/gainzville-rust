//! Determinism harness for the arbitrary-action generator.
//!
//! Property under test: with a fixed seed, the *sequence of generated actions*
//! is identical across independent simulation runs. Generation reads back the
//! evolving world state (`context.model()`, fed by the DB via `run_action`), so
//! any nondeterminism in the write/read path — unseeded ids, unordered DB
//! queries, etc. — surfaces here as a run-to-run divergence in the actions.
//!
//! Actions are compared via their `Debug` string (`Action` is not `PartialEq`);
//! this is exact for the plain structs/enums involved and gives a readable diff
//! on failure pinpointing the first divergent action.

use std::sync::Arc;

use generation::{Arbitrary, SimulationContext, io::SimIo};
use gv_core::actions::Action;
use gv_server::server::PostgresServer;
use rand::SeedableRng;
use rand::rngs::ChaCha8Rng;
use sqlx::PgPool;
use tracing::info;

const N_ACTIONS: usize = 1000;
const M_RUNS: usize = 10;

/// Run `n` arbitrary actions against a freshly-wiped database with a seeded
/// `SimIo`, returning the `Debug` rendering of every generated action (including
/// ones the mutator rejects — they're still generated and must be reproducible).
async fn run_sim(pool: &PgPool, seed: u64, n: usize) -> Vec<String> {
    // Start each run from an empty database so the only inputs are the seed.
    sqlx::query(
        "TRUNCATE actors, users, activities, entries, attributes, attribute_values \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("truncate should succeed");

    let server = PostgresServer::with_io(pool.clone(), Arc::new(SimIo::new(seed)));
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut context = SimulationContext::default();

    let mut actions = Vec::with_capacity(n);
    for _ in 0..n {
        let action = Action::arbitrary(&mut rng, &context);
        actions.push(format!("{action:?}"));
        match server.run_action(action).await {
            Ok(mx) => context.apply_mutation(mx).await.unwrap(),
            Err(_) => continue, // a rejected (invalid) action is still deterministic input
        }
    }
    actions
}

#[sqlx::test(migrations = "../gv-sql/postgres/migrations")]
async fn test_generation_is_deterministic(pool: PgPool) {
    let seed: u64 = rand::random();
    info!("seed={seed} n_actions={N_ACTIONS} m_runs={M_RUNS}");

    let baseline = run_sim(&pool, seed, N_ACTIONS).await;

    for run in 1..M_RUNS {
        let actions = run_sim(&pool, seed, N_ACTIONS).await;
        for (i, (expected, got)) in baseline.iter().zip(actions.iter()).enumerate() {
            assert_eq!(
                expected, got,
                "run {run} diverged from the baseline at action {i} (seed={seed})",
            );
        }
        assert_eq!(
            baseline.len(),
            actions.len(),
            "run {run} produced a different number of actions",
        );
    }
}

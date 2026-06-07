use std::sync::Arc;

use fractional_index::FractionalIndex;
use generation::{Arbitrary, GenerationContext, SimulationContext, io::SimIo, model::Model};
use gv_core::{
    actions::{Action, CreateUser, MoveEntry},
    error::{DomainError, RejectReason},
    models::entry::{Entry, Position, Temporal},
    queries::SnapshotAll,
    query_executor::QueryExecutor,
};
use gv_server::server::PostgresServer;
use gv_sql::postgres::PostgresQueryExecutor;
use rand::rngs::ChaCha8Rng;
use rand::{RngExt, SeedableRng};
use sqlx::PgPool;
use tracing::info;

#[sqlx::test(migrations = "../gv-sql/postgres/migrations")]
async fn test_move_entry_disallows_cycles(pool: PgPool) {
    let server = PostgresServer::new(pool);
    let mut rng = rand::rng();
    let context = SimulationContext::default();

    let create_user = CreateUser::arbitrary(&mut rng, &context);
    let actor_id = create_user.user.actor_id.clone();
    server.run_action(create_user.into()).await.unwrap();

    let mut entry_a = Entry::arbitrary(&mut rng, &context);
    entry_a.owner_id = actor_id;
    entry_a.is_sequence = true;
    entry_a.is_template = true;
    entry_a.activity_id = None;
    entry_a.position = None;
    // Templates carry no start/end (arbitrary may produce one).
    entry_a.temporal = Temporal::None;

    let mut entry_b = Entry::arbitrary(&mut rng, &context);
    entry_b.owner_id = actor_id;
    entry_b.is_sequence = true;
    entry_b.is_template = true;
    entry_b.activity_id = None;
    entry_b.position = Some(Position {
        parent_id: entry_a.id,
        frac_index: FractionalIndex::default(),
    });
    entry_b.temporal = Temporal::None;

    let move_action = Action::MoveEntry(MoveEntry {
        actor_id,
        entry_id: entry_a.id,
        position: Some(Position {
            parent_id: entry_a.id,
            frac_index: FractionalIndex::default(),
        }),
        temporal: Temporal::None,
    });

    server
        .run_action(Action::CreateEntry(entry_a.into()))
        .await
        .unwrap();
    server
        .run_action(Action::CreateEntry(entry_b.into()))
        .await
        .unwrap();
    let result = server.run_action(move_action).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(
        err,
        DomainError::Rejected(RejectReason::Precondition(_))
    ));
}

#[sqlx::test(migrations = "../gv-sql/postgres/migrations")]
async fn test_arbitrary_create_user(pool: PgPool) {
    let server = PostgresServer::new(pool);
    let mut rng = rand::rng();
    let context = SimulationContext::default();

    let action: Action = CreateUser::arbitrary(&mut rng, &context).into();

    server
        .run_action(action)
        .await
        .expect("create_user action should succeed");
}

#[sqlx::test(migrations = "../gv-sql/postgres/migrations")]
async fn test_arbitrary_actions(pool: PgPool) {
    use tracing_subscriber::{
        filter::{LevelFilter, Targets},
        fmt,
        prelude::*,
    };
    // Only this test's own logs; silence the internal crates' span/event spam.
    let filter = Targets::new()
        .with_target("postgres_tests", LevelFilter::INFO)
        .with_default(LevelFilter::ERROR);
    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_test_writer())
        .with(filter)
        .try_init();

    // let seed: u64 = 15287082126695428488;
    let seed: u64 = rand::random();
    info!("seed={}", seed);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let server = PostgresServer::with_io(pool, Arc::new(SimIo::new(rng.random())));
    let mut context = SimulationContext::default();

    for i in 0..1_000 {
        // The error taxonomy now tells correct refusals apart from bugs: a
        // `Rejected` is the system correctly disallowing an invalid action
        // (expected while fuzzing — log and continue), whereas an
        // `InvariantViolation` (observed corruption) or `Database` (backend
        // failure) is a real defect and fails the test. Still open: whether a
        // given `Rejected` was *itself* correct (vs over-rejecting a valid
        // action) — that needs a `valid_distribution` knob on generation.
        let action = Action::arbitrary(&mut rng, &context);
        let kind = action_kind(&action);
        let mx = match server.run_action(action.clone()).await {
            Ok(mx) => {
                info!(?i, "ok     {kind}");
                mx
            }
            // Correctly refused — keep fuzzing. No catch-all below, so a new
            // `DomainError` variant forces a deliberate decision here.
            Err(DomainError::Rejected(reason)) => {
                info!(?i, "reject {kind}: {reason}");
                continue;
            }
            Err(DomainError::InvariantViolation { invariant, context }) => {
                panic!("invariant violated by {kind} (seed={seed}): {invariant} ({context})");
            }
            Err(DomainError::Database(e)) => {
                panic!("database error on {kind} (seed={seed}): {e}");
            }
        };
        context.apply_mutation(mx).await.unwrap();

        // Check the model matches the current state of the database.
        let mut conn = server.pool.acquire().await.unwrap();
        let snapshot = PostgresQueryExecutor::new(&mut conn)
            .execute(SnapshotAll)
            .await
            .expect("snapshot should not fail");
        let snapshot_model = Model::from_snapshot(snapshot);
        assert_eq!(
            context.model(),
            &snapshot_model,
            "model diverged from database after {kind}"
        );
    }
}

/// Short variant name for an `Action`, for scannable per-action logging.
fn action_kind(action: &Action) -> &'static str {
    match action {
        Action::CreateUser(_) => "CreateUser",
        Action::CreateActivity(_) => "CreateActivity",
        Action::CreateAttribute(_) => "CreateAttribute",
        Action::CreateValue(_) => "CreateValue",
        Action::AttachValue(_) => "AttachValue",
        Action::DeleteAttributeValue(_) => "DeleteAttributeValue",
        Action::CreateEntry(_) => "CreateEntry",
        Action::CreateEntryFromActivity(_) => "CreateEntryFromActivity",
        Action::DeleteEntryRecursive(_) => "DeleteEntryRecursive",
        Action::MoveEntry(_) => "MoveEntry",
        Action::UpdateEntryCompletion(_) => "UpdateEntryCompletion",
        Action::UpdateAttributeValue(_) => "UpdateAttributeValue",
        Action::UpdateAttribute(_) => "UpdateAttribute",
        Action::UpdateEntry(_) => "UpdateEntry",
    }
}

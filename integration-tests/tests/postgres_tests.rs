use std::sync::Arc;

use fractional_index::FractionalIndex;
use generation::{Arbitrary, GenerationContext, SimulationContext, io::SimIo, model::Model};
use gv_core::{
    actions::{Action, CreateActivity, CreateEntry, CreateUser, MoveEntry},
    models::entry::{Entry, Position, Temporal},
    queries::{AllActorIds, SnapshotAll},
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
    let mut tx = server
        .pool
        .begin()
        .await
        .expect("begin transaction should not fail");

    let mut rng = rand::rng();
    let context = SimulationContext::default();

    let actor_ids = PostgresQueryExecutor::new(&mut *tx)
        .execute(AllActorIds {})
        .await
        .unwrap();
    let actor_id = actor_ids[0];

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
    assert!(matches!(err, gv_core::error::DomainError::Consistency(_)));
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
async fn test_arbitrary_create_entry(pool: PgPool) {
    // YOU ARE HERE
    // Fixed up the test, but it fails with eg called `Result::unwrap()` on an `Err` value: Consistency("child entry must match its parent's template/log kind").
    // Problem: that's the system working as it should and preventing an invalid parenting. But how
    // do we differentiate between a correct failure and incorrect one?
    // We need properties...
    let server = PostgresServer::new(pool);
    let mut rng = rand::rng();
    let mut context = SimulationContext::default();

    let mx = server
        .run_action(CreateUser::arbitrary(&mut rng, &context).into())
        .await
        .unwrap();
    context.apply_mutation(mx).await.unwrap();

    for _ in 0..100 {
        let mx = server
            .run_action(CreateActivity::arbitrary(&mut rng, &context).into())
            .await
            .unwrap();
        context.apply_mutation(mx).await.unwrap();
    }

    for _ in 0..100 {
        let mx = server
            .run_action(CreateEntry::arbitrary(&mut rng, &context).into())
            .await
            .unwrap();
        context.apply_mutation(mx).await.unwrap();
    }
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
        .with_target("generation::entry", LevelFilter::DEBUG)
        .with_default(LevelFilter::ERROR);
    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_test_writer())
        .with(filter)
        .try_init();

    let seed: u64 = 15287082126695428488; // random();
    info!("seed={}", seed);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let server = PostgresServer::with_io(pool, Arc::new(SimIo::new(rng.random())));
    let mut context = SimulationContext::default();

    for i in 0..500 {
        // Problem: running a MoveEntry action which tries to move into a non-sequence entry fails
        // as it should, but here that looks like a failure. I want to test that the system can
        // handle invalid inputs, so it seems that I need a way to differentiate between correct
        // errors and incorrect errors. But then, how do I determine if I correctly disallowed
        // some action, or incorrectly diallowed some action?
        // - Solution: parameterize the generation, valid_distribution: 0..1 where 0 is all invalid
        // and 1 is all valid.

        let action = Action::arbitrary(&mut rng, &context);
        let kind = action_kind(&action);
        info!(?i, ?action);
        let mx = match server.run_action(action.clone()).await {
            Ok(mx) => {
                info!("ok   {kind}");
                mx
            }
            Err(e) => {
                info!("FAIL {kind}: {e}");
                continue;
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

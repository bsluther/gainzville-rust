use fractional_index::FractionalIndex;
use generation::{Arbitrary, ArbitraryFrom, SimulationContext};
use gv_core::{
    actions::{Action, CreateActivity, CreateEntry, CreateUser, MoveEntry},
    models::{
        activity::Activity,
        entry::{Entry, Position, Temporal},
    },
    reader::Reader,
};
use gv_postgres::{reader::PostgresReader, server::PostgresServer};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sqlx::PgPool;
use tracing::info;

#[sqlx::test(migrations = "../postgres/migrations")]
async fn test_move_entry_disallows_cycles(pool: PgPool) {
    let server = PostgresServer::new(pool);
    let mut tx = server
        .pool
        .begin()
        .await
        .expect("begin transaction should not fail");

    let mut rng = rand::rng();
    let context = SimulationContext {};

    let actor_ids = PostgresReader::all_actor_ids(&mut *tx).await.unwrap();
    let actor_id = actor_ids[0];

    let mut entry_a = Entry::arbitrary(&mut rng, &context);
    entry_a.owner_id = actor_id;
    entry_a.is_sequence = true;
    entry_a.is_template = true;
    entry_a.activity_id = None;
    entry_a.position = None;

    let mut entry_b = Entry::arbitrary(&mut rng, &context);
    entry_b.owner_id = actor_id;
    entry_b.is_sequence = true;
    entry_b.is_template = true;
    entry_b.activity_id = None;
    entry_b.position = Some(Position {
        parent_id: entry_a.id,
        frac_index: FractionalIndex::default(),
    });

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

#[sqlx::test(migrations = "../postgres/migrations")]
async fn test_arbitrary_create_user(pool: PgPool) {
    let server = PostgresServer::new(pool);
    let mut rng = rand::rng();
    let context = SimulationContext {};

    let action: Action = CreateUser::arbitrary(&mut rng, &context).into();

    server
        .run_action(action)
        .await
        .expect("create_user action should succeed");
}

#[sqlx::test(migrations = "../postgres/migrations")]
async fn test_arbitrary_create_entry(pool: PgPool) {
    let server = PostgresServer::new(pool);
    let mut tx = server
        .pool
        .begin()
        .await
        .expect("begin transaction should not fail");
    let mut rng = rand::rng();
    let context = SimulationContext {};

    let actor_ids = PostgresReader::all_actor_ids(&mut *tx).await.unwrap();
    let activities = (0..100)
        .map(|_| Activity::arbitrary_from(&mut rng, &context, &actor_ids))
        .collect::<Vec<_>>();
    let entries = (0..100).fold(vec![], |mut acc, _| {
        let entry = Entry::arbitrary_from(&mut rng, &context, (&actor_ids, &activities, &acc));
        acc.push(entry);
        acc
    });

    for activity in activities {
        let create_activity: CreateActivity = activity.into();
        server.run_action(create_activity.into()).await.unwrap();
    }

    for entry in entries {
        let create_entry: CreateEntry = entry.into();
        server.run_action(create_entry.into()).await.unwrap();
    }
}

#[sqlx::test(migrations = "../postgres/migrations")]
async fn test_arbitrary_actions(pool: PgPool) {
    let _ = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::WARN)
        .with_test_writer()
        .try_init();

    let seed: u64 = 15287082126695428488; // random();
    info!("seed={}", seed);
    let server = PostgresServer::new(pool);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let context = SimulationContext {};

    for _ in 0..1_000 {
        let actor_ids = PostgresReader::all_actor_ids(&server.pool).await.unwrap();
        let activities = PostgresReader::all_activities(&server.pool).await.unwrap();
        let entries = PostgresReader::all_entries(&server.pool).await.unwrap();
        let action =
            Action::arbitrary_from(&mut rng, &context, (&actor_ids, &activities, &entries));
        // info!("Running action:\n{:?}", action);

        // Problem: running a MoveEntry action which tries to move into a non-sequence entry fails
        // as it should, but here that looks like a failure. I want to test that the system can
        // handle invalid inputs, so it seems that I need a way to differentiate between correct
        // errors and incorrect errors. But then, how do I determine if I correctly disallowed
        // some action, or incorrectly diallowed some action?
        // - Solution: parameterize the generation, valid_distribution: 0..1 where 0 is all invalid
        // and 1 is all valid.

        let _result = server.run_action(action.clone()).await;
    }
}

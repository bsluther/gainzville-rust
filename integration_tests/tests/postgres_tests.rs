use fractional_index::FractionalIndex;
use generation::{Arbitrary, ArbitraryFrom, SimulationContext};
use gv_core::{
    actions::{Action, CreateActivity, CreateEntry, MoveEntry},
    models::{
        activity::Activity,
        entry::{Entry, Position, Temporal},
    },
    repos::AuthnRepo,
};
use gv_postgres::{controller::PgController, repos::PgContext};
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrations = "../postgres/migrations")]
async fn test_move_entry_no_cycles(pool: PgPool) {
    let pg_controller = PgController { pool: pool.clone() };
    let mut tx = pg_controller
        .pool
        .begin()
        .await
        .expect("begin transaction should not fail");
    let mut repo = PgContext::new(&mut tx);
    let mut rng = rand::rng();
    let context = SimulationContext {};

    let actor_ids = repo.all_actor_ids().await.unwrap();
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
    // YOU ARE HERE
    // Need to make this better.
    // 1. Should be able to run in the same transaction, althought this is less critical.
    // 2. Provide a run_action_commit to deal with all the calls.
    pg_controller
        .run_action(Action::CreateEntry(entry_a.into()))
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    pg_controller
        .run_action(Action::CreateEntry(entry_b.into()))
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let result = pg_controller.run_action(move_action).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, gv_core::error::DomainError::Consistency(_)));
}

#[sqlx::test(migrations = "../postgres/migrations")]
async fn test_arbitrary_create_entry(pool: PgPool) {
    let pg_controller = PgController { pool: pool.clone() };
    let mut tx = pg_controller
        .pool
        .begin()
        .await
        .expect("begin transaction should not fail");
    let mut repo = PgContext::new(&mut tx);

    let mut rng = rand::rng();
    let context = SimulationContext {};

    let actor_ids = repo.all_actor_ids().await.unwrap();
    let activities = (0..100)
        .map(|_| Activity::arbitrary_from(&mut rng, &context, &actor_ids))
        .collect();
    let entries = (0..100).fold(vec![], |mut acc, _| {
        let entry = Entry::arbitrary_from(&mut rng, &context, (&activities, &acc));
        acc.push(entry);
        acc
    });

    for activity in activities {
        let create_activity: CreateActivity = activity.into();
        let _tx = pg_controller
            .run_action(create_activity.into())
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
    }

    for entry in entries {
        let create_entry: CreateEntry = entry.into();
        let _tx = pg_controller
            .run_action(create_entry.into())
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
    }
}

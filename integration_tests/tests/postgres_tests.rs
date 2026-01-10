use generation::{ArbitraryFrom, SimulationContext};
use gv_core::{
    actions::{CreateActivity, CreateEntry},
    models::{activity::Activity, entry::Entry},
    repos::AuthnRepo,
};
use gv_postgres::{controller::PgController, repos::PgContext};
use sqlx::PgPool;

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

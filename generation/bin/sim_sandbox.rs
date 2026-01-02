use std::env;

use gv_core::core::{
    actions::{CreateActivity, CreateEntry},
    models::{activity::Activity, entry::Entry},
    repos::AuthnRepo,
};
use gv_postgres::{controller::PgController, repos::PgContext};
use generation::{ArbitraryFrom, GenerationContext, SimulationContext, gen_random_text};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let db_url = env::var("DATABASE_URL").expect("Database URL must be set in env.");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    let pg_controller = PgController { pool: pool.clone() };
    let mut tx = pg_controller.pool.begin().await?;
    let mut repo = PgContext::new(&mut tx);

    let mut rng = rand::rng();
    let context = SimulationContext {};
    let actor_ids = repo.all_actor_ids().await?;

    let activities = (0..100)
        .map(|_| Activity::arbitrary_from(&mut rng, &context, &actor_ids))
        .collect();
    let entries = (0..100).fold(vec![], |mut acc, _| {
        let entry = Entry::arbitrary_from(&mut rng, &context, (&activities, &acc));
        acc.push(entry);
        acc
    });

    // TODO: pick one of the generated activities
    // let activity = activities[0].clone();
    // let entry = Entry::arbitrary_from(&mut rng, &context, (&vec![activity.clone()], &vec![]));
    // println!("{:?}", activity);

    // let create_activity: CreateActivity = activity.clone().into();
    // let create_entry: CreateEntry = entry.clone().into();

    for activity in activities {
        let create_activity: CreateActivity = activity.into();
        pg_controller
            .run_action(create_activity.into())
            .await?
            .commit()
            .await?;
    }

    for entry in entries {
        let create_entry: CreateEntry = entry.into();
        pg_controller
            .run_action(create_entry.into())
            .await?
            .commit()
            .await?;
    }

    Ok(())
}

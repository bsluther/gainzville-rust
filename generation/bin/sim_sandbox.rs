use std::env;

use generation::{ArbitraryFrom, SimulationContext};
use gv_core::{
    actions::{CreateActivity, CreateEntry},
    models::{activity::Activity, entry::Entry},
    reader::Reader,
};
use gv_postgres::{reader::PostgresReader, server::PostgresServer};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let db_url = env::var("DATABASE_URL").expect("Database URL must be set in env.");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;
    let server = PostgresServer::new(pool);

    let mut rng = rand::rng();
    let context = SimulationContext {};
    let actor_ids = PostgresReader::all_actor_ids(&server.pool).await?;

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
        let _tx = server.run_action(create_activity.into()).await?;
    }

    for entry in entries {
        let create_entry: CreateEntry = entry.into();
        let _tx = server.run_action(create_entry.into()).await?;
    }

    Ok(())
}

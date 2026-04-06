use std::env;

use generation::{ArbitraryFrom, SimulationContext};
use gv_core::{
    actions::{CreateActivity, CreateEntry},
    models::{activity::Activity, entry::Entry},
    queries::AllActorIds,
    query_executor::QueryExecutor,
};
use gv_postgres::{postgres_executor::PostgresQueryExecutor, server::PostgresServer};
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
    let context = SimulationContext::default();
    let actor_ids = {
        let mut connection = server.pool.acquire().await?;
        PostgresQueryExecutor::new(&mut *connection).execute(AllActorIds {}).await?
    };

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

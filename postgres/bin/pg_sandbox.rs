use std::env;

use gv_core::{
    actions::{Action, CreateEntry, CreateUser},
    models::{
        activity::{Activity, ActivityName},
        entry::{Entry, Temporal},
        user::User,
    },
    validation::{Email, Username},
};
use gv_postgres::controller::PgController;
use sqlx::postgres::PgPoolOptions;
use tracing::{Level, info, span};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt().pretty().init();
    let span = span!(Level::INFO, "main");
    let _guard = span.enter();

    let db_url = env::var("TEST_DATABASE_URL").expect("Database URL must be set in env.");

    info!(db_url = db_url, "Connecting to database");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    let pg_controller = PgController { pool: pool.clone() };

    let new_user_id = Uuid::new_v4();
    let new_user = User {
        actor_id: new_user_id.clone(),
        username: Username::parse("sandbox_test3".to_string())?,
        email: Email::parse("sandbox3@test.com".to_string())?,
    };

    info!(
        actor_id = new_user_id.to_string(),
        "Attempting to create user"
    );
    match pg_controller
        .run_action(Action::CreateUser(CreateUser { user: new_user }))
        .await
    {
        Ok(tx) => {
            println!("Handle create user succeeded!");
            tx.commit().await?;
        }
        Err(e) => {
            println!("Error in handle_create_user: {e}");
            return Ok(());
        }
    };

    // Create Pull Up activity
    let pull_up_id = Uuid::new_v4();
    let activity = Activity {
        id: pull_up_id,
        owner_id: new_user_id,
        source_activity_id: None,
        name: ActivityName::parse("Pull Up".to_string()).unwrap(),
        description: Some("Pull yourself up.".to_string()),
    };

    let tx = pg_controller
        .run_action(Action::CreateActivity(activity.into()))
        .await?;
    tx.commit().await?;

    let create_entry = CreateEntry {
        actor_id: new_user_id.clone(),
        entry: Entry {
            id: Uuid::new_v4(),
            activity_id: Some(pull_up_id.clone()),
            owner_id: new_user_id.clone(),
            position: None,
            is_template: false,
            display_as_sets: false,
            is_sequence: false,
            temporal: Temporal::None,
        },
    };
    let tx = pg_controller
        .run_action(Action::CreateEntry(create_entry))
        .await?;
    tx.commit().await?;

    Ok(())
}

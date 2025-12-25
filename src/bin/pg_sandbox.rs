use std::env;

use gv_rust_2025_12::{
    core::{
        models::user::User,
        validation::{Email, Username},
    },
    postgres::controller::PgController,
};
use sqlx::postgres::PgPoolOptions;
use tracing::{Level, info, span};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let new_id = Uuid::new_v4();
    let new_user = User {
        actor_id: new_id,
        username: Username::parse("sandbox_test6".to_string())?,
        email: Email::parse("sandbox6@test.com".to_string())?,
    };

    info!(actor_id = new_id.to_string(), "Attempting to create user");
    match pg_controller.handle_create_user(new_user).await {
        Ok(_) => {
            println!("Handle create user succeeded!");
        }
        Err(e) => {
            println!("Error in handle_create_user: {e}");
            return Ok(());
        }
    };

    println!("🔎 Verifying in Database...");
    let row = sqlx::query!(
        "SELECT username, email FROM users WHERE actor_id = $1",
        new_id
    )
    .fetch_optional(&pool)
    .await?;

    match row {
        Some(r) => println!(
            "🎉 FOUND: User '{}' is stored safely in Postgres.",
            r.username
        ),
        None => println!(
            "⚠️  WARNING: Workflow succeeded, but user was not found in DB! Check transaction commits."
        ),
    }
    Ok(())
}

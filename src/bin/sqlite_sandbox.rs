use std::env;

use gv_rust_2025_12::{
    core::{
        models::user::User,
        validation::{Email, Username},
    },
    sqlite::controller::SqliteController,
};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_url = env::var("SQLITE_DATABASE_URL").unwrap_or_else(|_| "sqlite:test.db".to_string());

    println!("Connecting to {}", db_url);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;
    let sqlite_controller = SqliteController { pool: pool.clone() };

    let new_id = Uuid::new_v4();
    let new_user = User {
        actor_id: new_id,
        username: Username::parse("sandbox_test".to_string())?,
        email: Email::parse("sandbox@test.com".to_string())?,
    };

    println!("🚀 Attempting to create user: {}", new_id);
    match sqlite_controller.handle_create_user(new_user).await {
        Ok(_) => {
            println!("Handle create user succeeded!");
        }
        Err(e) => {
            println!("Error in handle_create_user: {e}");
            return Ok(());
        }
    };

    println!("🔎 Verifying in Database...");
    let row = sqlx::query("SELECT username, email FROM users WHERE actor_id = ?")
        .bind(new_id.to_string())
        .fetch_optional(&pool)
        .await?;

    match row {
        Some(r) => {
            let username: String = r.try_get("username")?;
            println!(
                "🎉 FOUND: User '{}' is stored safely in SQLite.",
                username
            );
        }
        None => println!(
            "⚠️  WARNING: Workflow succeeded, but user was not found in DB! Check transaction commits."
        ),
    }
    Ok(())
}

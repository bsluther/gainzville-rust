#[path = "postgres/common.rs"]
mod common;

use gv_rust_2025_12::{
    core::{
        models::user::User,
        validation::{Email, Username},
    },
    postgres::controller::PgController,
};
use uuid::Uuid;

#[tokio::test]
async fn test_create_user() {
    let pool = common::setup_clear_database().await;
    let pg_controller = PgController { pool: pool.clone() };

    let new_id = Uuid::new_v4();
    let username = "test_user";
    let email = "test@example.com";
    let new_user = User {
        actor_id: new_id,
        username: Username::parse(username.to_string()).expect("Invalid username"),
        email: Email::parse(email.to_string()).expect("Invalid email"),
    };

    pg_controller
        .handle_create_user(new_user)
        .await
        .expect("Failed to create user");

    let row = sqlx::query!(
        "SELECT username, email FROM users WHERE actor_id = $1",
        new_id
    )
    .fetch_optional(&pool)
    .await
    .expect("Failed to query test user");

    assert!(row.is_some(), "User should exist in database");
    let row = row.unwrap();
    assert_eq!(
        row.username,
        username.to_string(),
        "Username should match created user"
    );
    assert_eq!(
        row.email,
        email.to_string(),
        "Email should match created user"
    )
}

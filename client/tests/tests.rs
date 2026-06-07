use sqlx::SqlitePool;

use gv_client::client::SqliteClient;
use gv_core::{
    actions::{Action, CreateUser},
    models::user::User,
    validation::{Email, Username},
};

use uuid::Uuid;

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_create_user_roundtrip(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));

    let new_id = Uuid::new_v4();
    let new_user = User {
        actor_id: new_id,
        username: Username::parse("sandbox_test".to_string()).unwrap(),
        email: Email::parse("sandbox@test.com".to_string()).unwrap(),
    };

    sqlite_client
        .run_action(Action::CreateUser(CreateUser { user: new_user }))
        .await
        .unwrap();

    sqlx::query("SELECT username, email FROM users WHERE actor_id = ?")
        .bind(new_id)
        .fetch_optional(&sqlite_client.pool)
        .await
        .unwrap()
        .expect("created user should exist in database");
}

#[path = "postgres/common.rs"]
mod common;
#[macro_use]
mod macros;
use std::str::FromStr;

use gv_core::{
    core::{
        actions::{Action, CreateActivity, CreateUser},
        models::{
            activity::{Activity, ActivityName},
            user::User,
        },
        validation::{Email, Username},
    },
    postgres::controller::PgController,
};
use proptest::{prelude::*, test_runner::FileFailurePersistence};
use test_strategy::proptest;
use uuid::Uuid;

const SYSTEM_ACTOR_ID: &str = "eee9e6ae-6531-4580-8356-427604a0dc02";

#[proptest(
    async = "tokio",
    ProptestConfig::with_failure_persistence(FileFailurePersistence::WithSource("regressions")),
    cases = 128
)]
async fn test_create_activity_ts(
    #[strategy(Activity::arbitrary_with(Uuid::from_str(SYSTEM_ACTOR_ID).unwrap()))]
    activity: Activity,
) {
    let pool = common::setup_test_pool().await;
    let pg_controller = PgController { pool: pool.clone() };

    let action = Action::CreateActivity(activity.clone().into());
    let mut tx = pg_controller
        .run_action(action)
        .await
        .expect("Failed to create activity");

    let row = sqlx::query!(
        "SELECT name, description FROM activities WHERE id = $1",
        activity.id
    )
    .fetch_optional(&mut *tx)
    .await
    .expect("Failed to query test activity");

    assert!(row.is_some(), "Activity should exist in database");
    let row = row.unwrap();
    assert_eq!(
        row.name,
        activity.name.to_string(),
        "Name should match created activity"
    );
    assert_eq!(
        row.description, activity.description,
        "Description should match created activity"
    );
}

#[proptest(
    async = "tokio",
    ProptestConfig::with_failure_persistence(FileFailurePersistence::WithSource("regressions")),
    cases = 128
)]
async fn test_create_user_ts(user: User) {
    let pool = common::setup_test_pool().await;
    let pg_controller = PgController { pool: pool.clone() };

    let mut tx = pg_controller
        .run_action(Action::CreateUser(CreateUser { user: user.clone() }))
        .await
        .expect("Failed to create user");

    let row = sqlx::query!(
        "SELECT username, email FROM users WHERE actor_id = $1",
        user.actor_id
    )
    .fetch_optional(&mut *tx)
    .await
    .expect("Failed to query test user");

    assert!(row.is_some(), "User should exist in database");
    let row = row.unwrap();
    assert_eq!(
        row.username,
        user.username.as_str().to_string(),
        "Username should match created user"
    );
    assert_eq!(
        row.email,
        user.email.as_str().to_string(),
        "Email should match created user"
    );
}

#[tokio::test]
async fn test_create_user() {
    let pool = common::setup_test_pool().await;
    let pg_controller = PgController { pool: pool.clone() };

    let new_id = Uuid::new_v4();
    let username = "test_user";
    let email = "test@example.com";
    let new_user = User {
        actor_id: new_id,
        username: Username::parse(username.to_string()).expect("Invalid username"),
        email: Email::parse(email.to_string()).expect("Invalid email"),
    };

    let mut tx = pg_controller
        .run_action(Action::CreateUser(CreateUser { user: new_user }))
        .await
        .expect("Failed to create user");

    let row = sqlx::query!(
        "SELECT username, email FROM users WHERE actor_id = $1",
        new_id
    )
    .fetch_optional(&mut *tx)
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
    );
}

#[tokio::test]
async fn test_create_activity() {
    let pool = common::setup_test_pool().await;
    let pg_controller = PgController { pool: pool.clone() };

    let name = "Test activity";
    let description = "This is a test activity";
    let act_id = Uuid::new_v4();
    let system_actor_id = Uuid::from_str(SYSTEM_ACTOR_ID).unwrap();
    let activity = Activity {
        id: act_id,
        owner_id: system_actor_id,
        source_activity_id: None,
        name: ActivityName::parse(name.to_string()).unwrap(),
        description: Some(description.to_string()),
    };
    let action = Action::CreateActivity(activity.into());
    let mut tx = pg_controller
        .run_action(action)
        .await
        .expect("Failed to create activity");

    let row = sqlx::query!(
        "SELECT name, description FROM activities WHERE id = $1",
        act_id
    )
    .fetch_optional(&mut *tx)
    .await
    .expect("Failed to query test activity");

    assert!(row.is_some(), "Activity should exist in database");
    let row = row.unwrap();
    assert_eq!(
        row.name,
        name.to_string(),
        "Name should match created activity"
    );
    assert_eq!(
        row.description,
        Some(description.to_string()),
        "Description should match created activity"
    );
}

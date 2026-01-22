use gv_core::{
    actions::{Action, CreateEntry, CreateUser},
    models::{
        activity::{Activity, ActivityName},
        entry::{Entry, Temporal},
        user::User,
    },
    validation::{Email, Username},
};

use gv_postgres::server::PostgresServer;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrations = "./migrations")]
async fn test_create_user_activity_entry(pool: PgPool) {
    let postgres_server = PostgresServer::new(pool);

    // Create user
    let user_id = Uuid::new_v4();
    let user = User {
        actor_id: user_id,
        username: Username::parse("sandbox_test".to_string()).unwrap(),
        email: Email::parse("sandbox@test.com".to_string()).unwrap(),
    };
    postgres_server
        .run_action(Action::CreateUser(CreateUser { user }))
        .await
        .unwrap();

    // Create activity
    let activity_id = Uuid::new_v4();
    let activity = Activity {
        id: activity_id,
        owner_id: user_id,
        source_activity_id: None,
        name: ActivityName::parse("Pull Up".to_string()).unwrap(),
        description: Some("Pull yourself up.".to_string()),
    };
    postgres_server
        .run_action(Action::CreateActivity(activity.into()))
        .await
        .unwrap();

    // Create entry
    let create_entry = CreateEntry {
        actor_id: user_id,
        entry: Entry {
            id: Uuid::new_v4(),
            activity_id: Some(activity_id),
            owner_id: user_id,
            position: None,
            is_template: false,
            display_as_sets: false,
            is_sequence: false,
            temporal: Temporal::None,
        },
    };
    postgres_server
        .run_action(Action::CreateEntry(create_entry))
        .await
        .unwrap();
}

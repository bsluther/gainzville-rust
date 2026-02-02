use fractional_index::FractionalIndex;
use gv_core::{
    SYSTEM_ACTOR_ID,
    actions::CreateEntry,
    models::entry::{Entry, Position, Temporal},
    reader::Reader,
};
use gv_sqlite::{client::SqliteClient, reader::SqliteReader};
use sqlx::SqlitePool;
use tracing::debug;
use uuid::Uuid;

#[sqlx::test(migrations = "../sqlite/migrations")]
async fn test_find_descendants(pool: SqlitePool) {
    let sqlite_client = SqliteClient::from_pool(pool);

    let a: Entry = Entry {
        activity_id: None,
        display_as_sets: false,
        id: Uuid::new_v4(),
        is_sequence: false,
        is_template: false,
        owner_id: SYSTEM_ACTOR_ID,
        position: None,
        temporal: Temporal::None,
    };

    let b: Entry = Entry {
        activity_id: None,
        display_as_sets: false,
        id: Uuid::new_v4(),
        is_sequence: false,
        is_template: false,
        owner_id: SYSTEM_ACTOR_ID,
        position: Some(Position {
            parent_id: a.id.clone(),
            frac_index: FractionalIndex::default(),
        }),
        temporal: Temporal::None,
    };

    sqlite_client
        .run_action(CreateEntry::from(a.clone()).into())
        .await
        .unwrap();
    sqlite_client
        .run_action(CreateEntry::from(b.clone()).into())
        .await
        .unwrap();

    let a_descs = SqliteReader::find_descendants(&sqlite_client.pool, a.id)
        .await
        .unwrap();
    let b_descs = SqliteReader::find_descendants(&sqlite_client.pool, b.id)
        .await
        .unwrap();

    println!("{:?}", b_descs);
    assert_eq!(a_descs.len(), 2);
    assert_eq!(b_descs.len(), 1);
}

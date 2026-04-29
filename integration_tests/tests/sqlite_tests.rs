use fractional_index::FractionalIndex;
use gv_client::{client::SqliteClient, sqlite_executor::SqliteQueryExecutor};
use gv_core::{
    SYSTEM_ACTOR_ID,
    actions::{CreateAttribute, CreateEntry, CreateValue},
    models::{
        attribute::{
            Attribute, AttributeConfig, AttributeValue, NumericConfig, NumericValue, Value,
        },
        entry::{Entry, Position, Temporal},
    },
    queries::{FindAttributeById, FindDescendants, FindValueByKey},
    query_executor::QueryExecutor,
};
use sqlx::SqlitePool;
use uuid::Uuid;

#[sqlx::test(migrations = "../client/migrations")]
async fn test_find_descendants(pool: SqlitePool) {
    let sqlite_client = SqliteClient::from_pool(pool);

    let a: Entry = Entry {
        activity_id: None,
        display_as_sets: false,
        id: Uuid::new_v4(),
        is_sequence: false,
        is_complete: false,
        is_template: false,
        name: Some("a".to_string()),
        owner_id: SYSTEM_ACTOR_ID,
        position: None,
        temporal: Temporal::None,
    };

    let b: Entry = Entry {
        activity_id: None,
        display_as_sets: false,
        id: Uuid::new_v4(),
        is_sequence: false,
        is_complete: false,
        is_template: false,
        name: Some("b".to_string()),
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

    let (a_descs, b_descs) = {
        let mut connection = sqlite_client.pool.acquire().await.unwrap();
        let a_descs = SqliteQueryExecutor::new(&mut *connection)
            .execute(FindDescendants { entry_id: a.id })
            .await
            .unwrap();
        let b_descs = SqliteQueryExecutor::new(&mut *connection)
            .execute(FindDescendants { entry_id: b.id })
            .await
            .unwrap();
        (a_descs, b_descs)
    };

    println!("{:?}", b_descs);
    assert_eq!(a_descs.len(), 2);
    assert_eq!(b_descs.len(), 1);
}

#[sqlx::test(migrations = "../client/migrations")]
async fn test_create_attribute_and_value(pool: SqlitePool) {
    let sqlite_client = SqliteClient::from_pool(pool);

    // Create an attribute with a numeric config.
    let config = AttributeConfig::Numeric(NumericConfig {
        min: Some(0.0),
        max: Some(500.0),
        integer: false,
        default: None,
    });
    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: SYSTEM_ACTOR_ID,
        name: "Weight".to_string(),
        config,
    };

    sqlite_client
        .run_action(CreateAttribute::from(attribute.clone()).into())
        .await
        .unwrap();

    // Read attribute back.
    let read_attr = {
        let mut connection = sqlite_client.pool.acquire().await.unwrap();
        SqliteQueryExecutor::new(&mut *connection)
            .execute(FindAttributeById {
                attribute_id: attribute.id,
            })
            .await
            .unwrap()
            .expect("attribute should exist")
    };
    assert_eq!(read_attr.id, attribute.id);
    assert_eq!(read_attr.name, "Weight");

    // Create an entry to attach the value to.
    let entry = Entry {
        id: Uuid::new_v4(),
        activity_id: None,
        name: Some("entry-with-weight".to_string()),
        owner_id: SYSTEM_ACTOR_ID,
        position: None,
        display_as_sets: false,
        is_sequence: false,
        is_complete: false,
        is_template: false,
        temporal: Temporal::None,
    };
    sqlite_client
        .run_action(CreateEntry::from(entry.clone()).into())
        .await
        .unwrap();

    // Create a value via CreateValue action.
    let value = Value {
        entry_id: entry.id,
        attribute_id: attribute.id,
        index_float: None,
        index_string: None,
        plan: Some(AttributeValue::Numeric(NumericValue::Exact(135.0))),
        actual: Some(AttributeValue::Numeric(NumericValue::Exact(140.0))),
    };
    let create_value = CreateValue {
        actor_id: SYSTEM_ACTOR_ID,
        value: value.clone(),
    };
    sqlite_client.run_action(create_value.into()).await.unwrap();

    // Read value back.
    let read_value = {
        let mut connection = sqlite_client.pool.acquire().await.unwrap();
        SqliteQueryExecutor::new(&mut *connection)
            .execute(FindValueByKey {
                entry_id: entry.id,
                attribute_id: attribute.id,
            })
            .await
            .unwrap()
            .expect("value should exist")
    };
    assert_eq!(read_value.entry_id, entry.id);
    assert_eq!(read_value.attribute_id, attribute.id);

    // Verify JSON round-tripping for plan and actual.
    match read_value.plan.unwrap() {
        AttributeValue::Numeric(NumericValue::Exact(v)) => assert_eq!(v, 135.0),
        other => panic!("expected Numeric Exact plan, got {:?}", other),
    }
    match read_value.actual.unwrap() {
        AttributeValue::Numeric(NumericValue::Exact(v)) => assert_eq!(v, 140.0),
        other => panic!("expected Numeric Exact actual, got {:?}", other),
    }
}

#[sqlx::test(migrations = "../client/migrations")]
async fn test_create_entry_rejects_anonymous_without_name(pool: SqlitePool) {
    let sqlite_client = SqliteClient::from_pool(pool);

    // Both activity_id None and name None must be rejected.
    let entry = Entry {
        id: Uuid::new_v4(),
        activity_id: None,
        name: None,
        owner_id: SYSTEM_ACTOR_ID,
        position: None,
        display_as_sets: false,
        is_sequence: false,
        is_complete: false,
        is_template: false,
        temporal: Temporal::None,
    };
    let err = sqlite_client
        .run_action(CreateEntry::from(entry).into())
        .await
        .expect_err("anonymous entry without a name should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("anonymous entry"),
        "unexpected error message: {msg}"
    );

    // Empty name is also rejected.
    let entry = Entry {
        id: Uuid::new_v4(),
        activity_id: None,
        name: Some("".to_string()),
        owner_id: SYSTEM_ACTOR_ID,
        position: None,
        display_as_sets: false,
        is_sequence: false,
        is_complete: false,
        is_template: false,
        temporal: Temporal::None,
    };
    sqlite_client
        .run_action(CreateEntry::from(entry).into())
        .await
        .expect_err("anonymous entry with empty name should be rejected");

    // A non-empty name is accepted.
    let entry = Entry {
        id: Uuid::new_v4(),
        activity_id: None,
        name: Some("Standalone".to_string()),
        owner_id: SYSTEM_ACTOR_ID,
        position: None,
        display_as_sets: false,
        is_sequence: false,
        is_complete: false,
        is_template: false,
        temporal: Temporal::None,
    };
    sqlite_client
        .run_action(CreateEntry::from(entry).into())
        .await
        .expect("anonymous entry with a name should be accepted");
}

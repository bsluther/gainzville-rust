use fractional_index::FractionalIndex;
use gv_client::client::SqliteClient;
use gv_sql::sqlite::SqliteQueryExecutor;
use gv_core::{
    SYSTEM_ACTOR_ID,
    actions::{AttachValue, CreateAttribute, CreateEntry, CreateValue, DeleteAttributeValue},
    models::{
        attribute::{
            Attribute, AttributeConfig, AttributeValue, MassConfig, MassMeasurement, MassUnit,
            MassValue, NumericConfig, NumericValue, Value,
        },
        entry::{Entry, Position, Temporal},
    },
    queries::{FindAttributeById, FindDescendants, FindValueByKey},
    query_executor::QueryExecutor,
};
use sqlx::SqlitePool;
use uuid::Uuid;

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_find_descendants(pool: SqlitePool) {
    let sqlite_client = SqliteClient::from_pool(pool);

    let a: Entry = Entry {
        activity_id: None,
        display_as_sets: false,
        id: Uuid::new_v4(),
        is_sequence: false,
        is_complete: false,
        is_template: false,
        name: None,
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
        name: None,
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

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
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
        description: None,
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
        name: None,
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

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_attach_and_detach_value(pool: SqlitePool) {
    let sqlite_client = SqliteClient::from_pool(pool);

    // Numeric attribute with a scalar default of 5.0.
    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: SYSTEM_ACTOR_ID,
        name: "Reps".to_string(),
        description: None,
        config: AttributeConfig::Numeric(NumericConfig {
            min: None,
            max: None,
            integer: true,
            default: Some(5.0),
        }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(attribute.clone()).into())
        .await
        .unwrap();

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
    sqlite_client
        .run_action(CreateEntry::from(entry.clone()).into())
        .await
        .unwrap();

    let read_value = || async {
        let mut connection = sqlite_client.pool.acquire().await.unwrap();
        SqliteQueryExecutor::new(&mut *connection)
            .execute(FindValueByKey {
                entry_id: entry.id,
                attribute_id: attribute.id,
            })
            .await
            .unwrap()
    };

    // Attach: seeds the config default into both plan and actual.
    sqlite_client
        .run_action(
            AttachValue {
                actor_id: SYSTEM_ACTOR_ID,
                entry_id: entry.id,
                attribute_id: attribute.id,
            }
            .into(),
        )
        .await
        .unwrap();
    let seeded = read_value().await.expect("value should exist after attach");
    for field in [seeded.plan.clone(), seeded.actual.clone()] {
        match field.expect("seeded default present") {
            AttributeValue::Numeric(NumericValue::Exact(v)) => assert_eq!(v, 5.0),
            other => panic!("expected seeded Numeric Exact 5.0, got {:?}", other),
        }
    }

    // Attaching again is a no-op and must not error (PK conflict avoided).
    sqlite_client
        .run_action(
            AttachValue {
                actor_id: SYSTEM_ACTOR_ID,
                entry_id: entry.id,
                attribute_id: attribute.id,
            }
            .into(),
        )
        .await
        .unwrap();
    assert!(read_value().await.is_some());

    // Detach removes the value.
    sqlite_client
        .run_action(
            DeleteAttributeValue {
                actor_id: SYSTEM_ACTOR_ID,
                entry_id: entry.id,
                attribute_id: attribute.id,
            }
            .into(),
        )
        .await
        .unwrap();
    assert!(read_value().await.is_none(), "value removed after detach");

    // Detaching again is an idempotent no-op.
    sqlite_client
        .run_action(
            DeleteAttributeValue {
                actor_id: SYSTEM_ACTOR_ID,
                entry_id: entry.id,
                attribute_id: attribute.id,
            }
            .into(),
        )
        .await
        .unwrap();
    assert!(read_value().await.is_none());
}

/// Helper: create a bare scalar entry owned by SYSTEM_ACTOR_ID.
async fn seed_entry(client: &SqliteClient) -> Entry {
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
    client
        .run_action(CreateEntry::from(entry.clone()).into())
        .await
        .unwrap();
    entry
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_attach_mass_seeds_default_units(pool: SqlitePool) {
    let sqlite_client = SqliteClient::from_pool(pool);

    // Mass attribute with two default units.
    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: SYSTEM_ACTOR_ID,
        name: "Load".to_string(),
        description: None,
        config: AttributeConfig::Mass(MassConfig {
            default_units: vec![MassUnit::Kilogram, MassUnit::Pound],
        }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(attribute.clone()).into())
        .await
        .unwrap();
    let entry = seed_entry(&sqlite_client).await;

    sqlite_client
        .run_action(
            AttachValue {
                actor_id: SYSTEM_ACTOR_ID,
                entry_id: entry.id,
                attribute_id: attribute.id,
            }
            .into(),
        )
        .await
        .unwrap();

    let value = {
        let mut connection = sqlite_client.pool.acquire().await.unwrap();
        SqliteQueryExecutor::new(&mut *connection)
            .execute(FindValueByKey {
                entry_id: entry.id,
                attribute_id: attribute.id,
            })
            .await
            .unwrap()
            .expect("value should exist after attach")
    };

    // Both plan and actual seed a zero-magnitude measurement per default unit.
    let expected = vec![
        MassMeasurement { unit: MassUnit::Kilogram, value: 0.0 },
        MassMeasurement { unit: MassUnit::Pound, value: 0.0 },
    ];
    for field in [value.plan, value.actual] {
        match field.expect("seeded mass present") {
            AttributeValue::Mass(MassValue::Exact(ms)) => assert_eq!(ms, expected),
            other => panic!("expected Mass Exact with default units, got {:?}", other),
        }
    }
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_attach_mass_empty_units_seeds_none(pool: SqlitePool) {
    let sqlite_client = SqliteClient::from_pool(pool);

    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: SYSTEM_ACTOR_ID,
        name: "Load".to_string(),
        description: None,
        config: AttributeConfig::Mass(MassConfig { default_units: vec![] }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(attribute.clone()).into())
        .await
        .unwrap();
    let entry = seed_entry(&sqlite_client).await;

    sqlite_client
        .run_action(
            AttachValue {
                actor_id: SYSTEM_ACTOR_ID,
                entry_id: entry.id,
                attribute_id: attribute.id,
            }
            .into(),
        )
        .await
        .unwrap();

    let value = {
        let mut connection = sqlite_client.pool.acquire().await.unwrap();
        SqliteQueryExecutor::new(&mut *connection)
            .execute(FindValueByKey {
                entry_id: entry.id,
                attribute_id: attribute.id,
            })
            .await
            .unwrap()
            .expect("value should exist after attach")
    };
    assert!(value.plan.is_none(), "no default units -> empty plan");
    assert!(value.actual.is_none(), "no default units -> empty actual");
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_create_value_is_noop_when_value_exists(pool: SqlitePool) {
    let sqlite_client = SqliteClient::from_pool(pool);

    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: SYSTEM_ACTOR_ID,
        name: "Reps".to_string(),
        description: None,
        config: AttributeConfig::Numeric(NumericConfig {
            min: None,
            max: None,
            integer: false,
            default: None,
        }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(attribute.clone()).into())
        .await
        .unwrap();
    let entry = seed_entry(&sqlite_client).await;

    let make_value = |actual: f64| Value {
        entry_id: entry.id,
        attribute_id: attribute.id,
        index_float: None,
        index_string: None,
        plan: None,
        actual: Some(AttributeValue::Numeric(NumericValue::Exact(actual))),
    };

    // First create wins.
    sqlite_client
        .run_action(
            CreateValue { actor_id: SYSTEM_ACTOR_ID, value: make_value(100.0) }.into(),
        )
        .await
        .unwrap();
    // Second create for the same key is a no-op (must not error or overwrite).
    sqlite_client
        .run_action(
            CreateValue { actor_id: SYSTEM_ACTOR_ID, value: make_value(200.0) }.into(),
        )
        .await
        .unwrap();

    let value = {
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
    match value.actual.expect("actual present") {
        AttributeValue::Numeric(NumericValue::Exact(v)) => {
            assert_eq!(v, 100.0, "second CreateValue must not overwrite")
        }
        other => panic!("expected Numeric Exact, got {:?}", other),
    }
}

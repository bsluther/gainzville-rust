use fractional_index::FractionalIndex;
use generation::{Arbitrary, SimulationContext};
use gv_client::client::SqliteClient;
use gv_core::{
    actions::{
        Action, AttachValue, AttributeChange, ConvertToSets, CreateActivity, CreateAttribute,
        CreateEntry, CreateEntryFromActivity, CreateUser, CreateValue, DeleteAttributeValue,
        DeleteEntryRecursive, DuplicateEntry, EntryChange, MassChange, MoveEntry, NumericChange,
        SelectChange, UpdateAttribute, UpdateAttributeValue, UpdateEntry, ValueField,
    },
    models::{
        activity::{Activity, ActivityName},
        attribute::{
            Attribute, AttributeConfig, AttributeValue, MassConfig, MassMeasurement, MassUnit,
            MassValue, NumericConfig, NumericValue, SelectConfig, SelectValue, Value,
        },
        entry::{Entry, Position, Temporal},
        user::User,
    },
    queries::{
        AllEntries, FindAttributeById, FindDescendants, FindEntryById, FindValueByKey,
        FindValuesForEntries,
    },
    query_executor::QueryExecutor,
    validation::{Email, Username},
};
use gv_sql::sqlite::SqliteQueryExecutor;
use rand::SeedableRng;
use rand::rngs::ChaCha8Rng;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

async fn run_actions(client: &SqliteClient, actions: impl IntoIterator<Item = Action>) {
    for action in actions {
        client.run_action(action).await.unwrap();
    }
}

async fn create_user(client: &SqliteClient) -> User {
    let id = Uuid::new_v4();
    let user = User {
        actor_id: id,
        username: Username::parse(format!("u{}", id.simple())).unwrap(),
        email: Email::parse(format!("{}@test.com", id.simple())).unwrap(),
    };
    client
        .run_action(CreateUser::from(user.clone()).into())
        .await
        .unwrap();
    user
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_find_descendants(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));

    let user = create_user(&sqlite_client).await;

    let a: Entry = Entry {
        activity_id: None,
        display_as_sets: false,
        id: Uuid::new_v4(),
        is_sequence: false,
        is_complete: false,
        is_template: false,
        name: None,
        owner_id: user.actor_id,
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
        owner_id: user.actor_id,
        position: Some(Position {
            parent_id: a.id.clone(),
            frac_index: FractionalIndex::default(),
        }),
        temporal: Temporal::None,
    };

    run_actions(
        &sqlite_client,
        [
            CreateEntry::from(a.clone()).into(),
            CreateEntry::from(b.clone()).into(),
        ],
    )
    .await;

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
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&sqlite_client).await;
    let user_id = user.actor_id.clone();

    // Create an attribute with a numeric config.
    let config = AttributeConfig::Numeric(NumericConfig {
        min: Some(0.0),
        max: Some(500.0),
        integer: false,
        default: None,
    });
    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
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
        owner_id: user_id,
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
        actor_id: user_id,
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
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&sqlite_client).await;
    let user_id = user.actor_id.clone();

    // Numeric attribute with a scalar default of 5.0.
    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: user_id,
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
        owner_id: user_id,
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
                actor_id: user_id,
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
                actor_id: user_id,
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
                actor_id: user_id,
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
                actor_id: user_id,
                entry_id: entry.id,
                attribute_id: attribute.id,
            }
            .into(),
        )
        .await
        .unwrap();
    assert!(read_value().await.is_none());
}

/// Helper: create a user and a bare scalar entry owned by that user.
async fn seed_entry(client: &SqliteClient) -> (User, Entry) {
    let user = create_user(&client).await;
    let user_id = user.actor_id.clone();

    let entry = Entry {
        id: Uuid::new_v4(),
        activity_id: None,
        name: None,
        owner_id: user_id,
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
    (user, entry)
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_attach_mass_seeds_default_unit(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));

    let (user, entry) = seed_entry(&sqlite_client).await;
    let user_id = user.actor_id.clone();

    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: user_id,
        name: "Load".to_string(),
        description: None,
        config: AttributeConfig::Mass(MassConfig {
            default_unit: MassUnit::Kilogram,
        }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(attribute.clone()).into())
        .await
        .unwrap();

    sqlite_client
        .run_action(
            AttachValue {
                actor_id: user_id,
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

    // Both plan and actual seed a zero-magnitude measurement in the default unit.
    let expected = MassMeasurement {
        unit: MassUnit::Kilogram,
        value: 0.0,
    };
    for field in [value.plan, value.actual] {
        match field.expect("seeded mass present") {
            AttributeValue::Mass(MassValue::Exact(m)) => assert_eq!(m, expected),
            other => panic!("expected Mass Exact in the default unit, got {:?}", other),
        }
    }
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_create_value_is_noop_when_value_exists(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let (user, entry) = seed_entry(&sqlite_client).await;

    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
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
            CreateValue {
                actor_id: user.actor_id,
                value: make_value(100.0),
            }
            .into(),
        )
        .await
        .unwrap();
    // Second create for the same key is a no-op (must not error or overwrite).
    sqlite_client
        .run_action(
            CreateValue {
                actor_id: user.actor_id,
                value: make_value(200.0),
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
            .expect("value should exist")
    };
    match value.actual.expect("actual present") {
        AttributeValue::Numeric(NumericValue::Exact(v)) => {
            assert_eq!(v, 100.0, "second CreateValue must not overwrite")
        }
        other => panic!("expected Numeric Exact, got {:?}", other),
    }
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_update_actual_on_plan_only_row_succeeds(pool: SqlitePool) {
    // Arbitrary seeding (Value::arbitrary) can produce plan-only rows. Updating
    // `actual` on such a row must succeed — the precondition is row existence,
    // not field existence — so plan-only seeds are NOT the cause of the
    // "value does not exist" rejection (that requires a missing row).
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let (user, entry) = seed_entry(&sqlite_client).await;

    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: "Outcome".to_string(),
        description: None,
        config: AttributeConfig::Select(SelectConfig {
            options: vec!["Flash".to_string(), "Sent".to_string()],
            ordered: false,
            default: None,
        }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(attribute.clone()).into())
        .await
        .unwrap();

    // Plan set, actual NULL — the shape that displays as empty in the UI today.
    sqlite_client
        .run_action(
            CreateValue {
                actor_id: user.actor_id,
                value: Value {
                    entry_id: entry.id,
                    attribute_id: attribute.id,
                    index_float: None,
                    index_string: None,
                    plan: Some(AttributeValue::Select(SelectValue::Exact("Flash".to_string()))),
                    actual: None,
                },
            }
            .into(),
        )
        .await
        .unwrap();

    let result = sqlite_client
        .run_action(
            UpdateAttributeValue {
                actor_id: user.actor_id,
                entry_id: entry.id,
                attribute_id: attribute.id,
                field: ValueField::Actual,
                value: Some(AttributeValue::Select(SelectValue::Exact("Sent".to_string()))),
            }
            .into(),
        )
        .await;
    assert!(result.is_ok(), "update actual on plan-only row should succeed: {result:?}");

    let value = {
        let mut connection = sqlite_client.pool.acquire().await.unwrap();
        SqliteQueryExecutor::new(&mut *connection)
            .execute(FindValueByKey { entry_id: entry.id, attribute_id: attribute.id })
            .await
            .unwrap()
            .expect("value row exists")
    };
    assert_eq!(value.actual, Some(AttributeValue::Select(SelectValue::Exact("Sent".to_string()))));
    assert_eq!(value.plan, Some(AttributeValue::Select(SelectValue::Exact("Flash".to_string()))));
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_update_attribute_value_clears_only_target_field(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let (user, entry) = seed_entry(&sqlite_client).await;

    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
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

    // Seed a value with both plan and actual populated.
    sqlite_client
        .run_action(
            CreateValue {
                actor_id: user.actor_id,
                value: Value {
                    entry_id: entry.id,
                    attribute_id: attribute.id,
                    index_float: None,
                    index_string: None,
                    plan: Some(AttributeValue::Numeric(NumericValue::Exact(5.0))),
                    actual: Some(AttributeValue::Numeric(NumericValue::Exact(8.0))),
                },
            }
            .into(),
        )
        .await
        .unwrap();

    // Clearing `actual` (value: None) nulls only that field; plan is untouched
    // and the value row remains attached.
    sqlite_client
        .run_action(
            UpdateAttributeValue {
                actor_id: user.actor_id,
                entry_id: entry.id,
                attribute_id: attribute.id,
                field: ValueField::Actual,
                value: None,
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
            .expect("value row should still exist after clearing a field")
    };
    assert!(value.actual.is_none(), "cleared field should be None");
    assert_eq!(
        value.plan,
        Some(AttributeValue::Numeric(NumericValue::Exact(5.0))),
        "clearing actual must not touch plan"
    );
}

async fn read_attribute(client: &SqliteClient, id: Uuid) -> Attribute {
    let mut connection = client.pool.acquire().await.unwrap();
    SqliteQueryExecutor::new(&mut *connection)
        .execute(FindAttributeById { attribute_id: id })
        .await
        .unwrap()
        .expect("attribute should exist")
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_update_attribute_defaults(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&sqlite_client).await;

    // --- Numeric: set default within bounds; reject out-of-bounds and non-integer.
    let numeric = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: "Reps".to_string(),
        description: None,
        config: AttributeConfig::Numeric(NumericConfig {
            min: Some(0.0),
            max: Some(20.0),
            integer: true,
            default: None,
        }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(numeric.clone()).into())
        .await
        .unwrap();

    sqlite_client
        .run_action(
            UpdateAttribute {
                actor_id: user.actor_id,
                attribute_id: numeric.id,
                change: AttributeChange::Numeric(NumericChange::SetDefault(Some(8.0))),
            }
            .into(),
        )
        .await
        .unwrap();
    assert_eq!(
        read_attribute(&sqlite_client, numeric.id)
            .await
            .as_numeric()
            .unwrap()
            .default,
        Some(8.0)
    );

    // Above max -> rejected.
    assert!(
        sqlite_client
            .run_action(
                UpdateAttribute {
                    actor_id: user.actor_id,
                    attribute_id: numeric.id,
                    change: AttributeChange::Numeric(NumericChange::SetDefault(Some(50.0))),
                }
                .into(),
            )
            .await
            .is_err(),
        "default above max must be rejected"
    );
    // Non-integer on an integer attribute -> rejected.
    assert!(
        sqlite_client
            .run_action(
                UpdateAttribute {
                    actor_id: user.actor_id,
                    attribute_id: numeric.id,
                    change: AttributeChange::Numeric(NumericChange::SetDefault(Some(3.5))),
                }
                .into(),
            )
            .await
            .is_err(),
        "non-integer default on integer attribute must be rejected"
    );
    // Default unchanged after rejected updates.
    assert_eq!(
        read_attribute(&sqlite_client, numeric.id)
            .await
            .as_numeric()
            .unwrap()
            .default,
        Some(8.0)
    );

    // --- Select: set default to an existing option; reject unknown option.
    let select = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: "Outcome".to_string(),
        description: None,
        config: AttributeConfig::Select(SelectConfig {
            options: vec!["Win".to_string(), "Loss".to_string()],
            ordered: false,
            default: None,
        }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(select.clone()).into())
        .await
        .unwrap();
    sqlite_client
        .run_action(
            UpdateAttribute {
                actor_id: user.actor_id,
                attribute_id: select.id,
                change: AttributeChange::Select(SelectChange::SetDefault(Some("Win".to_string()))),
            }
            .into(),
        )
        .await
        .unwrap();
    assert_eq!(
        read_attribute(&sqlite_client, select.id)
            .await
            .expect_select()
            .unwrap()
            .default,
        Some("Win".to_string())
    );
    assert!(
        sqlite_client
            .run_action(
                UpdateAttribute {
                    actor_id: user.actor_id,
                    attribute_id: select.id,
                    change: AttributeChange::Select(SelectChange::SetDefault(Some(
                        "Draw".to_string()
                    ))),
                }
                .into(),
            )
            .await
            .is_err(),
        "default not in options must be rejected"
    );

    // Type-mismatched change is rejected.
    assert!(
        sqlite_client
            .run_action(
                UpdateAttribute {
                    actor_id: user.actor_id,
                    attribute_id: select.id,
                    change: AttributeChange::Numeric(NumericChange::SetDefault(Some(1.0))),
                }
                .into(),
            )
            .await
            .is_err(),
        "numeric change on a select attribute must be rejected"
    );

    // --- Mass: replace the default unit; common SetName edit.
    let mass = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: "Load".to_string(),
        description: None,
        config: AttributeConfig::Mass(MassConfig {
            default_unit: MassUnit::Kilogram,
        }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(mass.clone()).into())
        .await
        .unwrap();
    sqlite_client
        .run_action(
            UpdateAttribute {
                actor_id: user.actor_id,
                attribute_id: mass.id,
                change: AttributeChange::Mass(MassChange::SetDefaultUnit(MassUnit::Pound)),
            }
            .into(),
        )
        .await
        .unwrap();
    assert_eq!(
        read_attribute(&sqlite_client, mass.id)
            .await
            .expect_mass()
            .unwrap()
            .default_unit,
        MassUnit::Pound
    );

    sqlite_client
        .run_action(
            UpdateAttribute {
                actor_id: user.actor_id,
                attribute_id: mass.id,
                change: AttributeChange::SetName("Weight".to_string()),
            }
            .into(),
        )
        .await
        .unwrap();
    assert_eq!(read_attribute(&sqlite_client, mass.id).await.name, "Weight");
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_update_attribute_noop(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&sqlite_client).await;

    // --- No-op: re-applying the same value emits no deltas.
    let numeric = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: "Reps".to_string(),
        description: None,
        config: AttributeConfig::Numeric(NumericConfig {
            min: None,
            max: None,
            integer: false,
            default: Some(5.0),
        }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(numeric.clone()).into())
        .await
        .unwrap();

    // Call the mutator directly so we can inspect the produced deltas.
    let mutation = {
        let mut conn = sqlite_client.pool.acquire().await.unwrap();
        let mut executor = SqliteQueryExecutor::new(&mut *conn);
        gv_core::mutators::update_attribute(
            &mut executor,
            &gv_core::io::SystemIo::default(),
            UpdateAttribute {
                actor_id: user.actor_id,
                attribute_id: numeric.id,
                change: AttributeChange::Numeric(NumericChange::SetDefault(Some(5.0))),
            },
        )
        .await
        .unwrap()
    };
    assert!(
        mutation.changes.is_empty(),
        "setting the default to its current value must be a no-op"
    );
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_template_temporal_rules(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&sqlite_client).await;

    let activity = Activity {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: ActivityName::parse("Squat".to_string()).unwrap(),
        description: None,
        source_activity_id: None,
    };
    sqlite_client
        .run_action(activity.clone().into_create_activity(Uuid::new_v4()).into())
        .await
        .unwrap();

    let template_entry = |temporal: Temporal| Entry {
        id: Uuid::new_v4(),
        activity_id: Some(activity.id),
        owner_id: user.actor_id,
        name: None,
        position: None,
        is_template: true,
        display_as_sets: false,
        is_sequence: false,
        is_complete: false,
        temporal,
    };

    // create_entry: a template entry with a start time is rejected...
    assert!(
        sqlite_client
            .run_action(
                CreateEntry::from(template_entry(Temporal::Start {
                    start: sqlx::types::chrono::Utc::now()
                }))
                .into(),
            )
            .await
            .is_err(),
        "template entry with a start time must be rejected"
    );
    // ...but duration-only and None are allowed.
    sqlite_client
        .run_action(
            CreateEntry::from(template_entry(Temporal::Duration { duration: 60_000 })).into(),
        )
        .await
        .unwrap();
    let none_template = template_entry(Temporal::None);
    let none_template_id = none_template.id;
    sqlite_client
        .run_action(CreateEntry::from(none_template.clone()).into())
        .await
        .unwrap();

    // move_entry: giving a template a start time is rejected.
    assert!(
        sqlite_client
            .run_action(
                MoveEntry {
                    actor_id: user.actor_id,
                    entry_id: none_template_id,
                    position: None,
                    temporal: Temporal::Start {
                        start: sqlx::types::chrono::Utc::now()
                    },
                }
                .into(),
            )
            .await
            .is_err(),
        "moving a template to a start-timed temporal must be rejected"
    );

    // Sanity: a log entry at root with no start/end is still rejected.
    let log_root = Entry {
        id: Uuid::new_v4(),
        activity_id: None,
        owner_id: user.actor_id,
        name: None,
        position: None,
        is_template: false,
        display_as_sets: false,
        is_sequence: false,
        is_complete: false,
        temporal: Temporal::Start {
            start: sqlx::types::chrono::Utc::now(),
        },
    };
    sqlite_client
        .run_action(CreateEntry::from(log_root.clone()).into())
        .await
        .unwrap();
    assert!(
        sqlite_client
            .run_action(
                MoveEntry {
                    actor_id: user.actor_id,
                    entry_id: log_root.id,
                    position: None,
                    temporal: Temporal::None,
                }
                .into(),
            )
            .await
            .is_err(),
        "log root with no start/end must still be rejected"
    );
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_update_entry_set_is_sequence(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&sqlite_client).await;

    // A sequence root with one child.
    let parent = Entry {
        id: Uuid::new_v4(),
        activity_id: None,
        name: None,
        owner_id: user.actor_id,
        position: None,
        display_as_sets: false,
        is_sequence: true,
        is_complete: false,
        is_template: false,
        temporal: Temporal::Start {
            start: sqlx::types::chrono::Utc::now(),
        },
    };
    let child = Entry {
        id: Uuid::new_v4(),
        activity_id: None,
        name: None,
        owner_id: user.actor_id,
        position: Some(Position {
            parent_id: parent.id,
            frac_index: FractionalIndex::default(),
        }),
        display_as_sets: false,
        is_sequence: false,
        is_complete: false,
        is_template: false,
        temporal: Temporal::None,
    };
    sqlite_client
        .run_action(CreateEntry::from(parent.clone()).into())
        .await
        .unwrap();
    sqlite_client
        .run_action(CreateEntry::from(child.clone()).into())
        .await
        .unwrap();

    async fn find(client: &SqliteClient, id: Uuid) -> Option<Entry> {
        let mut conn = client.pool.acquire().await.unwrap();
        SqliteQueryExecutor::new(&mut *conn)
            .execute(FindEntryById { entry_id: id })
            .await
            .unwrap()
    }

    // Converting the sequence to a scalar deletes its children.
    sqlite_client
        .run_action(
            UpdateEntry {
                actor_id: user.actor_id,
                entry_id: parent.id,
                change: EntryChange::SetIsSequence(false),
            }
            .into(),
        )
        .await
        .unwrap();
    assert_eq!(
        find(&sqlite_client, parent.id).await.map(|e| e.is_sequence),
        Some(false)
    );
    assert!(
        find(&sqlite_client, child.id).await.is_none(),
        "child must be deleted on scalar conversion"
    );

    // Converting back to a sequence is allowed.
    sqlite_client
        .run_action(
            UpdateEntry {
                actor_id: user.actor_id,
                entry_id: parent.id,
                change: EntryChange::SetIsSequence(true),
            }
            .into(),
        )
        .await
        .unwrap();
    assert_eq!(
        find(&sqlite_client, parent.id).await.map(|e| e.is_sequence),
        Some(true)
    );
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_create_entry_from_activity_instantiates_template(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&sqlite_client).await;

    // Attribute with a default to seed onto the template.
    let reps = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
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
        .run_action(CreateAttribute::from(reps.clone()).into())
        .await
        .unwrap();

    // Activity with a known sequence template root.
    let activity = Activity {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: ActivityName::parse("Bench".to_string()).unwrap(),
        description: None,
        source_activity_id: None,
    };
    let template_root = Entry {
        id: Uuid::new_v4(),
        activity_id: Some(activity.id),
        owner_id: user.actor_id,
        name: None,
        position: None,
        is_template: true,
        display_as_sets: false,
        is_sequence: true,
        is_complete: false,
        temporal: Temporal::None,
    };
    sqlite_client
        .run_action(
            CreateActivity {
                actor_id: user.actor_id,
                activity: activity.clone(),
                template: vec![template_root.clone()],
            }
            .into(),
        )
        .await
        .unwrap();

    // A template child (scalar) with a seeded "Reps" value.
    let template_child = Entry {
        id: Uuid::new_v4(),
        activity_id: None,
        owner_id: user.actor_id,
        name: Some("Set".to_string()),
        position: Some(Position {
            parent_id: template_root.id,
            frac_index: FractionalIndex::default(),
        }),
        is_template: true,
        display_as_sets: false,
        is_sequence: false,
        is_complete: false,
        temporal: Temporal::None,
    };
    sqlite_client
        .run_action(CreateEntry::from(template_child.clone()).into())
        .await
        .unwrap();
    sqlite_client
        .run_action(
            AttachValue {
                actor_id: user.actor_id,
                entry_id: template_child.id,
                attribute_id: reps.id,
            }
            .into(),
        )
        .await
        .unwrap();

    // Instantiate the activity at a day root.
    let start = sqlx::types::chrono::Utc::now();
    sqlite_client
        .run_action(
            CreateEntryFromActivity {
                actor_id: user.actor_id,
                activity_id: activity.id,
                position: None,
                temporal: Temporal::Start { start },
                is_template: false,
            }
            .into(),
        )
        .await
        .unwrap();

    // Read all entries; the instances are the non-template ones.
    let all = {
        let mut conn = sqlite_client.pool.acquire().await.unwrap();
        SqliteQueryExecutor::new(&mut *conn)
            .execute(AllEntries {})
            .await
            .unwrap()
    };
    let instances: Vec<&Entry> = all.iter().filter(|e| !e.is_template).collect();
    assert_eq!(instances.len(), 2, "root + child instantiated");

    let inst_root = instances
        .iter()
        .find(|e| e.position.is_none())
        .expect("instance root");
    assert_eq!(inst_root.activity_id, Some(activity.id));
    assert!(inst_root.is_sequence, "structure copied from template");
    assert_ne!(inst_root.id, template_root.id, "fresh id");
    assert!(matches!(inst_root.temporal, Temporal::Start { .. }));

    let inst_child = instances
        .iter()
        .find(|e| e.position.as_ref().map(|p| p.parent_id) == Some(inst_root.id))
        .expect("instance child under instance root");
    assert_ne!(inst_child.id, template_child.id);
    assert!(!inst_child.is_template);

    // The template's Reps value was copied onto the instantiated child.
    let values = {
        let mut conn = sqlite_client.pool.acquire().await.unwrap();
        SqliteQueryExecutor::new(&mut *conn)
            .execute(FindValuesForEntries {
                entry_ids: vec![inst_child.id],
            })
            .await
            .unwrap()
    };
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].attribute_id, reps.id);
    match values[0].actual.clone().expect("seeded actual") {
        AttributeValue::Numeric(NumericValue::Exact(v)) => assert_eq!(v, 5.0),
        other => panic!("expected Numeric Exact 5.0, got {:?}", other),
    }
}

/// Property-style round-trip: arbitrary entries written through the real client
/// write path (`run_action` → mutator → `SqliteDeltaExecutor`) and read back via
/// `AllEntries`.
///
/// This catches regressions where `EntryRow`'s `FromRow`-derived decode fails for
/// non-`Temporal::None` entries — e.g. when a `*Column` type's `compatible()` is
/// stricter than the underlying sqlx type's, rejecting valid SQL column types.
///
/// Ported from a gv-sql delta-level test; going through `run_action` means we
/// exercise the same path the live app uses (and no longer depend on a seeded
/// SYSTEM actor — users are created via arbitrary `CreateUser` actions).
#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn arbitrary_entries_round_trip_through_all_entries(pool: SqlitePool) {
    const N_USERS: usize = 4;
    const N_ENTRIES: usize = 30;

    let client = SqliteClient::from_pool(pool, Arc::new(gv_core::io::SystemIo::default()));
    let mut rng = ChaCha8Rng::seed_from_u64(0xb0c0_dabad7e5);
    let mut context = SimulationContext::default();

    // Seed a few users so generated entries can be owned by real actors. The
    // `From<Entry>` for `CreateEntry` makes the actor the entry's owner, so this
    // also satisfies the create-entry permission check. We apply each returned
    // mutation back to the context model so subsequent generation sees the actors.
    for _ in 0..N_USERS {
        let create = CreateUser::arbitrary(&mut rng, &context);
        let mx = client.run_action(create.into()).await.unwrap();
        context.apply_mutation(mx).await.unwrap();
    }

    // Generate arbitrary log entries, keeping the model in sync so generated
    // positions reference real sequence parents (and owners real actors).
    let mut inserted: HashMap<Uuid, Entry> = HashMap::with_capacity(N_ENTRIES);
    for _ in 0..N_ENTRIES {
        let create = CreateEntry::arbitrary(&mut rng, &context);
        inserted.insert(create.entry.id, create.entry.clone());
        let mx = client.run_action(create.into()).await.unwrap();
        context.apply_mutation(mx).await.unwrap();
    }

    // Read every entry back via AllEntries — this is what feeds the forest cache
    // in the live app.
    let entries = {
        let mut conn = client.pool.acquire().await.unwrap();
        SqliteQueryExecutor::new(&mut *conn)
            .execute(AllEntries {})
            .await
            .expect("AllEntries query must not fail")
    };

    assert_eq!(
        entries.len(),
        inserted.len(),
        "all inserted entries should come back through AllEntries"
    );
    for got in entries {
        let original = inserted.get(&got.id).expect("unknown id returned");
        assert_eq!(&got, original, "entry round-trip mismatch");
    }
}

// --- Sets (display_as_sets) ---

fn log_entry(owner_id: Uuid, activity_id: Option<Uuid>, position: Option<Position>) -> Entry {
    Entry {
        id: Uuid::new_v4(),
        activity_id,
        owner_id,
        name: None,
        position,
        is_template: false,
        display_as_sets: false,
        is_sequence: false,
        is_complete: false,
        temporal: Temporal::None,
    }
}

fn child_position(parent_id: Uuid, frac_index: FractionalIndex) -> Option<Position> {
    Some(Position {
        parent_id,
        frac_index,
    })
}

async fn find_entry(client: &SqliteClient, id: Uuid) -> Option<Entry> {
    let mut conn = client.pool.acquire().await.unwrap();
    SqliteQueryExecutor::new(&mut *conn)
        .execute(FindEntryById { entry_id: id })
        .await
        .unwrap()
}

/// A flagged sets sequence (root, on the timeline) with `member_count`
/// members of the given activity, built through the public actions.
async fn seed_sets_sequence(
    client: &SqliteClient,
    owner_id: Uuid,
    activity_id: Option<Uuid>,
    member_count: usize,
) -> (Entry, Vec<Entry>) {
    let mut sequence = log_entry(owner_id, None, None);
    sequence.is_sequence = true;
    sequence.temporal = Temporal::Start {
        start: sqlx::types::chrono::Utc::now(),
    };
    client
        .run_action(CreateEntry::from(sequence.clone()).into())
        .await
        .unwrap();

    let mut members = Vec::new();
    let mut frac_index = FractionalIndex::default();
    for _ in 0..member_count {
        let member = log_entry(
            owner_id,
            activity_id,
            child_position(sequence.id, frac_index.clone()),
        );
        client
            .run_action(CreateEntry::from(member.clone()).into())
            .await
            .unwrap();
        members.push(member);
        frac_index = FractionalIndex::new_after(&frac_index);
    }

    client
        .run_action(
            UpdateEntry {
                actor_id: owner_id,
                entry_id: sequence.id,
                change: EntryChange::SetDisplayAsSets(true),
            }
            .into(),
        )
        .await
        .unwrap();

    (sequence, members)
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_convert_to_sets_happy_path(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool, Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&client).await;

    let activity = Activity {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: ActivityName::parse("Bench Press".to_string()).unwrap(),
        description: None,
        source_activity_id: None,
    };
    client
        .run_action(activity.clone().into_create_activity(Uuid::new_v4()).into())
        .await
        .unwrap();

    let start = sqlx::types::chrono::Utc::now();
    let mut entry = log_entry(user.actor_id, Some(activity.id), None);
    entry.temporal = Temporal::StartAndDuration {
        start,
        duration_ms: 300_000,
    };
    entry.is_complete = true;
    client
        .run_action(CreateEntry::from(entry.clone()).into())
        .await
        .unwrap();

    let sequence_id = Uuid::new_v4();
    client
        .run_action(
            ConvertToSets {
                actor_id: user.actor_id,
                entry_id: entry.id,
                sequence_id,
            }
            .into(),
        )
        .await
        .unwrap();

    // The sequence takes the entry's root slot and start; anonymous, flagged.
    let sequence = find_entry(&client, sequence_id).await.unwrap();
    assert!(sequence.position.is_none(), "sequence takes the root slot");
    assert!(sequence.is_sequence);
    assert!(sequence.display_as_sets);
    assert_eq!(sequence.activity_id, None);
    assert_eq!(sequence.name, None);
    assert_eq!(sequence.temporal, Temporal::Start { start });
    assert!(!sequence.is_complete);

    // The entry becomes the sole member, keeping only its duration.
    let member = find_entry(&client, entry.id).await.unwrap();
    assert_eq!(member.parent_id(), Some(sequence_id));
    assert_eq!(member.temporal, Temporal::Duration { duration: 300_000 });
    assert_eq!(member.activity_id, Some(activity.id));
    assert!(member.is_complete, "member completion is untouched");

    // The flag round-trips through AllEntries (Entry::arbitrary no longer
    // generates it, so this is the persistence coverage for the column).
    let mut conn = client.pool.acquire().await.unwrap();
    let all = SqliteQueryExecutor::new(&mut *conn)
        .execute(AllEntries)
        .await
        .unwrap();
    assert!(
        all.iter().any(|e| e.id == sequence_id && e.display_as_sets),
        "display_as_sets must round-trip"
    );
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_convert_to_sets_rejections(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool, Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&client).await;

    let activity = Activity {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: ActivityName::parse("Squat".to_string()).unwrap(),
        description: None,
        source_activity_id: None,
    };
    let template_root_id = Uuid::new_v4();
    client
        .run_action(
            activity
                .clone()
                .into_create_activity(template_root_id)
                .into(),
        )
        .await
        .unwrap();

    // An activity template root cannot be converted.
    assert!(
        client
            .run_action(
                ConvertToSets {
                    actor_id: user.actor_id,
                    entry_id: template_root_id,
                    sequence_id: Uuid::new_v4(),
                }
                .into(),
            )
            .await
            .is_err(),
        "template root conversion must be rejected"
    );

    // The client-supplied sequence_id must be unused.
    let mut on_timeline = log_entry(user.actor_id, None, None);
    on_timeline.temporal = Temporal::Start {
        start: sqlx::types::chrono::Utc::now(),
    };
    client
        .run_action(CreateEntry::from(on_timeline.clone()).into())
        .await
        .unwrap();
    assert!(
        client
            .run_action(
                ConvertToSets {
                    actor_id: user.actor_id,
                    entry_id: on_timeline.id,
                    sequence_id: on_timeline.id,
                }
                .into(),
            )
            .await
            .is_err(),
        "colliding sequence_id must be rejected"
    );

    // A log root with no start/end cannot be converted: the sequence would
    // land off the timeline.
    let off_timeline = log_entry(user.actor_id, None, None);
    client
        .run_action(CreateEntry::from(off_timeline.clone()).into())
        .await
        .unwrap();
    assert!(
        client
            .run_action(
                ConvertToSets {
                    actor_id: user.actor_id,
                    entry_id: off_timeline.id,
                    sequence_id: Uuid::new_v4(),
                }
                .into(),
            )
            .await
            .is_err(),
        "off-timeline root conversion must be rejected"
    );

    // A member of an activity-bearing sets sequence cannot be converted: the
    // anonymous sequence would break the members' shared activity.
    let (_, members) = seed_sets_sequence(&client, user.actor_id, Some(activity.id), 2).await;
    assert!(
        client
            .run_action(
                ConvertToSets {
                    actor_id: user.actor_id,
                    entry_id: members[0].id,
                    sequence_id: Uuid::new_v4(),
                }
                .into(),
            )
            .await
            .is_err(),
        "converting a member among activity-bearing siblings must be rejected"
    );

    // Among all-anonymous members it is fine (anonymity matches).
    let (_, anon_members) = seed_sets_sequence(&client, user.actor_id, None, 2).await;
    client
        .run_action(
            ConvertToSets {
                actor_id: user.actor_id,
                entry_id: anon_members[0].id,
                sequence_id: Uuid::new_v4(),
            }
            .into(),
        )
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_duplicate_entry_exact_copy_after_source(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool, Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&client).await;

    let mut parent = log_entry(user.actor_id, None, None);
    parent.is_sequence = true;
    parent.temporal = Temporal::Start {
        start: sqlx::types::chrono::Utc::now(),
    };
    let fi1 = FractionalIndex::default();
    let fi2 = FractionalIndex::new_after(&fi1);
    let mut c1 = log_entry(user.actor_id, None, child_position(parent.id, fi1.clone()));
    c1.is_sequence = true;
    c1.is_complete = false;
    c1.temporal = Temporal::Duration { duration: 90_000 };
    let c2 = log_entry(user.actor_id, None, child_position(parent.id, fi2));
    let mut c1a = log_entry(
        user.actor_id,
        None,
        child_position(c1.id, FractionalIndex::default()),
    );
    c1a.is_complete = true;

    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: "Reps".to_string(),
        description: None,
        config: AttributeConfig::Numeric(NumericConfig {
            min: None,
            max: None,
            integer: true,
            default: None,
        }),
    };
    let value = Value {
        entry_id: c1a.id,
        attribute_id: attribute.id,
        index_float: Some(13.0),
        index_string: None,
        plan: None,
        actual: Some(AttributeValue::Numeric(NumericValue::Exact(13.0))),
    };

    run_actions(
        &client,
        [
            CreateEntry::from(parent.clone()).into(),
            CreateEntry::from(c1.clone()).into(),
            CreateEntry::from(c2.clone()).into(),
            CreateEntry::from(c1a.clone()).into(),
            CreateAttribute {
                actor_id: user.actor_id,
                attribute: attribute.clone(),
            }
            .into(),
            CreateValue {
                actor_id: user.actor_id,
                value: value.clone(),
            }
            .into(),
        ],
    )
    .await;

    client
        .run_action(
            DuplicateEntry {
                actor_id: user.actor_id,
                entry_id: c1.id,
            }
            .into(),
        )
        .await
        .unwrap();

    // The copy lands between c1 and c2 in sibling order.
    let mut conn = client.pool.acquire().await.unwrap();
    let subtree = SqliteQueryExecutor::new(&mut *conn)
        .execute(FindDescendants {
            entry_id: parent.id,
        })
        .await
        .unwrap();
    let forest = gv_core::forest::Forest::from(subtree);
    let children: Vec<Uuid> = forest.children(parent.id).iter().map(|e| e.id).collect();
    assert_eq!(children.len(), 3);
    assert_eq!(children[0], c1.id);
    assert_eq!(children[2], c2.id);
    let dup_id = children[1];
    assert_ne!(dup_id, c1.id);

    // Exact copy: temporal/completion verbatim, subtree deep-copied with
    // fresh ids, value re-keyed onto the copied grandchild.
    let dup = forest.entry(dup_id).unwrap();
    assert_eq!(dup.temporal, Temporal::Duration { duration: 90_000 });
    assert!(dup.is_sequence);
    let dup_children = forest.children(dup_id);
    assert_eq!(dup_children.len(), 1);
    let dup_grandchild = dup_children[0];
    assert_ne!(dup_grandchild.id, c1a.id);
    assert!(dup_grandchild.is_complete, "completion copies verbatim");

    let dup_value = SqliteQueryExecutor::new(&mut *conn)
        .execute(FindValueByKey {
            entry_id: dup_grandchild.id,
            attribute_id: attribute.id,
        })
        .await
        .unwrap()
        .expect("value must be re-keyed onto the copy");
    assert_eq!(
        dup_value.actual,
        Some(AttributeValue::Numeric(NumericValue::Exact(13.0)))
    );
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_duplicate_entry_roots(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool, Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&client).await;

    // A log root duplicates in place: same temporal, also a root.
    let start = sqlx::types::chrono::Utc::now();
    let mut root = log_entry(user.actor_id, None, None);
    root.temporal = Temporal::Start { start };
    client
        .run_action(CreateEntry::from(root.clone()).into())
        .await
        .unwrap();
    client
        .run_action(
            DuplicateEntry {
                actor_id: user.actor_id,
                entry_id: root.id,
            }
            .into(),
        )
        .await
        .unwrap();
    let mut conn = client.pool.acquire().await.unwrap();
    let all = SqliteQueryExecutor::new(&mut *conn)
        .execute(AllEntries)
        .await
        .unwrap();
    let copies: Vec<&Entry> = all
        .iter()
        .filter(|e| e.position.is_none() && e.temporal == Temporal::Start { start })
        .collect();
    assert_eq!(copies.len(), 2, "copy is a root with the same temporal");

    // An activity template root cannot be duplicated (it would mint a second
    // template root for the activity).
    let activity = Activity {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: ActivityName::parse("Deadlift".to_string()).unwrap(),
        description: None,
        source_activity_id: None,
    };
    let template_root_id = Uuid::new_v4();
    client
        .run_action(activity.into_create_activity(template_root_id).into())
        .await
        .unwrap();
    assert!(
        client
            .run_action(
                DuplicateEntry {
                    actor_id: user.actor_id,
                    entry_id: template_root_id,
                }
                .into(),
            )
            .await
            .is_err(),
        "template root duplication must be rejected"
    );
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_set_display_as_sets_guards(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool, Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&client).await;

    let activity = Activity {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: ActivityName::parse("Pull Up".to_string()).unwrap(),
        description: None,
        source_activity_id: None,
    };
    client
        .run_action(activity.clone().into_create_activity(Uuid::new_v4()).into())
        .await
        .unwrap();

    let set_flag = |entry_id, value| {
        UpdateEntry {
            actor_id: user.actor_id,
            entry_id,
            change: EntryChange::SetDisplayAsSets(value),
        }
        .into()
    };

    // A scalar cannot be flagged.
    let mut scalar = log_entry(user.actor_id, None, None);
    scalar.temporal = Temporal::Start {
        start: sqlx::types::chrono::Utc::now(),
    };
    client
        .run_action(CreateEntry::from(scalar.clone()).into())
        .await
        .unwrap();
    assert!(client.run_action(set_flag(scalar.id, true)).await.is_err());

    // An empty sequence cannot be flagged.
    let mut empty = log_entry(user.actor_id, None, None);
    empty.is_sequence = true;
    empty.temporal = Temporal::Start {
        start: sqlx::types::chrono::Utc::now(),
    };
    client
        .run_action(CreateEntry::from(empty.clone()).into())
        .await
        .unwrap();
    assert!(client.run_action(set_flag(empty.id, true)).await.is_err());

    // Heterogeneous members (activity + anonymous) cannot be flagged.
    let fi1 = FractionalIndex::default();
    let fi2 = FractionalIndex::new_after(&fi1);
    let m1 = log_entry(
        user.actor_id,
        Some(activity.id),
        child_position(empty.id, fi1),
    );
    let m2 = log_entry(user.actor_id, None, child_position(empty.id, fi2.clone()));
    run_actions(
        &client,
        [
            CreateEntry::from(m1.clone()).into(),
            CreateEntry::from(m2.clone()).into(),
        ],
    )
    .await;
    assert!(client.run_action(set_flag(empty.id, true)).await.is_err());

    // Homogeneous members flag fine; setting again is a no-op; a flagged
    // sequence cannot become a scalar until broken out.
    client
        .run_action(
            DeleteEntryRecursive {
                actor_id: user.actor_id,
                entry_id: m2.id,
            }
            .into(),
        )
        .await
        .unwrap();
    client.run_action(set_flag(empty.id, true)).await.unwrap();
    assert!(find_entry(&client, empty.id).await.unwrap().display_as_sets);
    client.run_action(set_flag(empty.id, true)).await.unwrap();

    let to_scalar = UpdateEntry {
        actor_id: user.actor_id,
        entry_id: empty.id,
        change: EntryChange::SetIsSequence(false),
    };
    assert!(
        client.run_action(to_scalar.clone().into()).await.is_err(),
        "flagged sequence cannot become a scalar"
    );

    // Break out, then scalar conversion works again.
    client.run_action(set_flag(empty.id, false)).await.unwrap();
    client.run_action(to_scalar.into()).await.unwrap();
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_sets_member_activity_guards(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool, Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&client).await;

    let make_activity = |name: &str| Activity {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: ActivityName::parse(name.to_string()).unwrap(),
        description: None,
        source_activity_id: None,
    };
    let activity_a = make_activity("Bench Press");
    let activity_b = make_activity("Overhead Press");
    run_actions(
        &client,
        [
            activity_a.clone().into_create_activity(Uuid::new_v4()).into(),
            activity_b.clone().into_create_activity(Uuid::new_v4()).into(),
        ],
    )
    .await;

    let (sequence, members) =
        seed_sets_sequence(&client, user.actor_id, Some(activity_a.id), 2).await;
    let append_position = || {
        child_position(
            sequence.id,
            FractionalIndex::new_after(members[1].frac_index().unwrap()),
        )
    };

    // CreateEntry into the flagged sequence: wrong activity and anonymous are
    // rejected; the shared activity is accepted.
    let wrong = log_entry(user.actor_id, Some(activity_b.id), append_position());
    assert!(
        client
            .run_action(CreateEntry::from(wrong).into())
            .await
            .is_err()
    );
    let anonymous = log_entry(user.actor_id, None, append_position());
    assert!(
        client
            .run_action(CreateEntry::from(anonymous).into())
            .await
            .is_err()
    );
    let matching = log_entry(user.actor_id, Some(activity_a.id), append_position());
    client
        .run_action(CreateEntry::from(matching.clone()).into())
        .await
        .unwrap();

    // CreateEntryFromActivity into the flagged sequence: B rejected, A fine.
    let instantiate = |activity_id| CreateEntryFromActivity {
        actor_id: user.actor_id,
        activity_id,
        position: child_position(
            sequence.id,
            FractionalIndex::new_after(matching.frac_index().unwrap()),
        ),
        temporal: Temporal::None,
        is_template: false,
    };
    assert!(
        client
            .run_action(instantiate(activity_b.id).into())
            .await
            .is_err()
    );
    client
        .run_action(instantiate(activity_a.id).into())
        .await
        .unwrap();

    // MoveEntry into the flagged sequence: wrong activity rejected, matching
    // accepted; reordering existing members is fine.
    let mut outsider = log_entry(user.actor_id, Some(activity_b.id), None);
    outsider.temporal = Temporal::Start {
        start: sqlx::types::chrono::Utc::now(),
    };
    client
        .run_action(CreateEntry::from(outsider.clone()).into())
        .await
        .unwrap();
    let move_into = |entry_id| MoveEntry {
        actor_id: user.actor_id,
        entry_id,
        position: child_position(
            sequence.id,
            FractionalIndex::new_before(members[0].frac_index().unwrap()),
        ),
        temporal: Temporal::None,
    };
    assert!(client.run_action(move_into(outsider.id).into()).await.is_err());
    client
        .run_action(move_into(members[1].id).into())
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_sets_min_member_guards(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool, Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&client).await;

    let (sequence, members) = seed_sets_sequence(&client, user.actor_id, None, 2).await;

    // Deleting down to one member is fine; deleting the last is rejected.
    client
        .run_action(
            DeleteEntryRecursive {
                actor_id: user.actor_id,
                entry_id: members[0].id,
            }
            .into(),
        )
        .await
        .unwrap();
    assert!(
        client
            .run_action(
                DeleteEntryRecursive {
                    actor_id: user.actor_id,
                    entry_id: members[1].id,
                }
                .into(),
            )
            .await
            .is_err(),
        "deleting the last member must be rejected"
    );

    // Moving the last member out is rejected.
    assert!(
        client
            .run_action(
                MoveEntry {
                    actor_id: user.actor_id,
                    entry_id: members[1].id,
                    position: None,
                    temporal: Temporal::Start {
                        start: sqlx::types::chrono::Utc::now()
                    },
                }
                .into(),
            )
            .await
            .is_err(),
        "moving the last member out must be rejected"
    );

    // The "+" flow: duplicating the last member appends a sibling copy.
    client
        .run_action(
            DuplicateEntry {
                actor_id: user.actor_id,
                entry_id: members[1].id,
            }
            .into(),
        )
        .await
        .unwrap();

    // Breaking out lifts the floor; deleting the whole sequence is always
    // fine and removes the members with it.
    client
        .run_action(
            UpdateEntry {
                actor_id: user.actor_id,
                entry_id: sequence.id,
                change: EntryChange::SetDisplayAsSets(false),
            }
            .into(),
        )
        .await
        .unwrap();
    client
        .run_action(
            DeleteEntryRecursive {
                actor_id: user.actor_id,
                entry_id: members[1].id,
            }
            .into(),
        )
        .await
        .unwrap();

    let (sequence2, _) = seed_sets_sequence(&client, user.actor_id, None, 1).await;
    client
        .run_action(
            DeleteEntryRecursive {
                actor_id: user.actor_id,
                entry_id: sequence2.id,
            }
            .into(),
        )
        .await
        .unwrap();
    assert!(find_entry(&client, sequence2.id).await.is_none());
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_create_entry_born_flagged_rejected(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool, Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&client).await;

    let mut entry = log_entry(user.actor_id, None, None);
    entry.is_sequence = true;
    entry.display_as_sets = true;
    entry.temporal = Temporal::Start {
        start: sqlx::types::chrono::Utc::now(),
    };
    assert!(
        client
            .run_action(CreateEntry::from(entry).into())
            .await
            .is_err(),
        "a new entry cannot be born with display_as_sets"
    );
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn test_sets_in_activity_templates(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool, Arc::new(gv_core::io::SystemIo::default()));
    let user = create_user(&client).await;

    let activity = Activity {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: ActivityName::parse("Core Series".to_string()).unwrap(),
        description: None,
        source_activity_id: None,
    };
    let template_entry = |activity_id, position, is_sequence, display_as_sets| Entry {
        id: Uuid::new_v4(),
        activity_id,
        owner_id: user.actor_id,
        name: Some("set".to_string()),
        position,
        is_template: true,
        display_as_sets,
        is_sequence,
        is_complete: false,
        temporal: Temporal::None,
    };

    // A template containing a flagged sequence with no members is rejected.
    let bad_root = template_entry(Some(activity.id), None, true, false);
    let bad_sets = template_entry(
        None,
        child_position(bad_root.id, FractionalIndex::default()),
        true,
        true,
    );
    assert!(
        client
            .run_action(
                CreateActivity {
                    actor_id: user.actor_id,
                    activity: activity.clone(),
                    template: vec![bad_root, bad_sets],
                }
                .into(),
            )
            .await
            .is_err(),
        "template with an empty sets sequence must be rejected"
    );

    // A valid sets template: root sequence -> flagged sequence -> 2 anonymous
    // members.
    let root = template_entry(Some(activity.id), None, true, false);
    let sets = template_entry(
        None,
        child_position(root.id, FractionalIndex::default()),
        true,
        true,
    );
    let fi1 = FractionalIndex::default();
    let fi2 = FractionalIndex::new_after(&fi1);
    let member1 = template_entry(None, child_position(sets.id, fi1), false, false);
    let member2 = template_entry(None, child_position(sets.id, fi2), false, false);
    client
        .run_action(
            CreateActivity {
                actor_id: user.actor_id,
                activity: activity.clone(),
                template: vec![root, sets.clone(), member1, member2],
            }
            .into(),
        )
        .await
        .unwrap();

    // Instantiation preserves display_as_sets into the log.
    client
        .run_action(
            CreateEntryFromActivity {
                actor_id: user.actor_id,
                activity_id: activity.id,
                position: None,
                temporal: Temporal::Start {
                    start: sqlx::types::chrono::Utc::now(),
                },
                is_template: false,
            }
            .into(),
        )
        .await
        .unwrap();
    let mut conn = client.pool.acquire().await.unwrap();
    let all = SqliteQueryExecutor::new(&mut *conn)
        .execute(AllEntries)
        .await
        .unwrap();
    let instantiated_sets: Vec<&Entry> = all
        .iter()
        .filter(|e| !e.is_template && e.display_as_sets)
        .collect();
    assert_eq!(
        instantiated_sets.len(),
        1,
        "instantiated subtree must preserve display_as_sets"
    );
    assert_ne!(instantiated_sets[0].id, sets.id, "instance has a fresh id");
    let forest = gv_core::forest::Forest::from(all.clone());
    assert_eq!(
        forest.children(instantiated_sets[0].id).len(),
        2,
        "instantiated sets sequence keeps its members"
    );
}

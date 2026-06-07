use fractional_index::FractionalIndex;
use gv_client::client::SqliteClient;
use gv_core::{
    actions::{
        Action, AttachValue, AttributeChange, CreateActivity, CreateAttribute, CreateEntry,
        CreateEntryFromActivity, CreateUser, CreateValue, DeleteAttributeValue, EntryChange,
        MassChange, MoveEntry, NumericChange, SelectChange, UpdateAttribute, UpdateEntry,
    },
    models::{
        activity::{Activity, ActivityName},
        attribute::{
            Attribute, AttributeConfig, AttributeValue, MassConfig, MassMeasurement, MassUnit,
            MassValue, NumericConfig, NumericValue, SelectConfig, Value,
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
use sqlx::SqlitePool;
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
async fn test_attach_mass_seeds_default_units(pool: SqlitePool) {
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));

    let (user, entry) = seed_entry(&sqlite_client).await;
    let user_id = user.actor_id.clone();

    // Mass attribute with two default units.
    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: user_id,
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

    // Both plan and actual seed a zero-magnitude measurement per default unit.
    let expected = vec![
        MassMeasurement {
            unit: MassUnit::Kilogram,
            value: 0.0,
        },
        MassMeasurement {
            unit: MassUnit::Pound,
            value: 0.0,
        },
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
    let sqlite_client =
        SqliteClient::from_pool(pool, std::sync::Arc::new(gv_core::io::SystemIo::default()));
    let (user, entry) = seed_entry(&sqlite_client).await;

    let attribute = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: "Load".to_string(),
        description: None,
        config: AttributeConfig::Mass(MassConfig {
            default_units: vec![],
        }),
    };
    sqlite_client
        .run_action(CreateAttribute::from(attribute.clone()).into())
        .await
        .unwrap();

    sqlite_client
        .run_action(
            AttachValue {
                actor_id: user.actor_id,
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

    // --- Mass: replace default units; common SetName edit.
    let mass = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: "Load".to_string(),
        description: None,
        config: AttributeConfig::Mass(MassConfig {
            default_units: vec![MassUnit::Kilogram],
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
                change: AttributeChange::Mass(MassChange::SetDefaultUnits(vec![
                    MassUnit::Pound,
                    MassUnit::Kilogram,
                ])),
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
            .default_units,
        vec![MassUnit::Pound, MassUnit::Kilogram]
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
async fn test_update_attribute_noop_and_dedupe(pool: SqlitePool) {
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

    // --- Mass dedupe: duplicates collapse, first-seen order preserved.
    let mass = Attribute {
        id: Uuid::new_v4(),
        owner_id: user.actor_id,
        name: "Load".to_string(),
        description: None,
        config: AttributeConfig::Mass(MassConfig {
            default_units: vec![],
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
                change: AttributeChange::Mass(MassChange::SetDefaultUnits(vec![
                    MassUnit::Kilogram,
                    MassUnit::Pound,
                    MassUnit::Kilogram,
                    MassUnit::Gram,
                ])),
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
            .default_units,
        vec![MassUnit::Kilogram, MassUnit::Pound, MassUnit::Gram]
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
        .run_action(CreateActivity::from(activity.clone()).into())
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

    // move_entry: a template root with no start/end is allowed (templates are
    // exempt from the log-root "must have start or end" rule).
    sqlite_client
        .run_action(
            MoveEntry {
                actor_id: user.actor_id,
                entry_id: none_template_id,
                position: None,
                temporal: Temporal::None,
            }
            .into(),
        )
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

//! End-to-end tests for `Importer::import_day` against a real SQLite pool:
//! plan → apply → idempotent re-run, and the failure/blocked paths.

use std::collections::BTreeMap;
use std::sync::Arc;

use gv_client::client::SqliteClient;
use gv_core::actions::{CreateAttribute, CreateUser};
use gv_core::io::SystemIo;
use gv_core::models::activity::{Activity, ActivityName};
use gv_core::models::attribute::{
    Attribute, MassConfig, MassUnit, SelectConfig, TextConfig,
};
use gv_core::models::user::User;
use gv_core::queries::{AllActivities, AllAttributes, FindEntryById, FindValueByKey};
use gv_core::validation::{Email, Username};
use gv_import::document::{DayDocument, DocEntry, SCHEMA_VERSION};
use gv_import::import::{EntryStatus, ImportConfig, Importer, ValueStatus};
use gv_import::registry::{Aliases, Registry};
use serde_json::json;
use sqlx::SqlitePool;
use uuid::Uuid;

const TZ: chrono_tz::Tz = chrono_tz::America::Denver;

async fn seed(client: &SqliteClient) -> (User, Registry) {
    let user = User {
        actor_id: Uuid::new_v4(),
        email: Email::parse("importer@test.com".to_string()).unwrap(),
        username: Username::parse("importer".to_string()).unwrap(),
    };
    client
        .run_action(CreateUser::from(user.clone()).into())
        .await
        .unwrap();

    for name in ["Climbing", "Autobelay"] {
        let activity = Activity {
            id: Uuid::new_v4(),
            owner_id: user.actor_id,
            name: ActivityName::parse(name.to_string()).unwrap(),
            description: None,
            source_activity_id: None,
        };
        client
            .run_action(activity.into_create_activity(Uuid::new_v4()).into())
            .await
            .unwrap();
    }

    let attributes: Vec<Attribute> = vec![
        Attribute {
            id: Uuid::new_v4(),
            owner_id: user.actor_id,
            name: "YDS".to_string(),
            description: None,
            config: SelectConfig {
                options: vec!["5.10".into(), "5.11".into(), "5.12".into()],
                ordered: true,
                default: None,
            }
            .into(),
        },
        Attribute {
            id: Uuid::new_v4(),
            owner_id: user.actor_id,
            name: "Outcome".to_string(),
            description: None,
            config: SelectConfig {
                options: vec!["flash".into(), "repeat".into(), "attempt".into()],
                ordered: false,
                default: None,
            }
            .into(),
        },
        Attribute {
            id: Uuid::new_v4(),
            owner_id: user.actor_id,
            name: "Location".to_string(),
            description: None,
            config: TextConfig {
                default: None,
                autocomplete: true,
            }
            .into(),
        },
        Attribute {
            id: Uuid::new_v4(),
            owner_id: user.actor_id,
            name: "Bodyweight".to_string(),
            description: None,
            config: MassConfig {
                default_unit: MassUnit::Pound,
            }
            .into(),
        },
    ];
    for attribute in attributes {
        client
            .run_action(
                CreateAttribute {
                    actor_id: user.actor_id,
                    attribute,
                }
                .into(),
            )
            .await
            .unwrap();
    }

    let registry = load_registry(client).await;
    (user, registry)
}

async fn load_registry(client: &SqliteClient) -> Registry {
    let activities = client.run_query(AllActivities {}).await.unwrap();
    let attributes = client.run_query(AllAttributes {}).await.unwrap();
    let mut aliases = Aliases::default();
    aliases
        .activities
        .insert("autobelays".to_string(), "Autobelay".to_string());
    Registry::new(activities, attributes, aliases)
}

fn day_doc(entries: Vec<DocEntry>) -> DayDocument {
    DayDocument {
        schema_version: SCHEMA_VERSION,
        source: "obsidian-training-log".to_string(),
        source_file: "2026-02/2026-02-24.md".to_string(),
        content_hash: None,
        date: chrono::NaiveDate::from_ymd_opt(2026, 2, 24).unwrap(),
        model: None,
        prompt_version: None,
        entries,
        skips: vec![],
        questions: vec![],
        applied: None,
    }
}

fn scalar(activity: Option<&str>, name: Option<&str>) -> DocEntry {
    DocEntry {
        activity: activity.map(str::to_string),
        name: name.map(str::to_string),
        start: None,
        end: None,
        duration: None,
        attributes: BTreeMap::new(),
        children: vec![],
        sequence: None,
    }
}

/// A realistic session: Climbing root (start+end, Location) with two
/// Autobelay children (via alias), plus anonymous "Woke"/"Weight" roots.
fn session_doc() -> DayDocument {
    let mut climb = scalar(Some("Climbing"), None);
    climb.start = Some("3:03pm".to_string());
    climb.end = Some("5:10pm".to_string());
    climb
        .attributes
        .insert("Location".to_string(), json!("Stone Age"));

    let mut ab1 = scalar(Some("Autobelays"), None);
    ab1.attributes.insert("YDS".to_string(), json!("5.10"));
    ab1.attributes.insert("Outcome".to_string(), json!("repeat"));
    let mut ab2 = scalar(Some("Autobelays"), None);
    ab2.attributes.insert("YDS".to_string(), json!("5.11"));
    ab2.attributes.insert("Outcome".to_string(), json!("Flash"));
    climb.children = vec![ab1, ab2];

    let mut woke = scalar(None, Some("Woke"));
    woke.start = Some("4:35am".to_string());

    let mut weight = scalar(None, Some("Weight"));
    weight.start = Some("7:31am".to_string());
    weight.attributes.insert(
        "Bodyweight".to_string(),
        json!({"value": 188.4, "unit": "lbs"}),
    );

    day_doc(vec![woke, weight, climb])
}

fn importer(pool: SqlitePool, actor_id: Uuid) -> Importer {
    Importer {
        client: SqliteClient::from_pool(pool, Arc::new(SystemIo::default())),
        config: ImportConfig {
            actor_id,
            timezone: TZ,
        },
    }
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn import_day_applies_then_rerun_is_idempotent(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);
    let doc = session_doc();

    // Dry run: everything planned, nothing written.
    let rehearsal = importer.import_day(&registry, &doc, true).await.unwrap();
    assert!(rehearsal.ok, "dry run should be clean: {:?}", rehearsal.warnings);
    assert!(rehearsal.entries.iter().all(|e| e.status == EntryStatus::Planned));
    for outcome in &rehearsal.entries {
        assert!(
            client
                .run_query(FindEntryById { entry_id: outcome.id })
                .await
                .unwrap()
                .is_none(),
            "dry run must not write"
        );
    }

    // Apply: 5 entries (woke, weight, climbing + 2 autobelays).
    let report = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(report.ok, "apply should be clean: {:?}", report.warnings);
    assert_eq!(report.entries.len(), 5);
    assert!(report.entries.iter().all(|e| e.status == EntryStatus::Created));

    // The climbing root is a sequence (incomplete); scalars are complete.
    let climb_outcome = report
        .entries
        .iter()
        .find(|e| e.path == "climbing#0")
        .unwrap();
    let climb = client
        .run_query(FindEntryById { entry_id: climb_outcome.id })
        .await
        .unwrap()
        .unwrap();
    assert!(climb.is_sequence && !climb.is_complete);
    assert!(climb.position.is_none());
    assert!(climb.temporal.start().is_some() && climb.temporal.end().is_some());

    let ab2_outcome = report
        .entries
        .iter()
        .find(|e| e.path == "climbing#0/autobelays#1")
        .unwrap();
    let ab2 = client
        .run_query(FindEntryById { entry_id: ab2_outcome.id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ab2.parent_id(), Some(climb.id));
    assert!(!ab2.is_sequence && ab2.is_complete);

    // Values landed as actuals; select matching is case-insensitive
    // ("Flash" → canonical "flash" passed validation).
    let yds = registry.resolve_attribute("YDS").unwrap();
    let value = client
        .run_query(FindValueByKey {
            entry_id: ab2.id,
            attribute_id: yds.id,
        })
        .await
        .unwrap()
        .unwrap();
    assert!(value.plan.is_none() && value.actual.is_some());

    // Re-run: same ids re-derived, everything already present, no dupes.
    let rerun = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(rerun.ok);
    assert!(rerun.entries.iter().all(|e| e.status == EntryStatus::Existed));
    assert!(
        rerun
            .entries
            .iter()
            .flat_map(|e| &e.values)
            .all(|v| v.status == ValueStatus::Existed)
    );
    assert_eq!(rerun.entry_ids, report.entry_ids);
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn rerun_with_new_child_appends_only_the_new_one(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);

    let mut doc = session_doc();
    importer.import_day(&registry, &doc, false).await.unwrap();

    // Re-extraction found a third autobelay.
    let mut ab3 = scalar(Some("Autobelays"), None);
    ab3.attributes.insert("YDS".to_string(), json!("5.12"));
    doc.entries[2].children.push(ab3);

    let report = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(report.ok);
    let created: Vec<_> = report
        .entries
        .iter()
        .filter(|e| e.status == EntryStatus::Created)
        .collect();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].path, "climbing#0/autobelays#2");

    // The new member appended after the existing ones.
    let ab3 = client
        .run_query(FindEntryById { entry_id: created[0].id })
        .await
        .unwrap()
        .unwrap();
    let ab2_id = report
        .entries
        .iter()
        .find(|e| e.path == "climbing#0/autobelays#1")
        .unwrap()
        .id;
    let ab2 = client
        .run_query(FindEntryById { entry_id: ab2_id })
        .await
        .unwrap()
        .unwrap();
    assert!(ab3.frac_index().unwrap() > ab2.frac_index().unwrap());
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn unknown_activity_fails_node_and_blocks_children(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);

    let mut root = scalar(Some("Rowing"), None); // not in the registry
    root.start = Some("6am".to_string());
    root.children = vec![scalar(None, Some("warmup"))];
    let doc = day_doc(vec![root]);

    let report = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(!report.ok);
    assert_eq!(report.entries[0].status, EntryStatus::Failed);
    assert!(report.entries[0].detail.as_ref().unwrap().contains("Rowing"));
    assert_eq!(report.entries[1].status, EntryStatus::Blocked);
    // Nothing written.
    for outcome in &report.entries {
        assert!(
            client
                .run_query(FindEntryById { entry_id: outcome.id })
                .await
                .unwrap()
                .is_none()
        );
    }
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn shape_rules_are_enforced(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);

    // Anonymous scalar without a name; root without any time.
    let nameless = scalar(None, None);
    let mut timeless = scalar(None, Some("Stretch"));
    timeless.duration = Some("10m".to_string());
    let doc = day_doc(vec![nameless, timeless]);

    let report = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(!report.ok);
    assert!(report.entries[0]
        .detail
        .as_ref()
        .unwrap()
        .contains("anonymous scalar"));
    assert!(report.entries[1]
        .detail
        .as_ref()
        .unwrap()
        .contains("start or end"));
}

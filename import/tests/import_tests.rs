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
    aliases
        .attributes
        .insert("bw".to_string(), "Bodyweight".to_string());
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

/// A child go with a grade + outcome, for building set/climb sequences.
fn child(activity: &str, yds: &str, outcome: &str) -> DocEntry {
    let mut c = scalar(Some(activity), None);
    c.attributes.insert("YDS".to_string(), json!(yds));
    c.attributes.insert("Outcome".to_string(), json!(outcome));
    c
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
async fn day_entries_returns_roots_with_subtrees(pool: SqlitePool) {
    // Exercises EntriesRootedInTimeInterval (get_day's query): its recursive CTE
    // must SELECT c.* — a bare SELECT * across the forest JOIN doubles the column
    // count and the UNION ALL is rejected. Regression guard for that.
    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);
    let doc = session_doc();

    let report = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(report.ok);

    let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 24).unwrap();
    let roots = importer.day_entries(date).await.unwrap();
    // woke, weight, climbing.
    assert_eq!(roots.len(), 3);
    let climbing = roots
        .iter()
        .find(|(root, _)| root.is_sequence)
        .expect("climbing root present");
    // Its two autobelay children come back as descendants.
    assert_eq!(climbing.1.len(), 2);
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn sets_flag_set_only_when_sequence_shares_children_activity(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);

    // A "sets" sequence: an Autobelay parent whose children are all Autobelay
    // (parent activity == members' activity). Plus the mixed Climbing session
    // (Climbing parent, Autobelay children) as the negative case.
    let mut sets = scalar(Some("Autobelay"), None);
    sets.start = Some("1:00pm".to_string());
    sets.children = vec![
        child("Autobelay", "5.10", "flash"),
        child("Autobelay", "5.11", "repeat"),
    ];
    let doc = day_doc(vec![sets, session_doc().entries.pop().unwrap()]);

    let report = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(report.ok, "apply should be clean: {:?}", report.warnings);

    let get = |path: &str| {
        let id = report.entries.iter().find(|e| e.path == path).unwrap().id;
        let client = &client;
        async move {
            client
                .run_query(FindEntryById { entry_id: id })
                .await
                .unwrap()
                .unwrap()
        }
    };

    // Same-activity exercise sequence → promoted to display_as_sets.
    let exercise = get("autobelay#0").await;
    assert!(exercise.is_sequence && exercise.display_as_sets);
    // Mixed climbing session (Climbing over Autobelay children) → left alone.
    let climb = get("climbing#0").await;
    assert!(climb.is_sequence && !climb.display_as_sets);

    // Re-run is idempotent: SetDisplayAsSets no-ops, no warnings.
    let rerun = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(rerun.ok, "rerun warnings: {:?}", rerun.warnings);
    assert!(rerun.entries.iter().all(|e| e.status == EntryStatus::Existed));
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
        .find(|e| e.path == "climbing#0/autobelay#1")
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
    assert_eq!(created[0].path, "climbing#0/autobelay#2");

    // The new member appended after the existing ones.
    let ab3 = client
        .run_query(FindEntryById { entry_id: created[0].id })
        .await
        .unwrap()
        .unwrap();
    let ab2_id = report
        .entries
        .iter()
        .find(|e| e.path == "climbing#0/autobelay#1")
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

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn rerun_preserves_app_edits_and_adds_new_values(pool: SqlitePool) {
    use gv_core::actions::{UpdateAttributeValue, ValueField};
    use gv_core::models::attribute::{AttributeValue, SelectValue};

    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);
    let mut doc = session_doc();
    importer.import_day(&registry, &doc, false).await.unwrap();

    // The user corrects a grade in the app after import.
    let report = importer.import_day(&registry, &doc, true).await.unwrap();
    let ab2_id = report
        .entries
        .iter()
        .find(|e| e.path == "climbing#0/autobelay#1")
        .unwrap()
        .id;
    let yds = registry.resolve_attribute("YDS").unwrap();
    let corrected = AttributeValue::Select(SelectValue::Exact("5.12".to_string()));
    client
        .run_action(
            UpdateAttributeValue {
                actor_id: user.actor_id,
                entry_id: ab2_id,
                attribute_id: yds.id,
                field: ValueField::Actual,
                value: Some(corrected.clone()),
            }
            .into(),
        )
        .await
        .unwrap();

    // A re-extraction adds a new attribute to that entry (via alias "bw").
    doc.entries[2].children[1]
        .attributes
        .insert("bw".to_string(), json!({"value": 188.4, "unit": "lb"}));

    let rerun = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(rerun.ok, "{:?}", rerun.warnings);

    // The app edit survived (values are never overwritten on re-run) …
    let value = client
        .run_query(FindValueByKey {
            entry_id: ab2_id,
            attribute_id: yds.id,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(value.actual, Some(corrected));

    // … while the newly-extracted attribute was created on the existing entry.
    let bodyweight = registry.resolve_attribute("bw").unwrap();
    assert_eq!(bodyweight.name, "Bodyweight");
    assert!(
        client
            .run_query(FindValueByKey {
                entry_id: ab2_id,
                attribute_id: bodyweight.id,
            })
            .await
            .unwrap()
            .is_some()
    );
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn failed_root_recovers_on_rerun_without_duplicates(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);

    let mut bad_root = scalar(Some("Rowing"), None); // unknown activity
    bad_root.start = Some("6am".to_string());
    bad_root.children = vec![scalar(None, Some("warmup"))];
    let mut good_root = scalar(None, Some("Woke"));
    good_root.start = Some("5am".to_string());
    let mut doc = day_doc(vec![good_root, bad_root]);

    let first = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(!first.ok);
    assert_eq!(first.entry_ids.len(), 1); // only the good root confirmed

    // Fix: the activity was actually Climbing.
    doc.entries[1].activity = Some("Climbing".to_string());
    let second = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(second.ok, "{:?}", second.warnings);
    let statuses: Vec<_> = second.entries.iter().map(|e| (e.path.as_str(), e.status)).collect();
    assert!(statuses.contains(&("woke#0", EntryStatus::Existed)));
    assert!(statuses.contains(&("climbing#0", EntryStatus::Created)));
    assert!(statuses.contains(&("climbing#0/warmup#0", EntryStatus::Created)));

    // No duplicates: exactly 3 imported entries exist (plus std-lib templates,
    // which are is_template).
    let entries = {
        use gv_core::queries::AllEntries;
        client.run_query(AllEntries {}).await.unwrap()
    };
    assert_eq!(entries.iter().filter(|e| !e.is_template).count(), 3);
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn alias_and_canonical_name_derive_the_same_ids(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);

    // First run extracts with the alias …
    let doc = session_doc(); // children use "Autobelays"
    let first = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(first.ok);

    // … a later prompt iteration normalizes to the canonical name. Same ids,
    // no duplicates.
    let mut canonical = session_doc();
    for child in &mut canonical.entries[2].children {
        child.activity = Some("Autobelay".to_string());
    }
    let second = importer.import_day(&registry, &canonical, false).await.unwrap();
    assert!(second.ok);
    assert!(second.entries.iter().all(|e| e.status == EntryStatus::Existed));
    assert_eq!(second.entry_ids, first.entry_ids);
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn scalar_promoted_to_sequence_when_rerun_brings_children(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);

    // Run 1: the session imported with its problems still in questions.
    let mut climb = scalar(Some("Climbing"), None);
    climb.start = Some("1pm".to_string());
    let mut doc = day_doc(vec![climb]);
    importer.import_day(&registry, &doc, false).await.unwrap();

    // Run 2: questions answered, children extracted.
    doc.entries[0].children = vec![scalar(Some("Autobelay"), None)];
    let report = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(report.ok, "{:?}", report.warnings);

    let parent = &report.entries[0];
    assert_eq!(parent.status, EntryStatus::Existed);
    assert!(parent.detail.as_deref().unwrap_or("").contains("promoted"));
    let stored = client
        .run_query(FindEntryById { entry_id: parent.id })
        .await
        .unwrap()
        .unwrap();
    assert!(stored.is_sequence && !stored.is_complete);

    let child = &report.entries[1];
    assert_eq!(child.status, EntryStatus::Created);
    let stored_child = client
        .run_query(FindEntryById { entry_id: child.id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_child.parent_id(), Some(parent.id));
}

#[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
async fn duration_only_children_and_forced_sequences_import(pool: SqlitePool) {
    let client = SqliteClient::from_pool(pool.clone(), Arc::new(SystemIo::default()));
    let (user, registry) = seed(&client).await;
    let importer = importer(pool, user.actor_id);

    let mut climb = scalar(Some("Climbing"), None);
    climb.start = Some("1pm".to_string());
    let mut plank = scalar(None, Some("Plank"));
    plank.duration = Some("40s".to_string());
    climb.children = vec![plank];

    // A sequence declared before its members are extracted.
    let mut empty_session = scalar(Some("Climbing"), None);
    empty_session.start = Some("6pm".to_string());
    empty_session.sequence = Some(true);

    let doc = day_doc(vec![climb, empty_session]);
    let report = importer.import_day(&registry, &doc, false).await.unwrap();
    assert!(report.ok, "{:?}", report.warnings);

    let plank_outcome = report
        .entries
        .iter()
        .find(|e| e.path == "climbing#0/plank#0")
        .unwrap();
    let stored_plank = client
        .run_query(FindEntryById { entry_id: plank_outcome.id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_plank.temporal.duration(), Some(40_000));

    let forced = report
        .entries
        .iter()
        .find(|e| e.path == "climbing#1")
        .unwrap();
    let stored_forced = client
        .run_query(FindEntryById { entry_id: forced.id })
        .await
        .unwrap()
        .unwrap();
    assert!(stored_forced.is_sequence && !stored_forced.is_complete);
}

//! Identity round-trip tests: `core → Row → core` for every model with
//! a bidirectional transform.
//!
//! These tests are pure-memory: they exercise the encoding/decoding
//! logic in the `Row` impls without touching a database. The DB layer
//! is tested separately in Phase B's per-model integration tests.

use chrono::{TimeZone, Utc};
use fractional_index::FractionalIndex;
use gv_core::{
    SYSTEM_ACTOR_ID,
    models::{
        activity::{Activity, ActivityName},
        attribute::{
            Attribute, AttributeConfig, AttributeValue, LengthConfig, LengthMeasurement,
            LengthUnit, LengthValue, MassConfig, MassMeasurement, MassUnit, MassValue,
            NumericConfig, NumericValue, Value,
        },
        entry::{Entry, Position, Temporal},
    },
    validation::{Email, Username},
};
use gv_sql::rows::{ActivityRow, AttributeRow, EntryRow, UserRow, ValueRow};
use uuid::Uuid;

fn sample_user() -> User {
    User {
        actor_id: Uuid::new_v4(),
        username: Username::parse("alice-1".to_string()).unwrap(),
        email: Email::parse("alice@example.com".to_string()).unwrap(),
    }
}

fn sample_activity() -> Activity {
    Activity {
        id: Uuid::new_v4(),
        owner_id: SYSTEM_ACTOR_ID,
        source_activity_id: Some(Uuid::new_v4()),
        name: ActivityName::parse("Bench Press".to_string()).unwrap(),
        description: Some("Standard barbell bench".to_string()),
    }
}

fn sample_entry_with_position() -> Entry {
    Entry {
        id: Uuid::new_v4(),
        activity_id: Some(Uuid::new_v4()),
        owner_id: SYSTEM_ACTOR_ID,
        name: Some("Set 3".to_string()),
        position: Some(Position {
            parent_id: Uuid::new_v4(),
            frac_index: FractionalIndex::default(),
        }),
        is_template: false,
        display_as_sets: true,
        is_sequence: false,
        is_complete: true,
        temporal: Temporal::StartAndDuration {
            start: Utc.with_ymd_and_hms(2026, 5, 22, 12, 34, 56).unwrap(),
            duration_ms: 90_000,
        },
    }
}

fn sample_entry_root() -> Entry {
    Entry {
        id: Uuid::new_v4(),
        activity_id: None,
        owner_id: SYSTEM_ACTOR_ID,
        name: None,
        position: None,
        is_template: true,
        display_as_sets: false,
        is_sequence: true,
        is_complete: false,
        temporal: Temporal::None,
    }
}

use gv_core::models::user::User;

#[test]
fn user_round_trips() {
    let user = sample_user();
    let row: UserRow = user.clone().into();
    let got: User = row.into();
    assert_eq!(got, user);
}

#[test]
fn activity_round_trips() {
    let activity = sample_activity();
    let row: ActivityRow = activity.clone().into();
    let got: Activity = row.into();
    assert_eq!(got, activity);
}

#[test]
fn activity_round_trips_minimal() {
    let activity = Activity {
        id: Uuid::new_v4(),
        owner_id: SYSTEM_ACTOR_ID,
        source_activity_id: None,
        name: ActivityName::parse("Squat".to_string()).unwrap(),
        description: None,
    };
    let row: ActivityRow = activity.clone().into();
    let got: Activity = row.into();
    assert_eq!(got, activity);
}

#[test]
fn entry_round_trips_with_position_and_temporal() {
    let entry = sample_entry_with_position();
    let row = EntryRow::from_entry(&entry);
    let got = row.to_entry().unwrap();
    assert_eq!(got, entry);
}

#[test]
fn entry_round_trips_root() {
    let entry = sample_entry_root();
    let row = EntryRow::from_entry(&entry);
    let got = row.to_entry().unwrap();
    assert_eq!(got, entry);
}

#[test]
fn entry_round_trips_all_temporal_variants() {
    let dt1 = Utc.with_ymd_and_hms(2026, 5, 22, 10, 0, 0).unwrap();
    let dt2 = Utc.with_ymd_and_hms(2026, 5, 22, 11, 0, 0).unwrap();
    let cases = [
        Temporal::None,
        Temporal::Start { start: dt1 },
        Temporal::End { end: dt2 },
        Temporal::Duration {
            duration: 3_600_000,
        },
        Temporal::StartAndEnd {
            start: dt1,
            end: dt2,
        },
        Temporal::StartAndDuration {
            start: dt1,
            duration_ms: 60_000,
        },
        Temporal::DurationAndEnd {
            duration_ms: 60_000,
            end: dt2,
        },
    ];
    for temporal in cases {
        let entry = Entry {
            id: Uuid::new_v4(),
            activity_id: None,
            owner_id: SYSTEM_ACTOR_ID,
            name: None,
            position: None,
            is_template: false,
            display_as_sets: false,
            is_sequence: false,
            is_complete: false,
            temporal: temporal.clone(),
        };
        let got = EntryRow::from_entry(&entry).to_entry().unwrap();
        assert_eq!(
            got.temporal, temporal,
            "temporal variant did not round-trip"
        );
    }
}

#[test]
fn attribute_round_trips_numeric() {
    let attr = Attribute {
        id: Uuid::new_v4(),
        owner_id: SYSTEM_ACTOR_ID,
        name: "Reps".to_string(),
        description: Some("Number of repetitions performed".to_string()),
        config: AttributeConfig::Numeric(
            NumericConfig::new(Some(0.0), Some(100.0), true, Some(10.0)).unwrap(),
        ),
    };
    let row = AttributeRow::from_attribute(&attr).unwrap();
    let got = row.to_attribute().unwrap();
    assert_eq!(got, attr);
}

#[test]
fn attribute_round_trips_mass() {
    let attr = Attribute {
        id: Uuid::new_v4(),
        owner_id: SYSTEM_ACTOR_ID,
        name: "Load".to_string(),
        description: None,
        config: AttributeConfig::Mass(MassConfig {
            default_unit: MassUnit::Kilogram,
        }),
    };
    let row = AttributeRow::from_attribute(&attr).unwrap();
    let got = row.to_attribute().unwrap();
    assert_eq!(got, attr);
}

#[test]
fn attribute_round_trips_length() {
    let attr = Attribute {
        id: Uuid::new_v4(),
        owner_id: SYSTEM_ACTOR_ID,
        name: "Distance".to_string(),
        description: None,
        config: AttributeConfig::Length(LengthConfig {
            default_unit: LengthUnit::Kilometer,
        }),
    };
    let row = AttributeRow::from_attribute(&attr).unwrap();
    let got = row.to_attribute().unwrap();
    assert_eq!(got, attr);
}

#[test]
fn value_round_trips_with_plan_and_actual() {
    let value = Value {
        entry_id: Uuid::new_v4(),
        attribute_id: Uuid::new_v4(),
        index_float: Some(10.5),
        index_string: None,
        plan: Some(AttributeValue::Numeric(NumericValue::Exact(10.0))),
        actual: Some(AttributeValue::Numeric(NumericValue::Exact(12.0))),
    };
    let row = ValueRow::from_value(&value).unwrap();
    let got = row.to_value().unwrap();
    assert_eq!(got, value);
}

#[test]
fn value_round_trips_mass() {
    let value = Value {
        entry_id: Uuid::new_v4(),
        attribute_id: Uuid::new_v4(),
        index_float: Some(50.0),
        index_string: None,
        plan: None,
        actual: Some(AttributeValue::Mass(MassValue::Exact(MassMeasurement {
            unit: MassUnit::Kilogram,
            value: 50.0,
        }))),
    };
    let row = ValueRow::from_value(&value).unwrap();
    let got = row.to_value().unwrap();
    assert_eq!(got, value);
}

#[test]
fn value_round_trips_length() {
    let value = Value {
        entry_id: Uuid::new_v4(),
        attribute_id: Uuid::new_v4(),
        index_float: None,
        index_string: None,
        plan: Some(AttributeValue::Length(LengthValue::Exact(
            LengthMeasurement {
                unit: LengthUnit::Mile,
                value: 3.1,
            },
        ))),
        actual: Some(AttributeValue::Length(LengthValue::Range {
            unit: LengthUnit::Meter,
            min: 100.0,
            max: 200.0,
        })),
    };
    let row = ValueRow::from_value(&value).unwrap();
    let got = row.to_value().unwrap();
    assert_eq!(got, value);
}

#[test]
fn value_round_trips_empty() {
    let value = Value {
        entry_id: Uuid::new_v4(),
        attribute_id: Uuid::new_v4(),
        index_float: None,
        index_string: None,
        plan: None,
        actual: None,
    };
    let row = ValueRow::from_value(&value).unwrap();
    let got = row.to_value().unwrap();
    assert_eq!(got, value);
}

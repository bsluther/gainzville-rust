use chrono::{DateTime, Utc};
use std::fmt::Debug;
use uuid::Uuid;

use crate::{
    models::{
        activity::Activity,
        actor::Actor,
        attribute::{Attribute, AttributeValue, Value},
        attribute_pair::AttributePair,
        entry::Entry,
        entry_join::EntryJoin,
        user::User,
    },
    validation::{Email, Username},
};

mod sealed {
    pub trait Sealed {}
}

pub trait Query: sealed::Sealed + Clone + Debug + Send + 'static {
    type Response: Clone + Debug + Send + 'static;
}

macro_rules! define_query {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident $body:tt => $response:ty
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        $vis struct $name $body

        impl sealed::Sealed for $name {}
        impl Query for $name {
            type Response = $response;
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AnyQuery {
    // Auth
    IsEmailRegistered(IsEmailRegistered),
    FindUserById(FindUserById),
    FindUserByUsername(FindUserByUsername),
    AllActorIds(AllActorIds),
    // Activity
    FindActivityById(FindActivityById),
    AllActivities(AllActivities),
    FindActivityTemplateRoot(FindActivityTemplateRoot),
    // Entry
    AllEntries(AllEntries),
    EntriesRootedInTimeInterval(EntriesRootedInTimeInterval),
    RootEntriesInInterval(RootEntriesInInterval),
    FindAncestors(FindAncestors),
    FindEntryById(FindEntryById),
    FindEntryJoinById(FindEntryJoinById),
    FindDescendants(FindDescendants),
    // Attribute
    FindAttributeById(FindAttributeById),
    AllAttributes(AllAttributes),
    FindAttributesByOwner(FindAttributesByOwner),
    // Value
    FindValueByKey(FindValueByKey),
    FindValuesForEntry(FindValuesForEntry),
    FindValuesForEntries(FindValuesForEntries),
    FindAttributePairsForEntry(FindAttributePairsForEntry),
    DistinctTextValuesForAttribute(DistinctTextValuesForAttribute),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnyQueryResponse {
    // Auth
    IsEmailRegistered(bool),
    FindUserById(Option<User>),
    FindUserByUsername(Option<User>),
    AllActorIds(Vec<Uuid>),
    // Activity
    FindActivityById(Option<Activity>),
    AllActivities(Vec<Activity>),
    FindActivityTemplateRoot(Option<Entry>),
    // Entry
    AllEntries(Vec<Entry>),
    EntriesRootedInTimeInterval(Vec<Entry>),
    RootEntriesInInterval(Vec<Entry>),
    FindAncestors(Vec<Uuid>),
    FindEntryById(Option<Entry>),
    FindEntryJoinById(Option<EntryJoin>),
    FindDescendants(Vec<Entry>),
    // Attribute
    FindAttributeById(Option<Attribute>),
    AllAttributes(Vec<Attribute>),
    FindAttributesByOwner(Vec<Attribute>),
    // Value
    FindValueByKey(Option<Value>),
    FindValuesForEntry(Vec<Value>),
    FindValuesForEntries(Vec<Value>),
    FindAttributePairsForEntry(Vec<AttributePair>),
    DistinctTextValuesForAttribute(Vec<String>),
}

impl From<IsEmailRegistered> for AnyQuery {
    fn from(value: IsEmailRegistered) -> Self {
        AnyQuery::IsEmailRegistered(value)
    }
}

impl From<FindUserById> for AnyQuery {
    fn from(value: FindUserById) -> Self {
        AnyQuery::FindUserById(value)
    }
}

impl From<FindUserByUsername> for AnyQuery {
    fn from(value: FindUserByUsername) -> Self {
        AnyQuery::FindUserByUsername(value)
    }
}

impl From<AllActorIds> for AnyQuery {
    fn from(value: AllActorIds) -> Self {
        AnyQuery::AllActorIds(value)
    }
}

impl From<FindActivityById> for AnyQuery {
    fn from(value: FindActivityById) -> Self {
        AnyQuery::FindActivityById(value)
    }
}

impl From<AllActivities> for AnyQuery {
    fn from(value: AllActivities) -> Self {
        AnyQuery::AllActivities(value)
    }
}

impl From<FindActivityTemplateRoot> for AnyQuery {
    fn from(value: FindActivityTemplateRoot) -> Self {
        AnyQuery::FindActivityTemplateRoot(value)
    }
}

impl From<AllEntries> for AnyQuery {
    fn from(value: AllEntries) -> Self {
        AnyQuery::AllEntries(value)
    }
}

impl From<EntriesRootedInTimeInterval> for AnyQuery {
    fn from(value: EntriesRootedInTimeInterval) -> Self {
        AnyQuery::EntriesRootedInTimeInterval(value)
    }
}

impl From<RootEntriesInInterval> for AnyQuery {
    fn from(value: RootEntriesInInterval) -> Self {
        AnyQuery::RootEntriesInInterval(value)
    }
}

impl From<FindAncestors> for AnyQuery {
    fn from(value: FindAncestors) -> Self {
        AnyQuery::FindAncestors(value)
    }
}

impl From<FindEntryById> for AnyQuery {
    fn from(value: FindEntryById) -> Self {
        AnyQuery::FindEntryById(value)
    }
}

impl From<FindEntryJoinById> for AnyQuery {
    fn from(value: FindEntryJoinById) -> Self {
        AnyQuery::FindEntryJoinById(value)
    }
}

impl From<FindDescendants> for AnyQuery {
    fn from(value: FindDescendants) -> Self {
        AnyQuery::FindDescendants(value)
    }
}

impl From<FindAttributeById> for AnyQuery {
    fn from(value: FindAttributeById) -> Self {
        AnyQuery::FindAttributeById(value)
    }
}

impl From<AllAttributes> for AnyQuery {
    fn from(value: AllAttributes) -> Self {
        AnyQuery::AllAttributes(value)
    }
}

impl From<FindAttributesByOwner> for AnyQuery {
    fn from(value: FindAttributesByOwner) -> Self {
        AnyQuery::FindAttributesByOwner(value)
    }
}

impl From<FindValueByKey> for AnyQuery {
    fn from(value: FindValueByKey) -> Self {
        AnyQuery::FindValueByKey(value)
    }
}

impl From<FindValuesForEntry> for AnyQuery {
    fn from(value: FindValuesForEntry) -> Self {
        AnyQuery::FindValuesForEntry(value)
    }
}

impl From<FindValuesForEntries> for AnyQuery {
    fn from(value: FindValuesForEntries) -> Self {
        AnyQuery::FindValuesForEntries(value)
    }
}

impl From<FindAttributePairsForEntry> for AnyQuery {
    fn from(value: FindAttributePairsForEntry) -> Self {
        AnyQuery::FindAttributePairsForEntry(value)
    }
}

impl From<DistinctTextValuesForAttribute> for AnyQuery {
    fn from(value: DistinctTextValuesForAttribute) -> Self {
        AnyQuery::DistinctTextValuesForAttribute(value)
    }
}

// --- Auth ---

define_query! {
    pub struct IsEmailRegistered { pub email: Email } => bool
}

define_query! {
    pub struct FindUserById { pub actor_id: Uuid } => Option<User>
}

define_query! {
    pub struct FindUserByUsername { pub username: Username } => Option<User>
}

define_query! {
    pub struct AllActorIds; => Vec<Uuid>
}

// --- Activity ---

define_query! {
    pub struct FindActivityById { pub id: Uuid } => Option<Activity>
}

define_query! {
    pub struct AllActivities; => Vec<Activity>
}

define_query! {
    /// The root template entry for an activity (parentless, `is_template`,
    /// matching `activity_id`). `CreateActivity` guarantees exactly one.
    pub struct FindActivityTemplateRoot { pub activity_id: Uuid } => Option<Entry>
}

// --- Entry ---

define_query! {
    pub struct AllEntries; => Vec<Entry>
}

define_query! {
    pub struct EntriesRootedInTimeInterval {
        pub from: DateTime<Utc>,
        pub to: DateTime<Utc>,
    } => Vec<Entry>
}

define_query! {
    /// Log roots (parentless, non-template entries) whose canonical instant falls in the half-open
    /// interval `[from, to)`, sorted by canonical instant (ties by id). Same semantics as
    /// `Forest::roots_in`, but answered by the database: this is what a log day shows.
    ///
    /// Unlike `EntriesRootedInTimeInterval` it returns roots only (no descendants), is half-open
    /// (an entry at the next day's midnight is not on this day), and places end-anchored entries
    /// (`End`, `DurationAndEnd`) by their canonical instant rather than by the `start_time` column.
    /// Executors pre-filter in SQL with [`ROOT_INTERVAL_END_ANCHOR_SLACK_MS`] and apply the exact
    /// predicate with [`log_roots_in_interval`], so both backends agree.
    pub struct RootEntriesInInterval {
        pub from: DateTime<Utc>,
        pub to: DateTime<Utc>,
    } => Vec<Entry>
}

/// For an end-anchored root (no `start_time` column), the canonical instant is `end - duration`,
/// with `duration <= u32::MAX` ms. So `canonical < to` implies `end < to + u32::MAX ms`: executors
/// use this as the SQL pre-filter's upper bound on `end_time` for start-less rows.
pub const ROOT_INTERVAL_END_ANCHOR_SLACK_MS: i64 = u32::MAX as i64;

/// The exact predicate and order behind `RootEntriesInInterval`: parentless, non-template entries
/// whose canonical instant is in `[from, to)`, sorted by (canonical instant, id). Shared by the
/// SQLite and Postgres executors (which only pre-filter in SQL) so the two cannot drift.
pub fn log_roots_in_interval(
    entries: impl IntoIterator<Item = Entry>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Vec<Entry> {
    let mut roots: Vec<Entry> = entries
        .into_iter()
        .filter(|e| {
            e.parent_id().is_none()
                && !e.is_template
                && e.temporal
                    .canonical_instant()
                    .is_some_and(|t| from <= t && t < to)
        })
        .collect();
    roots.sort_by_key(|e| (e.temporal.canonical_instant(), e.id));
    roots
}

define_query! {
    /// Where e is the entry with id == `entry_id`, this query returns a list of all the ancestor of
    /// e, including e. The result is sorted in ascending order so that e is first and the root is
    /// last, e.g. [e, ..., root_ancestor].
    pub struct FindAncestors { pub entry_id: Uuid } => Vec<Uuid>
}

define_query! {
    pub struct FindEntryById { pub entry_id: Uuid } => Option<Entry>
}

define_query! {
    pub struct FindEntryJoinById { pub entry_id: Uuid } => Option<EntryJoin>
}

define_query! {
    pub struct FindDescendants { pub entry_id: Uuid } => Vec<Entry>
}

// --- Attribute ---

define_query! {
    pub struct FindAttributeById { pub attribute_id: Uuid } => Option<Attribute>
}

define_query! {
    pub struct AllAttributes; => Vec<Attribute>
}

define_query! {
    /// `owner_id` is the query dimension: returns all attributes owned by this actor.
    pub struct FindAttributesByOwner { pub owner_id: Uuid } => Vec<Attribute>
}

// --- Value ---

define_query! {
    pub struct FindValueByKey {
        pub entry_id: Uuid,
        pub attribute_id: Uuid,
    } => Option<Value>
}

define_query! {
    pub struct FindValuesForEntry { pub entry_id: Uuid } => Vec<Value>
}

define_query! {
    /// `entry_ids` is owned (not borrowed) so that this struct can implement `Clone` and be stored
    /// in the `AnyQuery` enum.
    pub struct FindValuesForEntries { pub entry_ids: Vec<Uuid> } => Vec<Value>
}

define_query! {
    pub struct FindAttributePairsForEntry { pub entry_id: Uuid } => Vec<AttributePair>
}

define_query! {
    /// Distinct text values previously entered for an attribute (plan ∪
    /// actual), sorted — the source for a text attribute's input autocomplete.
    /// Scoped by `attribute_id` alone: the value-owner invariant guarantees
    /// every value for an attribute belongs to that attribute's owner, so no
    /// owner filter is needed.
    pub struct DistinctTextValuesForAttribute { pub attribute_id: Uuid } => Vec<String>
}

/// Distinct text values across a set of values' `plan` and `actual` fields,
/// sorted. Backs `DistinctTextValuesForAttribute` — "any text you've entered
/// for this attribute". Shared by the SQLite and Postgres executors so the
/// plan ∪ actual extraction lives in one place; non-text values are ignored.
pub fn distinct_text_values(values: &[Value]) -> Vec<String> {
    let mut set = std::collections::BTreeSet::new();
    for value in values {
        for field in [&value.plan, &value.actual].into_iter().flatten() {
            if let AttributeValue::Text(s) = field {
                set.insert(s.clone());
            }
        }
    }
    set.into_iter().collect()
}

// --- Simulation ---

// SnapshotAll is used to read *every* row from the database, regardless of auth, to bootstrap a
// model of the current state of the world. Should never be used on a production client/server.
define_query! {
    pub struct SnapshotAll; => Snapshot
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub users: Vec<User>,
    pub actors: Vec<Actor>,
    pub activities: Vec<Activity>,
    pub attributes: Vec<Attribute>,
    pub entries: Vec<Entry>,
    pub values: Vec<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::attribute::NumericValue;

    fn value(plan: Option<AttributeValue>, actual: Option<AttributeValue>) -> Value {
        Value {
            entry_id: Uuid::nil(),
            attribute_id: Uuid::nil(),
            index_float: None,
            index_string: None,
            plan,
            actual,
        }
    }

    fn text(s: &str) -> Option<AttributeValue> {
        Some(AttributeValue::Text(s.to_string()))
    }

    #[test]
    fn distinct_text_values_unions_plan_and_actual_sorted_deduped() {
        let values = vec![
            value(text("Stone Age"), text("Movement")),
            value(None, text("Stone Age")), // duplicate across rows
            value(text("Bouldering Project"), None),
        ];
        assert_eq!(
            distinct_text_values(&values),
            vec![
                "Bouldering Project".to_string(),
                "Movement".to_string(),
                "Stone Age".to_string(),
            ]
        );
    }

    #[test]
    fn log_roots_in_interval_matches_forest_roots_in() {
        use crate::forest::Forest;
        use crate::models::entry::{Entry, Position, Temporal};
        use chrono::{Duration, TimeZone};

        let day = Utc.with_ymd_and_hms(2026, 10, 30, 0, 0, 0).unwrap();
        let next = day + Duration::days(1);
        let root = |temporal: Temporal, is_template: bool| Entry {
            id: Uuid::new_v4(),
            activity_id: None,
            owner_id: Uuid::nil(),
            name: None,
            position: None,
            is_template,
            display_as_sets: false,
            is_sequence: true,
            is_complete: false,
            temporal,
        };
        let in_day = root(
            Temporal::Start {
                start: day + Duration::hours(9),
            },
            false,
        );
        let at_start = root(Temporal::Start { start: day }, false);
        let at_next_midnight = root(Temporal::Start { start: next }, false); // half-open: excluded
        // End-anchored: starts 23:00 (in day), ends 01:00 next day.
        let crosses_midnight = root(
            Temporal::DurationAndEnd {
                duration_ms: 2 * 3_600_000,
                end: next + Duration::hours(1),
            },
            false,
        );
        let end_only = root(
            Temporal::End {
                end: day + Duration::hours(7),
            },
            false,
        );
        let template = root(
            Temporal::Start {
                start: day + Duration::hours(10),
            },
            true,
        );
        let undated = root(Temporal::None, false);
        let mut child = root(
            Temporal::Start {
                start: day + Duration::hours(11),
            },
            false,
        );
        child.position = Some(Position {
            parent_id: in_day.id,
            frac_index: Default::default(),
        });

        let all = vec![
            in_day.clone(),
            at_start.clone(),
            at_next_midnight,
            crosses_midnight.clone(),
            end_only.clone(),
            template,
            undated,
            child,
        ];
        let got: Vec<Uuid> = log_roots_in_interval(all.clone(), day, next)
            .into_iter()
            .map(|e| e.id)
            .collect();
        let forest = Forest::from(all);
        let expected: Vec<Uuid> = forest
            .roots_in(day..next)
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(got, expected);
        assert_eq!(
            got,
            vec![at_start.id, end_only.id, in_day.id, crosses_midnight.id]
        );
    }

    #[test]
    fn distinct_text_values_ignores_non_text() {
        let values = vec![value(
            Some(AttributeValue::Numeric(NumericValue::Exact(5.0))),
            text("Note"),
        )];
        assert_eq!(distinct_text_values(&values), vec!["Note".to_string()]);
    }
}

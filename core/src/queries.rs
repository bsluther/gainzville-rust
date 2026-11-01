//! Questions with typed answers, and the sets of them each context accepts.
//!
//! - A **question** is a struct plus a [`Query`] impl naming its answer type. Each is defined once,
//!   in the crate that can name its answer and that every backing which answers it can see: here
//!   for questions answered by SQL executors (`gv-sql` sees only this crate), `gv-ui` for questions
//!   answered from the forest that return UI types.
//! - A **query set** is a flat enum of the questions one context accepts, with its response enum,
//!   `From` impls, and dispatch. Membership, not nesting: a question several contexts need is a
//!   plain variant in each set; no set wraps another.
//!
//! Sets defined so far:
//! - [`DomainQuery`] (here): what mutators and the server ask. Every member must be answerable by
//!   both the SQLite and Postgres executors (`DomainQueryExecutor`).
//! - `gv_client::queries::ClientQuery`: what Swift subscribes to and reads.
//!
//! `gv-ffi` restates each question and set as a uniffi remote declaration (`gv-ffi/src/types.rs`).

use chrono::{DateTime, Utc};
use std::{fmt::Debug, hash::Hash};
use uuid::Uuid;

use crate::{
    error::Result,
    models::{
        activity::Activity,
        actor::Actor,
        attribute::{Attribute, AttributeValue, Value},
        attribute_pair::AttributePair,
        entry::Entry,
        entry_join::EntryJoin,
        user::User,
    },
    query_executor::DomainQueryExecutor,
    validation::{Email, Username},
};

/// A question with a typed answer.
///
/// Not sealed: questions answered from the forest that return UI types (e.g. `gv_ui::ChildSlots`)
/// are defined outside `gv-core`. What the seal used to guarantee now comes from elsewhere:
/// exhaustive dispatch from the set enums, "can this run on Postgres" from each set's executor
/// bound, and the key requirements (a question is a subscription key) from the bounds below.
pub trait Query: Clone + Debug + PartialEq + Eq + Hash + Send + 'static {
    type Response: Clone + Debug + PartialEq + Send + 'static;
}

macro_rules! define_query {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident $body:tt => $response:ty
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        $vis struct $name $body

        impl Query for $name {
            type Response = $response;
        }
    };
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
    /// The root template entry for an activity (parentless, `is_template`, matching
    /// `activity_id`). `CreateActivity` guarantees exactly one.
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
    /// Log roots (parentless, non-template entries) whose canonical instant falls in the
    /// half-open interval `[from, to)`, sorted by canonical instant (ties by id). Same
    /// semantics as `Forest::roots_in`, but answered by the database: this is what a log day
    /// shows. Client-only (SQLite): not in the domain set, so Postgres needn't answer it.
    ///
    /// Unlike `EntriesRootedInTimeInterval` it returns roots only (no descendants), is
    /// half-open (an entry at the next day's midnight is not on this day), and places
    /// end-anchored entries (`End`, `DurationAndEnd`) by their canonical instant rather than
    /// by the `start_time` column. The executor pre-filters in SQL with
    /// `ROOT_INTERVAL_END_ANCHOR_SLACK_MS` and applies the exact predicate with
    /// `log_roots_in_interval`.
    pub struct RootEntriesInInterval {
        pub from: DateTime<Utc>,
        pub to: DateTime<Utc>,
    } => Vec<Entry>
}

define_query! {
    /// Where e is the entry with id == `entry_id`, this query returns a list of all the
    /// ancestor of e, including e. The result is sorted in ascending order so that e is first
    /// and the root is last, e.g. [e, ..., root_ancestor].
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
    /// `entry_ids` is owned (not borrowed) so that this struct can implement `Clone` and be
    /// stored in a set enum.
    pub struct FindValuesForEntries { pub entry_ids: Vec<Uuid> } => Vec<Value>
}

define_query! {
    pub struct FindAttributePairsForEntry { pub entry_id: Uuid } => Vec<AttributePair>
}

define_query! {
    /// Distinct text values previously entered for an attribute (plan ∪ actual), sorted — the
    /// source for a text attribute's input autocomplete. Scoped by `attribute_id` alone: the
    /// value-owner invariant guarantees every value for an attribute belongs to that
    /// attribute's owner, so no owner filter is needed.
    pub struct DistinctTextValuesForAttribute { pub attribute_id: Uuid } => Vec<String>
}

// --- The domain set ---

/// What mutators and the server can ask: every member is answerable by both the SQLite
/// and Postgres executors (`DomainQueryExecutor`, the bound every mutator takes).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DomainQuery {
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

/// The answer to a [`DomainQuery`]: one variant per member.
#[derive(Debug, Clone, PartialEq)]
pub enum DomainQueryResponse {
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

impl DomainQuery {
    /// Answer on any executor that implements the whole set.
    pub async fn execute<E: DomainQueryExecutor>(
        self,
        exec: &mut E,
    ) -> Result<DomainQueryResponse> {
        use DomainQueryResponse as R;
        match self {
            // Auth
            DomainQuery::IsEmailRegistered(q) => exec.execute(q).await.map(R::IsEmailRegistered),
            DomainQuery::FindUserById(q) => exec.execute(q).await.map(R::FindUserById),
            DomainQuery::FindUserByUsername(q) => exec.execute(q).await.map(R::FindUserByUsername),
            DomainQuery::AllActorIds(q) => exec.execute(q).await.map(R::AllActorIds),
            // Activity
            DomainQuery::FindActivityById(q) => exec.execute(q).await.map(R::FindActivityById),
            DomainQuery::AllActivities(q) => exec.execute(q).await.map(R::AllActivities),
            DomainQuery::FindActivityTemplateRoot(q) => {
                exec.execute(q).await.map(R::FindActivityTemplateRoot)
            }
            // Entry
            DomainQuery::AllEntries(q) => exec.execute(q).await.map(R::AllEntries),
            DomainQuery::EntriesRootedInTimeInterval(q) => {
                exec.execute(q).await.map(R::EntriesRootedInTimeInterval)
            }
            DomainQuery::FindAncestors(q) => exec.execute(q).await.map(R::FindAncestors),
            DomainQuery::FindEntryById(q) => exec.execute(q).await.map(R::FindEntryById),
            DomainQuery::FindEntryJoinById(q) => exec.execute(q).await.map(R::FindEntryJoinById),
            DomainQuery::FindDescendants(q) => exec.execute(q).await.map(R::FindDescendants),
            // Attribute
            DomainQuery::FindAttributeById(q) => exec.execute(q).await.map(R::FindAttributeById),
            DomainQuery::AllAttributes(q) => exec.execute(q).await.map(R::AllAttributes),
            DomainQuery::FindAttributesByOwner(q) => {
                exec.execute(q).await.map(R::FindAttributesByOwner)
            }
            // Value
            DomainQuery::FindValueByKey(q) => exec.execute(q).await.map(R::FindValueByKey),
            DomainQuery::FindValuesForEntry(q) => exec.execute(q).await.map(R::FindValuesForEntry),
            DomainQuery::FindValuesForEntries(q) => {
                exec.execute(q).await.map(R::FindValuesForEntries)
            }
            DomainQuery::FindAttributePairsForEntry(q) => {
                exec.execute(q).await.map(R::FindAttributePairsForEntry)
            }
            DomainQuery::DistinctTextValuesForAttribute(q) => {
                exec.execute(q).await.map(R::DistinctTextValuesForAttribute)
            }
        }
    }
}

impl From<IsEmailRegistered> for DomainQuery {
    fn from(query: IsEmailRegistered) -> Self {
        DomainQuery::IsEmailRegistered(query)
    }
}

impl From<FindUserById> for DomainQuery {
    fn from(query: FindUserById) -> Self {
        DomainQuery::FindUserById(query)
    }
}

impl From<FindUserByUsername> for DomainQuery {
    fn from(query: FindUserByUsername) -> Self {
        DomainQuery::FindUserByUsername(query)
    }
}

impl From<AllActorIds> for DomainQuery {
    fn from(query: AllActorIds) -> Self {
        DomainQuery::AllActorIds(query)
    }
}

impl From<FindActivityById> for DomainQuery {
    fn from(query: FindActivityById) -> Self {
        DomainQuery::FindActivityById(query)
    }
}

impl From<AllActivities> for DomainQuery {
    fn from(query: AllActivities) -> Self {
        DomainQuery::AllActivities(query)
    }
}

impl From<FindActivityTemplateRoot> for DomainQuery {
    fn from(query: FindActivityTemplateRoot) -> Self {
        DomainQuery::FindActivityTemplateRoot(query)
    }
}

impl From<AllEntries> for DomainQuery {
    fn from(query: AllEntries) -> Self {
        DomainQuery::AllEntries(query)
    }
}

impl From<EntriesRootedInTimeInterval> for DomainQuery {
    fn from(query: EntriesRootedInTimeInterval) -> Self {
        DomainQuery::EntriesRootedInTimeInterval(query)
    }
}

impl From<FindAncestors> for DomainQuery {
    fn from(query: FindAncestors) -> Self {
        DomainQuery::FindAncestors(query)
    }
}

impl From<FindEntryById> for DomainQuery {
    fn from(query: FindEntryById) -> Self {
        DomainQuery::FindEntryById(query)
    }
}

impl From<FindEntryJoinById> for DomainQuery {
    fn from(query: FindEntryJoinById) -> Self {
        DomainQuery::FindEntryJoinById(query)
    }
}

impl From<FindDescendants> for DomainQuery {
    fn from(query: FindDescendants) -> Self {
        DomainQuery::FindDescendants(query)
    }
}

impl From<FindAttributeById> for DomainQuery {
    fn from(query: FindAttributeById) -> Self {
        DomainQuery::FindAttributeById(query)
    }
}

impl From<AllAttributes> for DomainQuery {
    fn from(query: AllAttributes) -> Self {
        DomainQuery::AllAttributes(query)
    }
}

impl From<FindAttributesByOwner> for DomainQuery {
    fn from(query: FindAttributesByOwner) -> Self {
        DomainQuery::FindAttributesByOwner(query)
    }
}

impl From<FindValueByKey> for DomainQuery {
    fn from(query: FindValueByKey) -> Self {
        DomainQuery::FindValueByKey(query)
    }
}

impl From<FindValuesForEntry> for DomainQuery {
    fn from(query: FindValuesForEntry) -> Self {
        DomainQuery::FindValuesForEntry(query)
    }
}

impl From<FindValuesForEntries> for DomainQuery {
    fn from(query: FindValuesForEntries) -> Self {
        DomainQuery::FindValuesForEntries(query)
    }
}

impl From<FindAttributePairsForEntry> for DomainQuery {
    fn from(query: FindAttributePairsForEntry) -> Self {
        DomainQuery::FindAttributePairsForEntry(query)
    }
}

impl From<DistinctTextValuesForAttribute> for DomainQuery {
    fn from(query: DistinctTextValuesForAttribute) -> Self {
        DomainQuery::DistinctTextValuesForAttribute(query)
    }
}

/// For an end-anchored root (no `start_time` column), the canonical instant is `end - duration`,
/// with `duration <= u32::MAX` ms. So `canonical < to` implies `end < to + u32::MAX ms`: the
/// executor uses this as the SQL pre-filter's upper bound on `end_time` for start-less rows.
pub const ROOT_INTERVAL_END_ANCHOR_SLACK_MS: i64 = u32::MAX as i64;

/// The exact predicate and order behind `RootEntriesInInterval`: parentless, non-template entries
/// whose canonical instant is in `[from, to)`, sorted by (canonical instant, id). The SQLite
/// executor only pre-filters in SQL; this keeps it in step with `Forest::roots_in` (tested below).
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

define_query! {
    // SnapshotAll is used to read *every* row from the database, regardless of auth, to bootstrap
    // a model of the current state of the world. Should never be used on a production
    // client/server. Not in any set and not exposed over the FFI.
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

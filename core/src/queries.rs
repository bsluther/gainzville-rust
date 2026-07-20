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
    fn distinct_text_values_ignores_non_text() {
        let values = vec![value(
            Some(AttributeValue::Numeric(NumericValue::Exact(5.0))),
            text("Note"),
        )];
        assert_eq!(distinct_text_values(&values), vec!["Note".to_string()]);
    }
}

use chrono::{DateTime, Utc};
use std::fmt::Debug;
use uuid::Uuid;

use crate::{
    models::{
        activity::Activity,
        attribute::{Attribute, Value},
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
        #[derive(Debug, Clone)]
        $vis struct $name $body

        impl sealed::Sealed for $name {}
        impl Query for $name {
            type Response = $response;
        }
    };
}

#[derive(Debug, Clone)]
pub enum AnyQuery {
    // Auth
    IsEmailRegistered(IsEmailRegistered),
    FindUserById(FindUserById),
    FindUserByUsername(FindUserByUsername),
    AllActorIds(AllActorIds),
    // Activity
    FindActivityById(FindActivityById),
    AllActivities(AllActivities),
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

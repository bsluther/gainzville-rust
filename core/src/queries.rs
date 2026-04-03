// TODO: encode return types. Each struct's execute method returns a concrete type, but there is
// no shared abstraction binding a Query variant to its output. A trait like:
//
//   trait ExecutableQuery<DB, R> { type Output; async fn execute(...) -> Result<Self::Output>; }
//
// would enable generic run_query dispatch without a QueryResult enum. The right shape depends on
// subscription model needs and whether Output must be object-safe.

use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    error::Result,
    models::{
        activity::Activity,
        attribute::{Attribute, Value},
        attribute_pair::AttributePair,
        entry::Entry,
        entry_join::EntryJoin,
        user::User,
    },
    reader::Reader,
    validation::{Email, Username},
};

#[derive(Debug, Clone)]
pub enum Query {
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

impl From<IsEmailRegistered> for Query {
    fn from(value: IsEmailRegistered) -> Self {
        Query::IsEmailRegistered(value)
    }
}

impl From<FindUserById> for Query {
    fn from(value: FindUserById) -> Self {
        Query::FindUserById(value)
    }
}

impl From<FindUserByUsername> for Query {
    fn from(value: FindUserByUsername) -> Self {
        Query::FindUserByUsername(value)
    }
}

impl From<AllActorIds> for Query {
    fn from(value: AllActorIds) -> Self {
        Query::AllActorIds(value)
    }
}

impl From<FindActivityById> for Query {
    fn from(value: FindActivityById) -> Self {
        Query::FindActivityById(value)
    }
}

impl From<AllActivities> for Query {
    fn from(value: AllActivities) -> Self {
        Query::AllActivities(value)
    }
}

impl From<AllEntries> for Query {
    fn from(value: AllEntries) -> Self {
        Query::AllEntries(value)
    }
}

impl From<EntriesRootedInTimeInterval> for Query {
    fn from(value: EntriesRootedInTimeInterval) -> Self {
        Query::EntriesRootedInTimeInterval(value)
    }
}

impl From<FindAncestors> for Query {
    fn from(value: FindAncestors) -> Self {
        Query::FindAncestors(value)
    }
}

impl From<FindEntryById> for Query {
    fn from(value: FindEntryById) -> Self {
        Query::FindEntryById(value)
    }
}

impl From<FindEntryJoinById> for Query {
    fn from(value: FindEntryJoinById) -> Self {
        Query::FindEntryJoinById(value)
    }
}

impl From<FindDescendants> for Query {
    fn from(value: FindDescendants) -> Self {
        Query::FindDescendants(value)
    }
}

impl From<FindAttributeById> for Query {
    fn from(value: FindAttributeById) -> Self {
        Query::FindAttributeById(value)
    }
}

impl From<AllAttributes> for Query {
    fn from(value: AllAttributes) -> Self {
        Query::AllAttributes(value)
    }
}

impl From<FindAttributesByOwner> for Query {
    fn from(value: FindAttributesByOwner) -> Self {
        Query::FindAttributesByOwner(value)
    }
}

impl From<FindValueByKey> for Query {
    fn from(value: FindValueByKey) -> Self {
        Query::FindValueByKey(value)
    }
}

impl From<FindValuesForEntry> for Query {
    fn from(value: FindValuesForEntry) -> Self {
        Query::FindValuesForEntry(value)
    }
}

impl From<FindValuesForEntries> for Query {
    fn from(value: FindValuesForEntries) -> Self {
        Query::FindValuesForEntries(value)
    }
}

impl From<FindAttributePairsForEntry> for Query {
    fn from(value: FindAttributePairsForEntry) -> Self {
        Query::FindAttributePairsForEntry(value)
    }
}

// --- Auth ---

#[derive(Debug, Clone)]
pub struct IsEmailRegistered {
    pub email: Email,
}

impl IsEmailRegistered {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<bool>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::is_email_registered(connection, self.email.clone()).await
    }
}

/// `actor_id` here is the lookup key (the actor's primary key), not an auth context.
#[derive(Debug, Clone)]
pub struct FindUserById {
    pub actor_id: Uuid,
}

impl FindUserById {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Option<User>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_user_by_id(connection, self.actor_id).await
    }
}

#[derive(Debug, Clone)]
pub struct FindUserByUsername {
    pub username: Username,
}

impl FindUserByUsername {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Option<User>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_user_by_username(connection, self.username.clone()).await
    }
}

#[derive(Debug, Clone)]
pub struct AllActorIds;

impl AllActorIds {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Vec<Uuid>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::all_actor_ids(connection).await
    }
}

// --- Activity ---

#[derive(Debug, Clone)]
pub struct FindActivityById {
    pub id: Uuid,
}

impl FindActivityById {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Option<Activity>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_activity_by_id(connection, self.id).await
    }
}

#[derive(Debug, Clone)]
pub struct AllActivities;

impl AllActivities {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Vec<Activity>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::all_activities(connection).await
    }
}

// --- Entry ---

#[derive(Debug, Clone)]
pub struct AllEntries;

impl AllEntries {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Vec<Entry>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::all_entries(connection).await
    }
}

#[derive(Debug, Clone)]
pub struct EntriesRootedInTimeInterval {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

impl EntriesRootedInTimeInterval {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Vec<Entry>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::entries_rooted_in_time_interval(connection, self.from, self.to).await
    }
}

#[derive(Debug, Clone)]
pub struct FindAncestors {
    pub entry_id: Uuid,
}

impl FindAncestors {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Vec<Uuid>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_ancestors(connection, self.entry_id).await
    }
}

#[derive(Debug, Clone)]
pub struct FindEntryById {
    pub entry_id: Uuid,
}

impl FindEntryById {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Option<Entry>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_entry_by_id(connection, self.entry_id).await
    }
}

#[derive(Debug, Clone)]
pub struct FindEntryJoinById {
    pub entry_id: Uuid,
}

impl FindEntryJoinById {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Option<EntryJoin>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_entry_join_by_id(connection, self.entry_id).await
    }
}

#[derive(Debug, Clone)]
pub struct FindDescendants {
    pub entry_id: Uuid,
}

impl FindDescendants {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Vec<Entry>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_descendants(connection, self.entry_id).await
    }
}

// --- Attribute ---

#[derive(Debug, Clone)]
pub struct FindAttributeById {
    pub attribute_id: Uuid,
}

impl FindAttributeById {
    pub async fn execute<DB, R>(
        &self,
        connection: &mut DB::Connection,
    ) -> Result<Option<Attribute>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_attribute_by_id(connection, self.attribute_id).await
    }
}

#[derive(Debug, Clone)]
pub struct AllAttributes;

impl AllAttributes {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Vec<Attribute>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::all_attributes(connection).await
    }
}

/// `owner_id` is the query dimension: returns all attributes owned by this actor.
#[derive(Debug, Clone)]
pub struct FindAttributesByOwner {
    pub owner_id: Uuid,
}

impl FindAttributesByOwner {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Vec<Attribute>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_attributes_by_owner(connection, self.owner_id).await
    }
}

// --- Value ---

#[derive(Debug, Clone)]
pub struct FindValueByKey {
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
}

impl FindValueByKey {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Option<Value>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_value_by_key(connection, self.entry_id, self.attribute_id).await
    }
}

#[derive(Debug, Clone)]
pub struct FindValuesForEntry {
    pub entry_id: Uuid,
}

impl FindValuesForEntry {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Vec<Value>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_values_for_entry(connection, self.entry_id).await
    }
}

/// `entry_ids` is owned (not borrowed) so that this struct can implement `Clone` and be stored
/// in the `Query` enum.
#[derive(Debug, Clone)]
pub struct FindValuesForEntries {
    pub entry_ids: Vec<Uuid>,
}

impl FindValuesForEntries {
    pub async fn execute<DB, R>(&self, connection: &mut DB::Connection) -> Result<Vec<Value>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_values_for_entries(connection, &self.entry_ids).await
    }
}

#[derive(Debug, Clone)]
pub struct FindAttributePairsForEntry {
    pub entry_id: Uuid,
}

impl FindAttributePairsForEntry {
    pub async fn execute<DB, R>(
        &self,
        connection: &mut DB::Connection,
    ) -> Result<Vec<AttributePair>>
    where
        DB: sqlx::Database,
        R: Reader<DB>,
    {
        R::find_attribute_pairs_for_entry(connection, self.entry_id).await
    }
}

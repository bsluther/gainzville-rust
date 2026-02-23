use uuid::Uuid;
use sqlx::types::chrono::{DateTime, Utc};

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
    validation::{Email, Username},
};

/// Database-agnostic read interface for domain models.
///
/// Design contract:
/// - Every method accepts `&mut DB::Connection`.
/// - Every method must be callable inside transaction-scoped mutator flows by passing `&mut **tx`.
///
/// See `docs/reader.md` for the full execution model and rationale.

#[allow(async_fn_in_trait)]
pub trait Reader<DB: sqlx::Database> {
    // Authn methods

    async fn is_email_registered(connection: &mut DB::Connection, email: Email) -> Result<bool>;

    async fn find_user_by_id(
        connection: &mut DB::Connection,
        actor_id: Uuid,
    ) -> Result<Option<User>>;

    async fn find_user_by_username(
        connection: &mut DB::Connection,
        username: Username,
    ) -> Result<Option<User>>;

    async fn all_actor_ids(connection: &mut DB::Connection) -> Result<Vec<Uuid>>;

    // Activity methods

    async fn find_activity_by_id(
        connection: &mut DB::Connection,
        id: Uuid,
    ) -> Result<Option<Activity>>;

    async fn all_activities(connection: &mut DB::Connection) -> Result<Vec<Activity>>;

    // Entry methods

    async fn all_entries(connection: &mut DB::Connection) -> Result<Vec<Entry>>;

    async fn entries_rooted_in_time_interval(
        connection: &mut DB::Connection,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Entry>>;

    /// Get the ids of the provided entry's ancestors.
    /// Returns ancestor ids in ascending order including the provided entry,
    /// i.e. [entry, parent, grandparent, ..., root].
    /// If the entry is not found, returns error.
    async fn find_ancestors(connection: &mut DB::Connection, entry_id: Uuid) -> Result<Vec<Uuid>>;

    async fn find_entry_by_id(
        connection: &mut DB::Connection,
        entry_id: Uuid,
    ) -> Result<Option<Entry>>;

    async fn find_entry_join_by_id(
        connection: &mut DB::Connection,
        entry_id: Uuid,
    ) -> Result<Option<EntryJoin>>;

    /// Find entry and all descendants recursively, result includes the queried entry. An empty
    /// result vector is returned if the queried entry is not found.
    async fn find_descendants(
        connection: &mut DB::Connection,
        entry_id: Uuid,
    ) -> Result<Vec<Entry>>;

    // Attribute methods

    async fn find_attribute_by_id(
        connection: &mut DB::Connection,
        attribute_id: Uuid,
    ) -> Result<Option<Attribute>>;

    async fn find_attributes_by_owner(
        connection: &mut DB::Connection,
        owner_id: Uuid,
    ) -> Result<Vec<Attribute>>;

    // Value methods

    async fn find_value_by_key(
        connection: &mut DB::Connection,
        entry_id: Uuid,
        attribute_id: Uuid,
    ) -> Result<Option<Value>>;

    async fn find_values_for_entry(
        connection: &mut DB::Connection,
        entry_id: Uuid,
    ) -> Result<Vec<Value>>;

    async fn find_attribute_pairs_for_entry(
        connection: &mut DB::Connection,
        entry_id: Uuid,
    ) -> Result<Vec<AttributePair>>;
}

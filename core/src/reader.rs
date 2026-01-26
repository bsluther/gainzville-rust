use sqlx::Executor;
use uuid::Uuid;

use crate::{
    error::Result,
    models::{activity::Activity, entry::Entry, user::User},
    validation::{Email, Username},
};

#[allow(async_fn_in_trait)]
pub trait Reader<DB: sqlx::Database> {
    // Authn methods

    async fn is_email_registered<'e, E>(executor: E, email: Email) -> Result<bool>
    where
        E: Executor<'e, Database = DB>;

    async fn find_user_by_id<'e>(
        executor: impl Executor<'e, Database = DB>,
        actor_id: Uuid,
    ) -> Result<Option<User>>;

    async fn find_user_by_username<'e>(
        executor: impl Executor<'e, Database = DB>,
        username: Username,
    ) -> Result<Option<User>>;

    async fn all_actor_ids<'e>(executor: impl Executor<'e, Database = DB>) -> Result<Vec<Uuid>>;

    // Activity methods

    async fn find_activity_by_id<'e>(
        executor: impl Executor<'e, Database = DB>,
        id: Uuid,
    ) -> Result<Option<Activity>>;

    async fn all_activities<'e>(
        executor: impl Executor<'e, Database = DB>,
    ) -> Result<Vec<Activity>>;

    // Entry methods

    async fn all_entries<'e>(executor: impl Executor<'e, Database = DB>) -> Result<Vec<Entry>>;

    /// Get the ids of the provided entry's ancestors.
    /// Returns ancestor ids in ascending order including the provided entry,
    /// i.e. [entry, parent, grandparent, ..., root].
    /// If the entry is not found, returns error.
    async fn find_ancestors<'e>(
        executor: impl Executor<'e, Database = DB>,
        entry_id: Uuid,
    ) -> Result<Vec<Uuid>>;

    async fn find_entry_by_id<'e>(
        executor: impl Executor<'e, Database = DB>,
        entry_id: Uuid,
    ) -> Result<Option<Entry>>;
}

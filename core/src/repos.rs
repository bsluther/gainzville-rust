use sqlx::Executor;
use uuid::Uuid;

use crate::{
    error::Result,
    models::{activity::Activity, entry::Entry, user::User},
    validation::{Email, Username},
};

#[allow(async_fn_in_trait)]
pub trait AuthnRepo {
    async fn is_email_registered(&mut self, email: Email) -> Result<bool>;
    async fn find_user_by_id(&mut self, actor_id: Uuid) -> Result<Option<User>>;
    async fn find_user_by_username(&mut self, username: Username) -> Result<Option<User>>;
    async fn all_actor_ids(&mut self) -> Result<Vec<Uuid>>;
}

#[allow(async_fn_in_trait)]
pub trait ActivityRepo {
    async fn find_activity_by_id(&mut self, id: Uuid) -> Result<Option<Activity>>;
    async fn all_activities(&mut self) -> Result<Vec<Activity>>;
}

#[allow(async_fn_in_trait)]
pub trait EntryRepo {
    /// Get the ids of the provided entry's ancestors.
    /// Returns ancestor ids in ascending order including the provided entry,
    /// i.e. [entry, parent, grandparent, ..., root].
    /// If the entry is not found, returns error.
    async fn find_ancestors(&mut self, entry_id: Uuid) -> Result<Vec<Uuid>>;
    async fn find_entry_by_id(&mut self, entry_id: Uuid) -> Result<Option<Entry>>;
}

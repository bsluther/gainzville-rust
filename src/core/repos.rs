use uuid::Uuid;

use crate::core::{
    error::Result,
    models::{activity::Activity, user::User},
    validation::{Email, Username},
};

#[allow(async_fn_in_trait)]
pub trait AuthnRepo {
    async fn is_email_registered(&mut self, email: Email) -> Result<bool>;
    async fn find_user_by_id(&mut self, actor_id: Uuid) -> Result<Option<User>>;
    async fn find_user_by_username(&mut self, username: Username) -> Result<Option<User>>;
}

pub trait ActivityRepo {
    async fn find_activity_by_id(&mut self, id: Uuid) -> Result<Option<Activity>>;
}
pub trait EntryRepo {
    // async fn find_
}

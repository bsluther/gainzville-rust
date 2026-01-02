use sqlx::{Sqlite, Transaction};

use crate::core::{
    error::Result,
    models::{activity::Activity, user::User},
    repos::{ActivityRepo, AuthnRepo},
    validation::{Email, Username},
};

// The Repo lives only as long as the Transaction borrow.
// Need to borrow as mutable because we are going to mutate the transaction.
// Transaction operations must run serially, i.e. the borrow must end before the next call.
// 'c is the SqliteContext lifetime.
// 't is the transction lifetime, needs to outlive the repo.
pub struct SqliteContext<'c, 't> {
    tx: &'c mut Transaction<'t, Sqlite>,
}

impl<'c, 't> SqliteContext<'c, 't> {
    pub fn new(tx: &'c mut Transaction<'t, Sqlite>) -> Self {
        Self { tx }
    }
}

impl<'c, 't> AuthnRepo for SqliteContext<'c, 't> {
    async fn is_email_registered(&mut self, email: Email) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = ?")
            .bind(email.as_str())
            .fetch_one(&mut **self.tx) // Deref magic to get the Executor
            .await?;

        Ok(count > 0)
    }

    async fn find_user_by_id(&mut self, actor_id: uuid::Uuid) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT actor_id, username, email FROM users WHERE actor_id = ?",
        )
        .bind(actor_id)
        .fetch_optional(&mut **self.tx)
        .await?;

        Ok(user)
    }

    async fn find_user_by_username(&mut self, username: Username) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
            .bind(username.as_str())
            .fetch_optional(&mut **self.tx)
            .await?;

        Ok(user)
    }
    async fn all_actor_ids(&mut self) -> Result<Vec<uuid::Uuid>> {
        let actor_ids = sqlx::query_scalar(
            r#"
            SELECT id FROM actors
            "#,
        )
        .fetch_all(&mut **self.tx)
        .await?;
        Ok(actor_ids)
    }
}

impl<'c, 't> ActivityRepo for SqliteContext<'c, 't> {
    async fn find_activity_by_id(&mut self, id: uuid::Uuid) -> Result<Option<Activity>> {
        let activity = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&mut **self.tx)
        .await?;

        Ok(activity)
    }
}

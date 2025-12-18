use sqlx::{Postgres, Transaction};

use crate::core::{
    delta::{Delta, ModelDelta},
    error::Result,
    models::{actor::Actor, user::User},
};

#[allow(async_fn_in_trait)]
pub trait PgApply: Sized {
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()>;
}

impl PgApply for ModelDelta {
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        match self {
            ModelDelta::Actor(delta) => delta.apply_delta(tx).await,
            ModelDelta::User(delta) => delta.apply_delta(tx).await,
        }
    }
}

impl PgApply for Delta<Actor> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        match self {
            Delta::Insert { id, new } => {
                sqlx::query!(
                    r#"
                    INSERT INTO actors (id, actor_kind, created_at)
                    VALUES ($1, $2, $3)
                    "#,
                    id,
                    new.actor_kind.to_string(),
                    new.created_at,
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Update { .. } => {} // No-op
            Delta::Delete { id, .. } => {
                sqlx::query!(
                    r#"
                    DELETE FROM actors WHERE id = $1
                    "#,
                    id
                )
                .execute(&mut **tx)
                .await?;
            }
        };
        Ok(())
    }
}

impl PgApply for Delta<User> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        match self {
            Delta::Insert { id, new } => {
                sqlx::query!(
                    r#"
                    INSERT INTO users (actor_id, username, email)
                    VALUES ($1, $2, $3)
                    "#,
                    id,
                    new.username.as_str(),
                    new.email.as_str(),
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Update { id, new, .. } => {
                sqlx::query!(
                    r#"
                    UPDATE users
                    SET
                        username = COALESCE($1, username),
                        email = COALESCE($2, email)

                    WHERE actor_id = $3
                    "#,
                    new.username.map(|u| u.as_str().to_string()),
                    new.email.map(|u| u.as_str().to_string()),
                    id
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Delete { id, .. } => {
                sqlx::query!(
                    r#"
                    DELETE FROM users WHERE actor_id = $1
                    "#,
                    id
                )
                .execute(&mut **tx)
                .await?;
            }
        };
        Ok(())
    }
}

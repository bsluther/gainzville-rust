use sqlx::{Sqlite, Transaction};

use crate::core::{
    delta::{Delta, ModelDelta},
    error::Result,
    models::{actor::Actor, user::User},
};

#[allow(async_fn_in_trait)]
pub trait SqliteApply: Sized {
    async fn apply_delta(self, tx: &mut Transaction<'_, Sqlite>) -> Result<()>;
}

impl SqliteApply for ModelDelta {
    async fn apply_delta(self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        match self {
            ModelDelta::Actor(delta) => delta.apply_delta(tx).await,
            ModelDelta::User(delta) => delta.apply_delta(tx).await,
        }
    }
}

impl SqliteApply for Delta<Actor> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        match self {
            Delta::Insert { id, new } => {
                sqlx::query("INSERT INTO actors (id, actor_kind, created_at) VALUES (?, ?, ?)")
                    .bind(id.to_string())
                    .bind(new.actor_kind.to_string())
                    .bind(new.created_at.to_rfc3339())
                    .execute(&mut **tx)
                    .await?;
            }
            Delta::Update { .. } => {} // No-op
            Delta::Delete { id, .. } => {
                sqlx::query("DELETE FROM actors WHERE id = ?")
                    .bind(id.to_string())
                    .execute(&mut **tx)
                    .await?;
            }
        };
        Ok(())
    }
}

impl SqliteApply for Delta<User> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        match self {
            Delta::Insert { id, new } => {
                sqlx::query("INSERT INTO users (actor_id, username, email) VALUES (?, ?, ?)")
                    .bind(id.to_string())
                    .bind(new.username.as_str())
                    .bind(new.email.as_str())
                    .execute(&mut **tx)
                    .await?;
            }
            Delta::Update { id, new, .. } => {
                sqlx::query(
                    "UPDATE users SET username = COALESCE(?, username), email = COALESCE(?, email) WHERE actor_id = ?"
                )
                .bind(new.username.map(|u| u.as_str().to_string()))
                .bind(new.email.map(|u| u.as_str().to_string()))
                .bind(id.to_string())
                .execute(&mut **tx)
                .await?;
            }
            Delta::Delete { id, .. } => {
                sqlx::query("DELETE FROM users WHERE actor_id = ?")
                    .bind(id.to_string())
                    .execute(&mut **tx)
                    .await?;
            }
        };
        Ok(())
    }
}

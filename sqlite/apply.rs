use sqlx::{Sqlite, Transaction};

use gv_core::{
    delta::{Delta, ModelDelta},
    error::Result,
    models::{activity::Activity, actor::Actor, entry::Entry, user::User},
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
            ModelDelta::Activity(delta) => delta.apply_delta(tx).await,
            ModelDelta::Entry(delta) => delta.apply_delta(tx).await,
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
                // TODO: this updates all fields, even those that haven't changed.
                sqlx::query(
                    "UPDATE users SET username = COALESCE(?, username), email = COALESCE(?, email) WHERE actor_id = ?"
                )
                .bind(new.username.as_str().to_string())
                .bind(new.email.as_str().to_string())
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

impl SqliteApply for Delta<Activity> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        match self {
            Delta::Insert { id, new } => {
                sqlx::query("INSERT INTO activities (id, owner_id, source_activity_id, name, description) VALUES (?, ?, ?, ?, ?)")
                    .bind(id.to_string())
                    .bind(new.owner_id.to_string())
                    .bind(new.source_activity_id)
                    .bind(new.name.to_string())
                    .bind(new.description)
                    .execute(&mut **tx)
                    .await?;
            }
            Delta::Update { id, new, .. } => {
                sqlx::query("UPDATE activities SET owner_id = ?, source_activity_id = ?, name = ?, description = ? WHERE id = ?")
                    .bind(new.owner_id)
                    .bind(new.source_activity_id)
                    .bind(new.name.to_string())
                    .bind(new.description)
                    .bind(id)
                    .execute(&mut **tx)
                    .await?;
            }
            Delta::Delete { id, .. } => {
                sqlx::query("DELETE FROM activities WHERE id = ?")
                    .bind(id)
                    .execute(&mut **tx)
                    .await?;
            }
        };
        Ok(())
    }
}

impl SqliteApply for Delta<Entry> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        match self {
            Delta::Insert { id, new } => {
                sqlx::query(
                    r#"
                    INSERT INTO entries (
                        id, activity_id, 
                        owner_id, 
                        parent_id, 
                        frac_index,
                        is_template, 
                        display_as_sets, 
                        is_sequence,
                        start_time, 
                        end_time, 
                        duration_ms
                    )
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(id.to_string())
                .bind(new.activity_id.map(|u| u.to_string()))
                .bind(new.owner_id.to_string())
                .bind(new.parent_id().map(|u| u.to_string()))
                .bind(new.frac_index().map(|f| f.to_string()))
                .bind(new.is_template)
                .bind(new.display_as_sets)
                .bind(new.is_sequence)
                .bind(new.temporal.start().map(|dt| dt.to_rfc3339()))
                .bind(new.temporal.end().map(|dt| dt.to_rfc3339()))
                .bind(new.temporal.duration().map(|d| d as i64))
                .execute(&mut **tx)
                .await?;
            }
            Delta::Update { id, new, .. } => {
                sqlx::query(
                    r#"
                    UPDATE entries SET
                        activity_id = ?, 
                        parent_id = ?,
                        frac_index = ?,
                        display_as_sets = ?, 
                        is_sequence = ?,
                        start_time = ?, 
                        end_time = ?,
                        duration_ms = ?
                    WHERE id = ?
                    "#,
                )
                .bind(new.activity_id.map(|u| u.to_string()))
                .bind(new.parent_id().map(|u| u.to_string()))
                .bind(new.frac_index().map(|f| f.to_string()))
                .bind(new.display_as_sets)
                .bind(new.is_sequence)
                .bind(new.temporal.start().map(|dt| dt.to_rfc3339()))
                .bind(new.temporal.end().map(|dt| dt.to_rfc3339()))
                .bind(new.temporal.duration().map(|d| d as i64))
                .bind(id.to_string())
                .execute(&mut **tx)
                .await?;
            }
            Delta::Delete { id, .. } => {
                sqlx::query("DELETE FROM entries WHERE id = ?")
                    .bind(id.to_string())
                    .execute(&mut **tx)
                    .await?;
            }
        };
        Ok(())
    }
}

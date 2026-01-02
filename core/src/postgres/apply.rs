use sqlx::{Postgres, Transaction};
use tracing::{info, instrument, warn};

use crate::core::{
    delta::{Delta, ModelDelta},
    error::Result,
    models::{activity::Activity, actor::Actor, entry::Entry, user::User},
};

#[allow(async_fn_in_trait)]
pub trait PgApply: Sized {
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()>;
}

impl PgApply for ModelDelta {
    #[instrument(skip_all)]
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        info!(?self, "Applying delta");
        match self {
            ModelDelta::Actor(delta) => delta.apply_delta(tx).await,
            ModelDelta::User(delta) => delta.apply_delta(tx).await,
            ModelDelta::Activity(delta) => delta.apply_delta(tx).await,
            ModelDelta::Entry(delta) => delta.apply_delta(tx).await,
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
            Delta::Update { .. } => {
                // No-op, shouldn't happen.
                warn!("Applying update delta to Actor table which does not support updates");
            }
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
                // TODO: this updates all fields, even those that haven't changed.
                sqlx::query!(
                    r#"
                    UPDATE users
                    SET
                        username = $1,
                        email = $2

                    WHERE actor_id = $3
                    "#,
                    new.username.as_str().to_string(),
                    new.email.as_str().to_string(),
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

impl PgApply for Delta<Activity> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        match self {
            Delta::Insert { id, new } => {
                sqlx::query!(
                    r#"
                    INSERT INTO activities (id, owner_id, source_activity_id, name, description)
                    VALUES ($1, $2, $3, $4, $5)
                    "#,
                    id,
                    new.owner_id,
                    new.source_activity_id,
                    new.name.to_string(),
                    new.description
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Update { id, new, .. } => {
                sqlx::query!(
                    r#"
                    UPDATE activities
                    SET
                        owner_id = $1,
                        source_activity_id = $2,
                        name = $3,
                        description = $4
                    WHERE id = $5
                    "#,
                    new.owner_id,
                    new.source_activity_id,
                    new.name.to_string(),
                    new.description,
                    id
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Delete { id, .. } => {
                sqlx::query!(
                    r#"
                    DELETE FROM activities WHERE id = $1
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

impl PgApply for Delta<Entry> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        match self {
            Delta::Insert { id, new } => {
                sqlx::query!(
                    r#"
                    INSERT INTO entries (
                        id,
                        activity_id,
                        owner_id,
                        parent_id, 
                        frac_index,
                        is_template,
                        display_as_sets,
                        is_sequence
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    "#,
                    id,
                    new.activity_id,
                    new.owner_id,
                    new.parent_id,
                    new.frac_index.map(|f| f.to_string()),
                    new.is_template,
                    new.display_as_sets,
                    new.is_sequence
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Update { id, new, .. } => {
                sqlx::query!(
                    r#"
                    UPDATE entries
                    SET
                        activity_id = $1,
                        parent_id = $2,
                        frac_index = $3,
                        display_as_sets = $4,
                        is_sequence = $5
                    WHERE id = $6
                    "#,
                    new.activity_id,
                    new.parent_id,
                    new.frac_index.map(|f| f.to_string()),
                    new.display_as_sets,
                    new.is_sequence,
                    id
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Delete { id, .. } => {
                sqlx::query!(
                    r#"
                    DELETE FROM entries WHERE id = $1
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

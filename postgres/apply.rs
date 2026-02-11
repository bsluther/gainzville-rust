use sqlx::{Postgres, Transaction};
use tracing::{info, instrument, warn};

use gv_core::{
    delta::{Delta, ModelDelta},
    error::Result,
    models::{
        activity::Activity,
        actor::Actor,
        attribute::{Attribute, AttributeRow, Value, ValueRow},
        entry::Entry,
        user::User,
    },
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
            ModelDelta::Attribute(delta) => delta.apply_delta(tx).await,
            ModelDelta::Value(delta) => delta.apply_delta(tx).await,
        }
    }
}

impl PgApply for Delta<Actor> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        match self {
            Delta::Insert { new } => {
                sqlx::query!(
                    r#"
                    INSERT INTO actors (id, actor_kind, created_at)
                    VALUES ($1, $2, $3)
                    "#,
                    new.actor_id,
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
            Delta::Delete { old } => {
                sqlx::query!(
                    r#"
                    DELETE FROM actors WHERE id = $1
                    "#,
                    old.actor_id
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
            Delta::Insert { new } => {
                sqlx::query!(
                    r#"
                    INSERT INTO users (actor_id, username, email)
                    VALUES ($1, $2, $3)
                    "#,
                    new.actor_id,
                    new.username.as_str(),
                    new.email.as_str(),
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Update { old, new } => {
                assert_eq!(old.actor_id, new.actor_id, "update must not mutate primary key");
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
                    new.actor_id
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Delete { old } => {
                sqlx::query!(
                    r#"
                    DELETE FROM users WHERE actor_id = $1
                    "#,
                    old.actor_id
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
            Delta::Insert { new } => {
                sqlx::query!(
                    r#"
                    INSERT INTO activities (id, owner_id, source_activity_id, name, description)
                    VALUES ($1, $2, $3, $4, $5)
                    "#,
                    new.id,
                    new.owner_id,
                    new.source_activity_id,
                    new.name.to_string(),
                    new.description
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Update { old, new } => {
                assert_eq!(old.id, new.id, "update must not mutate primary key");
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
                    new.id
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Delete { old } => {
                sqlx::query!(
                    r#"
                    DELETE FROM activities WHERE id = $1
                    "#,
                    old.id
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
            Delta::Insert { new } => {
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
                        is_sequence,
                        start_time,
                        end_time,
                        duration_ms
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                    "#,
                    new.id,
                    new.activity_id,
                    new.owner_id,
                    new.parent_id(),
                    new.frac_index().map(|f| f.to_string()),
                    new.is_template,
                    new.display_as_sets,
                    new.is_sequence,
                    new.temporal.start(),
                    new.temporal.end(),
                    new.temporal.duration().map(|d| d as i64)
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Update { old, new } => {
                assert_eq!(old.id, new.id, "update must not mutate primary key");
                sqlx::query!(
                    r#"
                    UPDATE entries
                    SET
                        activity_id = $1,
                        parent_id = $2,
                        frac_index = $3,
                        display_as_sets = $4,
                        is_sequence = $5,
                        start_time = $6,
                        end_time = $7,
                        duration_ms = $8
                    WHERE id = $9
                    "#,
                    new.activity_id,
                    new.parent_id(),
                    new.frac_index().map(|f| f.to_string()),
                    new.display_as_sets,
                    new.is_sequence,
                    new.temporal.start(),
                    new.temporal.end(),
                    new.temporal.duration().map(|d| d as i64),
                    new.id
                )
                .execute(&mut **tx)
                .await?;
            }
            Delta::Delete { old } => {
                sqlx::query!(
                    r#"
                    DELETE FROM entries WHERE id = $1
                    "#,
                    old.id
                )
                .execute(&mut **tx)
                .await?;
            }
        };
        Ok(())
    }
}

impl PgApply for Delta<Attribute> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        match self {
            Delta::Insert { new } => {
                let row = AttributeRow::from_attribute(&new)?;
                sqlx::query(
                    r#"
                    INSERT INTO attributes (id, owner_id, name, data_type, config)
                    VALUES ($1, $2, $3, $4, $5)
                    "#,
                )
                .bind(row.id)
                .bind(row.owner_id)
                .bind(row.name)
                .bind(row.data_type)
                .bind(row.config)
                .execute(&mut **tx)
                .await?;
            }
            Delta::Update { old, new } => {
                assert_eq!(old.id, new.id, "update must not mutate primary key");
                let row = AttributeRow::from_attribute(&new)?;
                sqlx::query(
                    r#"
                    UPDATE attributes
                    SET owner_id = $1, name = $2, data_type = $3, config = $4
                    WHERE id = $5
                    "#,
                )
                .bind(row.owner_id)
                .bind(row.name)
                .bind(row.data_type)
                .bind(row.config)
                .bind(row.id)
                .execute(&mut **tx)
                .await?;
            }
            Delta::Delete { old } => {
                sqlx::query("DELETE FROM attributes WHERE id = $1")
                    .bind(old.id)
                    .execute(&mut **tx)
                    .await?;
            }
        };
        Ok(())
    }
}

impl PgApply for Delta<Value> {
    async fn apply_delta(self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        match self {
            Delta::Insert { new } => {
                let row = ValueRow::from_value(&new)?;
                sqlx::query(
                    r#"
                    INSERT INTO attribute_values (entry_id, attribute_id, plan, actual, index_float, index_string)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    "#,
                )
                .bind(row.entry_id)
                .bind(row.attribute_id)
                .bind(row.plan)
                .bind(row.actual)
                .bind(row.index_float)
                .bind(row.index_string)
                .execute(&mut **tx)
                .await?;
            }
            Delta::Update { old, new } => {
                assert_eq!(
                    (old.entry_id, old.attribute_id),
                    (new.entry_id, new.attribute_id),
                    "update must not mutate primary key"
                );
                let row = ValueRow::from_value(&new)?;
                sqlx::query(
                    r#"
                    UPDATE attribute_values
                    SET plan = $1, actual = $2, index_float = $3, index_string = $4
                    WHERE entry_id = $5 AND attribute_id = $6
                    "#,
                )
                .bind(row.plan)
                .bind(row.actual)
                .bind(row.index_float)
                .bind(row.index_string)
                .bind(row.entry_id)
                .bind(row.attribute_id)
                .execute(&mut **tx)
                .await?;
            }
            Delta::Delete { old } => {
                sqlx::query(
                    "DELETE FROM attribute_values WHERE entry_id = $1 AND attribute_id = $2",
                )
                .bind(old.entry_id)
                .bind(old.attribute_id)
                .execute(&mut **tx)
                .await?;
            }
        };
        Ok(())
    }
}

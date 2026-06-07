use sqlx::PgConnection;
use tracing::{info, instrument, warn};

use crate::error::SqlErr;

use gv_core::{
    delta::{AnyDelta, Delta},
    delta_executor::{AnyDeltaExecutor, DeltaExecutor},
    error::Result,
    models::{
        activity::Activity,
        actor::Actor,
        attribute::{Attribute, Value},
        entry::Entry,
        user::User,
    },
};

pub struct PostgresDeltaExecutor<'c> {
    conn: &'c mut PgConnection,
}

impl<'c> PostgresDeltaExecutor<'c> {
    pub fn new(conn: &'c mut PgConnection) -> Self {
        PostgresDeltaExecutor { conn }
    }
}

impl AnyDeltaExecutor for PostgresDeltaExecutor<'_> {
    #[instrument(skip_all)]
    async fn apply_any_delta(&mut self, delta: AnyDelta) -> Result<()> {
        info!(?delta, "Applying delta");
        match delta {
            AnyDelta::Actor(delta) => self.apply_delta(delta).await,
            AnyDelta::User(delta) => self.apply_delta(delta).await,
            AnyDelta::Activity(delta) => self.apply_delta(delta).await,
            AnyDelta::Entry(delta) => self.apply_delta(delta).await,
            AnyDelta::Attribute(delta) => self.apply_delta(delta).await,
            AnyDelta::Value(delta) => self.apply_delta(delta).await,
        }
    }
}

impl DeltaExecutor<Actor> for PostgresDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<Actor>) -> Result<()> {
        match delta {
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
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
            Delta::Update { .. } => {
                warn!("Applying update delta to Actor table which does not support updates");
            }
            Delta::Delete { old } => {
                sqlx::query!(
                    r#"
                    DELETE FROM actors WHERE id = $1
                    "#,
                    old.actor_id
                )
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
        };
        Ok(())
    }
}

impl DeltaExecutor<User> for PostgresDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<User>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                let row = crate::rows::UserRow::from(new);
                sqlx::query!(
                    r#"
                    INSERT INTO users (actor_id, username, email)
                    VALUES ($1, $2, $3)
                    "#,
                    row.actor_id as _,
                    row.username as _,
                    row.email as _,
                )
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
            Delta::Update { old, new } => {
                assert_eq!(
                    old.actor_id, new.actor_id,
                    "update must not mutate primary key"
                );
                let row = crate::rows::UserRow::from(new);
                sqlx::query!(
                    r#"
                    UPDATE users
                    SET
                        username = $1,
                        email = $2
                    WHERE actor_id = $3
                    "#,
                    row.username as _,
                    row.email as _,
                    row.actor_id as _,
                )
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
            Delta::Delete { old } => {
                let row = crate::rows::UserRow::from(old);
                sqlx::query!(
                    r#"
                    DELETE FROM users WHERE actor_id = $1
                    "#,
                    row.actor_id as _,
                )
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
        };
        Ok(())
    }
}

impl DeltaExecutor<Activity> for PostgresDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<Activity>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                let row = crate::rows::ActivityRow::from(new);
                sqlx::query!(
                    r#"
                    INSERT INTO activities (id, owner_id, source_activity_id, name, description)
                    VALUES ($1, $2, $3, $4, $5)
                    "#,
                    row.id as _,
                    row.owner_id as _,
                    row.source_activity_id as _,
                    row.name as _,
                    row.description,
                )
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
            Delta::Update { old, new } => {
                assert_eq!(old.id, new.id, "update must not mutate primary key");
                let row = crate::rows::ActivityRow::from(new);
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
                    row.owner_id as _,
                    row.source_activity_id as _,
                    row.name as _,
                    row.description,
                    row.id as _,
                )
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
            Delta::Delete { old } => {
                let row = crate::rows::ActivityRow::from(old);
                sqlx::query!(
                    r#"
                    DELETE FROM activities WHERE id = $1
                    "#,
                    row.id as _,
                )
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
        };
        Ok(())
    }
}

impl DeltaExecutor<Entry> for PostgresDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<Entry>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                let row = crate::rows::EntryRow::from_entry(&new);
                sqlx::query!(
                    r#"
                    INSERT INTO entries (
                        id,
                        activity_id,
                        owner_id,
                        name,
                        parent_id,
                        frac_index,
                        is_template,
                        display_as_sets,
                        is_sequence,
                        is_complete,
                        start_time,
                        end_time,
                        duration_ms
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                    "#,
                    row.id as _,
                    row.activity_id as _,
                    row.owner_id as _,
                    row.name,
                    row.parent_id as _,
                    row.frac_index as _,
                    row.is_template,
                    row.display_as_sets,
                    row.is_sequence,
                    row.is_complete,
                    row.start_time as _,
                    row.end_time as _,
                    row.duration_ms,
                )
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
            Delta::Update { old, new } => {
                assert_eq!(old.id, new.id, "update must not mutate primary key");
                let row = crate::rows::EntryRow::from_entry(&new);
                sqlx::query!(
                    r#"
                    UPDATE entries
                    SET
                        activity_id = $1,
                        name = $2,
                        parent_id = $3,
                        frac_index = $4,
                        display_as_sets = $5,
                        is_sequence = $6,
                        is_complete = $7,
                        start_time = $8,
                        end_time = $9,
                        duration_ms = $10
                    WHERE id = $11
                    "#,
                    row.activity_id as _,
                    row.name,
                    row.parent_id as _,
                    row.frac_index as _,
                    row.display_as_sets,
                    row.is_sequence,
                    row.is_complete,
                    row.start_time as _,
                    row.end_time as _,
                    row.duration_ms,
                    row.id as _,
                )
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
            Delta::Delete { old } => {
                sqlx::query!(
                    r#"
                    DELETE FROM entries WHERE id = $1
                    "#,
                    crate::columns::UuidColumn(old.id) as _,
                )
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
        };
        Ok(())
    }
}

impl DeltaExecutor<Attribute> for PostgresDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<Attribute>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                let row = crate::rows::AttributeRow::from_attribute(&new)?;
                sqlx::query(
                    r#"
                    INSERT INTO attributes (id, owner_id, name, description, data_type, config)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    "#,
                )
                .bind(row.id)
                .bind(row.owner_id)
                .bind(row.name)
                .bind(row.description)
                .bind(row.data_type)
                .bind(row.config)
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
            Delta::Update { old, new } => {
                assert_eq!(old.id, new.id, "update must not mutate primary key");
                let row = crate::rows::AttributeRow::from_attribute(&new)?;
                sqlx::query(
                    r#"
                    UPDATE attributes
                    SET owner_id = $1, name = $2, description = $3, data_type = $4, config = $5
                    WHERE id = $6
                    "#,
                )
                .bind(row.owner_id)
                .bind(row.name)
                .bind(row.description)
                .bind(row.data_type)
                .bind(row.config)
                .bind(row.id)
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
            Delta::Delete { old } => {
                sqlx::query("DELETE FROM attributes WHERE id = $1")
                    .bind(crate::columns::UuidColumn(old.id))
                    .execute(&mut *self.conn)
                    .await
                    .sql_err()?;
            }
        };
        Ok(())
    }
}

impl DeltaExecutor<Value> for PostgresDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<Value>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                let row = crate::rows::ValueRow::from_value(&new)?;
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
                .execute(&mut *self.conn)
                .await.sql_err()?;
            }
            Delta::Update { old, new } => {
                assert_eq!(
                    (old.entry_id, old.attribute_id),
                    (new.entry_id, new.attribute_id),
                    "update must not mutate primary key"
                );
                let row = crate::rows::ValueRow::from_value(&new)?;
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
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
            Delta::Delete { old } => {
                sqlx::query(
                    "DELETE FROM attribute_values WHERE entry_id = $1 AND attribute_id = $2",
                )
                .bind(crate::columns::UuidColumn(old.entry_id))
                .bind(crate::columns::UuidColumn(old.attribute_id))
                .execute(&mut *self.conn)
                .await
                .sql_err()?;
            }
        };
        Ok(())
    }
}

use gv_core::{
    delta::{AnyDelta, Delta},
    delta_executor::{AnyDeltaExecutor, DeltaExecutor},
    error::{DbErr, Result},
    models::{
        activity::Activity,
        actor::Actor,
        attribute::{Attribute, Value},
        entry::Entry,
        user::User,
    },
};
use sqlx::SqliteConnection;

pub struct SqliteDeltaExecutor<'c> {
    conn: &'c mut SqliteConnection,
}
impl<'c> SqliteDeltaExecutor<'c> {
    pub fn new(conn: &'c mut SqliteConnection) -> Self {
        SqliteDeltaExecutor { conn }
    }
}

impl AnyDeltaExecutor for SqliteDeltaExecutor<'_> {
    async fn apply_any_delta(&mut self, delta: AnyDelta) -> Result<()> {
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
impl DeltaExecutor<Actor> for SqliteDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<Actor>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                sqlx::query("INSERT INTO actors (id, actor_kind, created_at) VALUES (?, ?, ?)")
                    .bind(new.actor_id)
                    .bind(new.actor_kind.to_string())
                    .bind(new.created_at.to_rfc3339())
                    .execute(&mut *self.conn)
                    .await
                    .db_err()?;
            }
            Delta::Update { .. } => {} // No-op
            Delta::Delete { old } => {
                sqlx::query("DELETE FROM actors WHERE id = ?")
                    .bind(old.actor_id)
                    .execute(&mut *self.conn)
                    .await
                    .db_err()?;
            }
        };
        Ok(())
    }
}

impl DeltaExecutor<User> for SqliteDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<User>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                let row = crate::rows::UserRow::from(new);
                sqlx::query("INSERT INTO users (actor_id, username, email) VALUES (?, ?, ?)")
                    .bind(row.actor_id)
                    .bind(row.username)
                    .bind(row.email)
                    .execute(&mut *self.conn)
                    .await
                    .db_err()?;
            }
            Delta::Update { old, new } => {
                assert_eq!(
                    old.actor_id, new.actor_id,
                    "update must not mutate primary key"
                );
                let row = crate::rows::UserRow::from(new);
                sqlx::query(
                    "UPDATE users SET username = COALESCE(?, username), email = COALESCE(?, email) WHERE actor_id = ?"
                )
                .bind(row.username)
                .bind(row.email)
                .bind(row.actor_id)
                .execute(&mut *self.conn)
                .await.db_err()?;
            }
            Delta::Delete { old } => {
                let row = crate::rows::UserRow::from(old);
                sqlx::query("DELETE FROM users WHERE actor_id = ?")
                    .bind(row.actor_id)
                    .execute(&mut *self.conn)
                    .await
                    .db_err()?;
            }
        };
        Ok(())
    }
}

impl DeltaExecutor<Activity> for SqliteDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<Activity>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                let row = crate::rows::ActivityRow::from(new);
                sqlx::query("INSERT INTO activities (id, owner_id, source_activity_id, name, description) VALUES (?, ?, ?, ?, ?)")
                    .bind(row.id)
                    .bind(row.owner_id)
                    .bind(row.source_activity_id)
                    .bind(row.name)
                    .bind(row.description)
                    .execute(&mut *self.conn)
                    .await.db_err()?;
            }
            Delta::Update { old, new } => {
                assert_eq!(old.id, new.id, "update must not mutate primary key");
                let row = crate::rows::ActivityRow::from(new);
                sqlx::query("UPDATE activities SET owner_id = ?, source_activity_id = ?, name = ?, description = ? WHERE id = ?")
                    .bind(row.owner_id)
                    .bind(row.source_activity_id)
                    .bind(row.name)
                    .bind(row.description)
                    .bind(row.id)
                    .execute(&mut *self.conn)
                    .await.db_err()?;
            }
            Delta::Delete { old } => {
                let row = crate::rows::ActivityRow::from(old);
                sqlx::query("DELETE FROM activities WHERE id = ?")
                    .bind(row.id)
                    .execute(&mut *self.conn)
                    .await
                    .db_err()?;
            }
        };
        Ok(())
    }
}

impl DeltaExecutor<Entry> for SqliteDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<Entry>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                let row = crate::rows::EntryRow::from_entry(&new);
                sqlx::query(
                    r#"
                    INSERT INTO entries (
                        id, activity_id,
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
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(row.id)
                .bind(row.activity_id)
                .bind(row.owner_id)
                .bind(row.name)
                .bind(row.parent_id)
                .bind(row.frac_index)
                .bind(row.is_template)
                .bind(row.display_as_sets)
                .bind(row.is_sequence)
                .bind(row.is_complete)
                .bind(row.start_time)
                .bind(row.end_time)
                .bind(row.duration_ms)
                .execute(&mut *self.conn)
                .await
                .db_err()?;
            }
            Delta::Update { old, new } => {
                assert_eq!(old.id, new.id, "update must not mutate primary key");
                let row = crate::rows::EntryRow::from_entry(&new);
                sqlx::query(
                    r#"
                    UPDATE entries SET
                        activity_id = ?,
                        name = ?,
                        parent_id = ?,
                        frac_index = ?,
                        display_as_sets = ?,
                        is_sequence = ?,
                        is_complete = ?,
                        start_time = ?,
                        end_time = ?,
                        duration_ms = ?
                    WHERE id = ?
                    "#,
                )
                .bind(row.activity_id)
                .bind(row.name)
                .bind(row.parent_id)
                .bind(row.frac_index)
                .bind(row.display_as_sets)
                .bind(row.is_sequence)
                .bind(row.is_complete)
                .bind(row.start_time)
                .bind(row.end_time)
                .bind(row.duration_ms)
                .bind(row.id)
                .execute(&mut *self.conn)
                .await
                .db_err()?;
            }
            Delta::Delete { old } => {
                sqlx::query("DELETE FROM entries WHERE id = ?")
                    .bind(crate::columns::UuidColumn(old.id))
                    .execute(&mut *self.conn)
                    .await
                    .db_err()?;
            }
        };
        Ok(())
    }
}

impl DeltaExecutor<Attribute> for SqliteDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<Attribute>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                let row = crate::rows::AttributeRow::from_attribute(&new)?;
                sqlx::query(
                    "INSERT INTO attributes (id, owner_id, name, description, data_type, config) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(row.id)
                .bind(row.owner_id)
                .bind(row.name)
                .bind(row.description)
                .bind(row.data_type)
                .bind(row.config)
                .execute(&mut *self.conn)
                .await.db_err()?;
            }
            Delta::Update { old, new } => {
                assert_eq!(old.id, new.id, "update must not mutate primary key");
                let row = crate::rows::AttributeRow::from_attribute(&new)?;
                sqlx::query(
                    "UPDATE attributes SET owner_id = ?, name = ?, description = ?, data_type = ?, config = ? WHERE id = ?",
                )
                .bind(row.owner_id)
                .bind(row.name)
                .bind(row.description)
                .bind(row.data_type)
                .bind(row.config)
                .bind(row.id)
                .execute(&mut *self.conn)
                .await.db_err()?;
            }
            Delta::Delete { old } => {
                sqlx::query("DELETE FROM attributes WHERE id = ?")
                    .bind(crate::columns::UuidColumn(old.id))
                    .execute(&mut *self.conn)
                    .await
                    .db_err()?;
            }
        };
        Ok(())
    }
}

impl DeltaExecutor<Value> for SqliteDeltaExecutor<'_> {
    async fn apply_delta(&mut self, delta: Delta<Value>) -> Result<()> {
        match delta {
            Delta::Insert { new } => {
                let row = crate::rows::ValueRow::from_value(&new)?;
                sqlx::query(
                    r#"
                    INSERT INTO attribute_values (entry_id, attribute_id, plan, actual, index_float, index_string)
                    VALUES (?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(row.entry_id)
                .bind(row.attribute_id)
                .bind(row.plan)
                .bind(row.actual)
                .bind(row.index_float)
                .bind(row.index_string)
                .execute(&mut *self.conn)
                .await.db_err()?;
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
                    SET plan = ?, actual = ?, index_float = ?, index_string = ?
                    WHERE entry_id = ? AND attribute_id = ?
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
                .db_err()?;
            }
            Delta::Delete { old } => {
                sqlx::query("DELETE FROM attribute_values WHERE entry_id = ? AND attribute_id = ?")
                    .bind(crate::columns::UuidColumn(old.entry_id))
                    .bind(crate::columns::UuidColumn(old.attribute_id))
                    .execute(&mut *self.conn)
                    .await
                    .db_err()?;
            }
        };
        Ok(())
    }
}

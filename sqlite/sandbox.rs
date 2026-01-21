use gv_core::{
    actions::Action,
    error::{DomainError, Result},
    models::{
        activity::{Activity, ActivityName},
        entry::{Entry, EntryRow},
        user::User,
    },
    sandbox::{Reader, compute_mutation},
    validation::{Email, Username},
};
use itertools::Itertools;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::apply::SqliteApply;

///////////// CLIENT /////////////

pub struct SqliteClient {
    pub pool: SqlitePool,
}

impl SqliteClient {
    pub async fn run_action(&self, action: Action) -> Result<()> {
        // Begin Sqlite transaction.
        let mut tx = self.pool.begin().await?;

        // Create mutation.
        let mx = match action {
            Action::CreateActivity(action) => {
                compute_mutation::create_activity::<sqlx::Sqlite, SqliteReader>(&mut tx, action)
                    .await?
            }
            Action::CreateUser(action) => {
                compute_mutation::create_user::<sqlx::Sqlite, SqliteReader>(&mut tx, action).await?
            }
            Action::CreateEntry(action) => {
                compute_mutation::create_entry::<sqlx::Sqlite, SqliteReader>(&mut tx, action)
                    .await?
            }
            Action::MoveEntry(action) => {
                compute_mutation::move_entry::<sqlx::Sqlite, SqliteReader>(&mut tx, action).await?
            }
        };

        // TODO: log mutation in this transaction.
        // sync_service.log_mutation(mx);

        // Apply deltas.
        for delta in mx.changes {
            delta.apply_delta(&mut tx).await?;
        }

        // Commit the transaction.
        tx.commit().await?;

        // TODO: send mutation to service (or add to a pending_mutations queue).
        // sync_service.append_applied_mutation(mx);

        Ok(())
    }
}

///////////// READER /////////////

/// SQLite-specific row type for Activity.
/// SQLite stores UUIDs as TEXT, so we need to parse them from strings.
#[derive(FromRow)]
struct ActivitySqliteRow {
    id: String,
    owner_id: String,
    source_activity_id: Option<String>,
    name: ActivityName,
    description: Option<String>,
}

impl ActivitySqliteRow {
    fn to_activity(self) -> Result<Activity> {
        Ok(Activity {
            id: Uuid::parse_str(&self.id)
                .map_err(|e| DomainError::Other(format!("invalid activity id: {e}")))?,
            owner_id: Uuid::parse_str(&self.owner_id)
                .map_err(|e| DomainError::Other(format!("invalid owner_id: {e}")))?,
            source_activity_id: self
                .source_activity_id
                .map(|s| Uuid::parse_str(&s))
                .transpose()
                .map_err(|e| DomainError::Other(format!("invalid source_activity_id: {e}")))?,
            name: self.name,
            description: self.description,
        })
    }
}

/// Helper struct for ancestor query results.
#[derive(FromRow)]
struct AncestorRow {
    id: String,
    parent_id: Option<String>,
}

struct SqliteReader;
impl Reader<sqlx::Sqlite> for SqliteReader {
    ///////////// Authn /////////////
    async fn is_email_registered<'e, E>(
        executor: E,
        email: gv_core::validation::Email,
    ) -> Result<bool>
    where
        E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
    {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = ?")
            .bind(email.as_str())
            .fetch_one(executor)
            .await?;

        Ok(count > 0)
    }

    async fn find_user_by_id<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        actor_id: Uuid,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT actor_id, username, email FROM users WHERE actor_id = ?",
        )
        .bind(actor_id)
        .fetch_optional(executor)
        .await?;

        Ok(user)
    }

    async fn find_user_by_username<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        username: Username,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
            .bind(username.as_str())
            .fetch_optional(executor)
            .await?;

        Ok(user)
    }

    async fn all_actor_ids<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    ) -> Result<Vec<Uuid>> {
        let actor_ids = sqlx::query_scalar(
            r#"
            SELECT id FROM actors
            "#,
        )
        .fetch_all(executor)
        .await?;
        Ok(actor_ids)
    }

    ///////////// Activity /////////////

    async fn find_activity_by_id<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        id: Uuid,
    ) -> Result<Option<Activity>> {
        sqlx::query_as::<_, ActivitySqliteRow>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(executor)
        .await?
        .map(|r| r.to_activity())
        .transpose()
    }

    async fn all_activities<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    ) -> Result<Vec<Activity>> {
        sqlx::query_as::<_, ActivitySqliteRow>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities",
        )
        .fetch_all(executor)
        .await?
        .into_iter()
        .map(|r| r.to_activity())
        .collect()
    }

    ///////////// Entry /////////////

    async fn find_ancestors<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        entry_id: Uuid,
    ) -> Result<Vec<Uuid>> {
        let entry_id_str = entry_id.to_string();

        // Note: Can't use query! macro here because it requires a concrete connection at compile time.
        // Using query_as with a manual struct instead.
        let results: Vec<AncestorRow> = sqlx::query_as(
            r#"
            WITH RECURSIVE ancestors AS (
                SELECT id, parent_id, 0 as dist
                    FROM entries
                    WHERE id = ?
                UNION ALL
                SELECT e.id, e.parent_id, a.dist + 1 as dist
                    FROM entries e
                    INNER JOIN ancestors a ON a.parent_id = e.id
            )
            SELECT id, parent_id FROM ancestors
            ORDER BY dist
            "#,
        )
        .bind(entry_id_str)
        .fetch_all(executor)
        .await?;

        if results.is_empty() {
            return Err(DomainError::Other("entry not found".to_string()));
        }

        // Validate parent-child chain
        for (child, parent) in results.iter().tuple_windows() {
            let child_parent = child
                .parent_id
                .as_ref()
                .expect("non-root entries must have parent_id");
            let parent_id = &parent.id;
            assert_eq!(
                child_parent, parent_id,
                "broken ancestor chain: child parent_id {} != parent id {}",
                child_parent, parent_id
            );
        }

        // Last row must be root (no parent)
        assert!(
            results.last().unwrap().parent_id.is_none(),
            "root must have no parent"
        );

        // Extract IDs - SQLite returns UUIDs as strings, so parse them back
        let ancestors = results
            .into_iter()
            .map(|r| Uuid::parse_str(&r.id).expect("all entries must have valid UUID ids"))
            .collect();

        Ok(ancestors)
    }

    async fn find_entry_by_id<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        entry_id: Uuid,
    ) -> Result<Option<Entry>> {
        sqlx::query_as::<_, EntryRow>(
            r#"
            SELECT id, owner_id, activity_id, parent_id, frac_index, is_template, display_as_sets, is_sequence, start_time, end_time, duration_ms
            FROM entries
            WHERE id = ?
            "#,
        )
        .bind(entry_id.to_string())
        .fetch_optional(executor)
        .await?
        .map(|e| e.to_entry())
        .transpose()
    }
}

pub mod tests {
    use super::*;
    use gv_core::{SYSTEM_ACTOR_ID, actions::CreateActivity};

    #[sqlx::test(migrations = "./migrations")]
    fn test_create_activity(pool: SqlitePool) {
        let sqlite_client = SqliteClient { pool };

        let id = Uuid::new_v4();
        let activity = Activity {
            id: id.clone(),
            owner_id: SYSTEM_ACTOR_ID,
            name: ActivityName::parse("test".to_string()).unwrap(),
            description: None,
            source_activity_id: None,
        };
        let create_activity: CreateActivity = activity.into();
        let action: Action = create_activity.into();

        let _ = sqlite_client.run_action(action).await;

        let queried_activity = SqliteReader::find_activity_by_id(&sqlite_client.pool, id)
            .await
            .unwrap();

        assert!(queried_activity.is_some());
    }
}

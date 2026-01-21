use gv_core::{
    actions::Action,
    error::{DomainError, Result},
    models::{
        activity::Activity,
        entry::{Entry, EntryRow},
        user::User,
    },
    sandbox::{Reader, compute_mutation},
    validation::{Email, Username},
};
use itertools::Itertools;
use sqlx::PgPool;
use uuid::Uuid;

use crate::apply::PgApply;

///////////// SERVER /////////////

pub struct PostgresServer {
    pub pool: PgPool,
}

impl PostgresServer {
    pub fn new(pool: PgPool) -> Self {
        PostgresServer { pool }
    }

    pub async fn run_action(&self, action: Action) -> Result<()> {
        // Begin Postgres transaction.
        let mut tx = self.pool.begin().await?;

        // Create mutation.
        let mx = match action {
            Action::CreateActivity(action) => {
                compute_mutation::create_activity::<sqlx::Postgres, PostgresReader>(&mut tx, action)
                    .await?
            }
            Action::CreateUser(action) => {
                compute_mutation::create_user::<sqlx::Postgres, PostgresReader>(&mut tx, action)
                    .await?
            }
            Action::CreateEntry(action) => {
                compute_mutation::create_entry::<sqlx::Postgres, PostgresReader>(&mut tx, action)
                    .await?
            }
            Action::MoveEntry(action) => {
                compute_mutation::move_entry::<sqlx::Postgres, PostgresReader>(&mut tx, action)
                    .await?
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

/// Helper struct for ancestor query results.
struct AncestorRow {
    id: Uuid,
    parent_id: Option<Uuid>,
}

pub struct PostgresReader;
impl Reader<sqlx::Postgres> for PostgresReader {
    ///////////// Authn /////////////
    async fn is_email_registered<'e, E>(executor: E, email: Email) -> Result<bool>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = $1")
            .bind(email.as_str())
            .fetch_one(executor)
            .await?;

        Ok(count > 0)
    }

    async fn find_user_by_id<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
        actor_id: Uuid,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT actor_id, username, email FROM users WHERE actor_id = $1",
        )
        .bind(actor_id)
        .fetch_optional(executor)
        .await?;

        Ok(user)
    }

    async fn find_user_by_username<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
        username: Username,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
            .bind(username.as_str())
            .fetch_optional(executor)
            .await?;

        Ok(user)
    }

    async fn all_actor_ids<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
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
        executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
        id: Uuid,
    ) -> Result<Option<Activity>> {
        let activity = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(executor)
        .await?;

        Ok(activity)
    }

    async fn all_activities<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
    ) -> Result<Vec<Activity>> {
        let activities = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities",
        )
        .fetch_all(executor)
        .await?;

        Ok(activities)
    }

    ///////////// Entry /////////////

    async fn find_ancestors<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
        entry_id: Uuid,
    ) -> Result<Vec<Uuid>> {
        // Postgres returns UUIDs directly, but we need to map the row manually
        // since we can't use query! macro here (requires concrete connection at compile time).
        let results: Vec<(Uuid, Option<Uuid>)> = sqlx::query_as(
            r#"
            WITH RECURSIVE ancestors AS (
                SELECT id, parent_id, 0 as dist
                    FROM entries
                    WHERE id = $1
                UNION ALL
                SELECT e.id, e.parent_id, a.dist + 1 as dist
                    FROM entries e
                    INNER JOIN ancestors a ON a.parent_id = e.id
            )
            SELECT id, parent_id FROM ancestors
            ORDER BY dist
            "#,
        )
        .bind(entry_id)
        .fetch_all(executor)
        .await?;

        if results.is_empty() {
            return Err(DomainError::Other("entry not found".to_string()));
        }

        // Convert to AncestorRow for validation
        let results: Vec<AncestorRow> = results
            .into_iter()
            .map(|(id, parent_id)| AncestorRow { id, parent_id })
            .collect();

        // Validate parent-child chain
        for (child, parent) in results.iter().tuple_windows() {
            let child_parent = child
                .parent_id
                .expect("non-root entries must have parent_id");
            let parent_id = parent.id;
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

        // Extract IDs
        let ancestors = results.into_iter().map(|r| r.id).collect();

        Ok(ancestors)
    }

    async fn find_entry_by_id<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
        entry_id: Uuid,
    ) -> Result<Option<Entry>> {
        sqlx::query_as::<_, EntryRow>(
            r#"
            SELECT id, owner_id, activity_id, parent_id, frac_index, is_template, display_as_sets, is_sequence, start_time, end_time, duration_ms
            FROM entries
            WHERE id = $1
            "#,
        )
        .bind(entry_id)
        .fetch_optional(executor)
        .await?
        .map(|e| e.to_entry())
        .transpose()
    }
}

pub mod tests {
    pub use super::*;
    pub use gv_core::{SYSTEM_ACTOR_ID, actions::CreateActivity};

    #[sqlx::test(migrations = "./migrations")]
    fn test_create_activity(pool: PgPool) {
        let postgres_server = PostgresServer { pool };

        let id = Uuid::new_v4();
        let activity = Activity {
            id,
            owner_id: SYSTEM_ACTOR_ID,
            name: gv_core::models::activity::ActivityName::parse("test".to_string()).unwrap(),
            description: None,
            source_activity_id: None,
        };
        let create_activity: CreateActivity = activity.into();
        let action: Action = create_activity.into();

        let _ = postgres_server.run_action(action).await;

        let queried_activity = PostgresReader::find_activity_by_id(&postgres_server.pool, id)
            .await
            .unwrap();

        assert!(queried_activity.is_some());
    }
}

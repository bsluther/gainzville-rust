use gv_core::{
    error::{DomainError, Result},
    models::{
        activity::Activity,
        entry::{Entry, EntryRow},
        user::User,
    },
    reader::Reader,
    validation::{Email, Username},
};
use itertools::Itertools;

use uuid::Uuid;

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

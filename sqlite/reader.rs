use gv_core::{
    error::{DomainError, Result},
    models::{
        activity::Activity,
        attribute::{Attribute, AttributeRow, Value, ValueRow},
        entry::{Entry, EntryRow},
        entry_view::{EntryView, EntryViewRow},
        user::User,
    },
    reader::Reader,
    validation::Username,
};
use itertools::Itertools;
use sqlx::{
    FromRow,
    types::chrono::{DateTime, Utc},
};

use uuid::Uuid;

pub struct SqliteReader;
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
        let activity = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(executor)
        .await?;
        Ok(activity)
    }

    async fn all_activities<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    ) -> Result<Vec<Activity>> {
        let activities = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities",
        )
        .fetch_all(executor)
        .await?;
        println!("all activites.len={}", activities.len());
        Ok(activities)
    }

    ///////////// Entry /////////////

    async fn all_entries<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    ) -> Result<Vec<Entry>> {
        sqlx::query_as::<_, EntryRow>("SELECT * FROM entries")
            .fetch_all(executor)
            .await?
            .into_iter()
            .map(|r| r.to_entry())
            .collect()
    }

    async fn find_ancestors<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        entry_id: Uuid,
    ) -> Result<Vec<Uuid>> {
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
        .bind(entry_id)
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

        let ancestors = results.into_iter().map(|r| r.id).collect();

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
        .bind(entry_id)
        .fetch_optional(executor)
        .await?
        .map(|e| e.to_entry())
        .transpose()
    }

    async fn find_entry_view_by_id<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        entry_id: Uuid,
    ) -> Result<Option<EntryView>> {
        sqlx::query_as::<_, EntryViewRow>(
            r#"
            SELECT
                e.id, e.activity_id, e.owner_id, e.parent_id, e.frac_index,
                e.is_template, e.display_as_sets, e.is_sequence,
                e.start_time, e.end_time, e.duration_ms,
                a.id as act_id, a.owner_id as act_owner_id,
                a.source_activity_id as act_source_activity_id,
                a.name as act_name, a.description as act_description
            FROM entries e
            LEFT JOIN activities a ON e.activity_id = a.id
            WHERE e.id = ?
            "#,
        )
        .bind(entry_id)
        .fetch_optional(executor)
        .await?
        .map(|row| row.to_entry_view())
        .transpose()
    }

    async fn find_descendants<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        entry_id: Uuid,
    ) -> Result<Vec<Entry>> {
        sqlx::query_as::<sqlx::Sqlite, EntryRow>(
            r#"
            WITH RECURSIVE tree AS (
                SELECT * FROM entries e
                WHERE e.id = ?
                UNION ALL
                SELECT c.* FROM entries c
                    INNER JOIN tree ON c.parent_id = tree.id
            )
            SELECT * FROM tree
            "#,
        )
        .bind(entry_id)
        .fetch_all(executor)
        .await?
        .into_iter()
        .map(|e| e.to_entry())
        .collect()
    }

    ///////////// Attribute /////////////

    async fn find_attribute_by_id<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        attribute_id: Uuid,
    ) -> Result<Option<Attribute>> {
        sqlx::query_as::<_, AttributeRow>(
            "SELECT id, owner_id, name, data_type, config FROM attributes WHERE id = ?",
        )
        .bind(attribute_id)
        .fetch_optional(executor)
        .await?
        .map(|row| row.to_attribute())
        .transpose()
    }

    async fn find_attributes_by_owner<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        owner_id: Uuid,
    ) -> Result<Vec<Attribute>> {
        sqlx::query_as::<_, AttributeRow>(
            "SELECT id, owner_id, name, data_type, config FROM attributes WHERE owner_id = ?",
        )
        .bind(owner_id)
        .fetch_all(executor)
        .await?
        .into_iter()
        .map(|row| row.to_attribute())
        .collect()
    }

    ///////////// Value /////////////

    async fn find_value_by_key<'e>(
        executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
        entry_id: Uuid,
        attribute_id: Uuid,
    ) -> Result<Option<Value>> {
        sqlx::query_as::<_, ValueRow>(
            "SELECT entry_id, attribute_id, plan, actual, index_float, index_string FROM attribute_values WHERE entry_id = ? AND attribute_id = ?",
        )
        .bind(entry_id)
        .bind(attribute_id)
        .fetch_optional(executor)
        .await?
        .map(|row| row.to_value())
        .transpose()
    }
}
// TODO: move into Reader.
pub async fn entries_rooted_in_time_interval<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Entry>> {
    sqlx::query_as::<sqlx::Sqlite, EntryRow>(
        r#"
        WITH RECURSIVE forest AS (
            SELECT * FROM entries e
            WHERE e.start_time BETWEEN ? AND ?
                AND e.parent_id IS NULL
            UNION ALL
            SELECT * FROM entries c
                INNER JOIN forest ON c.parent_id = forest.id
        )
        SELECT * FROM forest
        "#,
    )
    .bind(from)
    .bind(to)
    .fetch_all(executor)
    .await?
    .into_iter()
    .map(|r| r.to_entry())
    .collect()
}

/// Helper struct for ancestor query results.
#[derive(FromRow)]
struct AncestorRow {
    id: Uuid,
    parent_id: Option<Uuid>,
}

mod tests {
    // use super::*;
}

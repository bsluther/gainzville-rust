use gv_core::{
    error::{DomainError, Result},
    models::{
        activity::Activity,
        attribute::{Attribute, AttributeRow, Value, ValueRow},
        entry::{Entry, EntryRow},
        entry_join::{EntryJoin, EntryJoinRow},
        user::User,
    },
    reader::Reader,
    validation::{Email, Username},
};
use itertools::Itertools;
use sqlx::types::chrono::{DateTime, Utc};

use uuid::Uuid;

/// Helper struct for ancestor query results.
struct AncestorRow {
    id: Uuid,
    parent_id: Option<Uuid>,
}

pub struct PostgresReader;
impl Reader<sqlx::Postgres> for PostgresReader {
    ///////////// Authn /////////////
    async fn is_email_registered(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        email: Email,
    ) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = $1")
            .bind(email.as_str())
            .fetch_one(&mut *connection)
            .await?;

        Ok(count > 0)
    }

    async fn find_user_by_id(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        actor_id: Uuid,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT actor_id, username, email FROM users WHERE actor_id = $1",
        )
        .bind(actor_id)
        .fetch_optional(&mut *connection)
        .await?;

        Ok(user)
    }

    async fn find_user_by_username(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        username: Username,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
            .bind(username.as_str())
            .fetch_optional(&mut *connection)
            .await?;

        Ok(user)
    }

    async fn all_actor_ids(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
    ) -> Result<Vec<Uuid>> {
        let actor_ids = sqlx::query_scalar(
            r#"
            SELECT id FROM actors
            "#,
        )
        .fetch_all(&mut *connection)
        .await?;
        Ok(actor_ids)
    }

    ///////////// Activity /////////////

    async fn find_activity_by_id(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        id: Uuid,
    ) -> Result<Option<Activity>> {
        let activity = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?;

        Ok(activity)
    }

    async fn all_activities(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
    ) -> Result<Vec<Activity>> {
        let activities = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities",
        )
        .fetch_all(&mut *connection)
        .await?;

        Ok(activities)
    }

    ///////////// Entry /////////////

    async fn all_entries(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
    ) -> Result<Vec<Entry>> {
        sqlx::query_as::<_, EntryRow>("SELECT * FROM entries")
            .fetch_all(&mut *connection)
            .await?
            .into_iter()
            .map(|r| r.to_entry())
            .collect()
    }

    async fn entries_rooted_in_time_interval(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Entry>> {
        sqlx::query_as::<sqlx::Postgres, EntryRow>(
            r#"
            WITH RECURSIVE forest AS (
                SELECT * FROM entries e
                WHERE e.start_time BETWEEN $1 AND $2
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
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(|r| r.to_entry())
        .collect()
    }

    async fn find_ancestors(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
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
        .fetch_all(&mut *connection)
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

    async fn find_entry_by_id(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
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
        .fetch_optional(&mut *connection)
        .await?
        .map(|e| e.to_entry())
        .transpose()
    }

    async fn find_entry_join_by_id(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        entry_id: Uuid,
    ) -> Result<Option<EntryJoin>> {
        let mut entry_join = sqlx::query_as::<_, EntryJoinRow>(
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
            WHERE e.id = $1
            "#,
        )
        .bind(entry_id)
        .fetch_optional(&mut *connection)
        .await?
        .map(|row| row.to_entry_view())
        .transpose()?;

        if let Some(entry_join) = &mut entry_join {
            let values = PostgresReader::find_values_for_entry(&mut *connection, entry_id).await?;

            for value in values {
                entry_join.values.insert(value.attribute_id, value);
            }
        };

        Ok(entry_join)
    }

    async fn find_descendants(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        entry_id: Uuid,
    ) -> Result<Vec<Entry>> {
        sqlx::query_as::<sqlx::Postgres, EntryRow>(
            r#"
            WITH RECURSIVE tree AS (
                SELECT * FROM entries e
                WHERE e.id = $1
                UNION ALL
                SELECT c.* FROM entries c
                    INNER JOIN tree ON c.parent_id = tree.id
            )
            SELECT * FROM tree
            "#,
        )
        .bind(entry_id)
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(|e| e.to_entry())
        .collect()
    }

    ///////////// Attribute /////////////

    async fn find_attribute_by_id(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        attribute_id: Uuid,
    ) -> Result<Option<Attribute>> {
        sqlx::query_as::<_, AttributeRow>(
            "SELECT id, owner_id, name, data_type, config FROM attributes WHERE id = $1",
        )
        .bind(attribute_id)
        .fetch_optional(&mut *connection)
        .await?
        .map(|row| row.to_attribute())
        .transpose()
    }

    async fn find_attributes_by_owner(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        owner_id: Uuid,
    ) -> Result<Vec<Attribute>> {
        sqlx::query_as::<_, AttributeRow>(
            "SELECT id, owner_id, name, data_type, config FROM attributes WHERE owner_id = $1",
        )
        .bind(owner_id)
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(|row| row.to_attribute())
        .collect()
    }

    ///////////// Value /////////////

    async fn find_value_by_key(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        entry_id: Uuid,
        attribute_id: Uuid,
    ) -> Result<Option<Value>> {
        sqlx::query_as::<_, ValueRow>(
            "SELECT entry_id, attribute_id, plan, actual, index_float, index_string FROM attribute_values WHERE entry_id = $1 AND attribute_id = $2",
        )
        .bind(entry_id)
        .bind(attribute_id)
        .fetch_optional(&mut *connection)
        .await?
        .map(|row| row.to_value())
        .transpose()
    }

    async fn find_values_for_entry(
        connection: &mut <sqlx::Postgres as sqlx::Database>::Connection,
        entry_id: Uuid,
    ) -> Result<Vec<Value>> {
        sqlx::query_as::<_, ValueRow>(
            r#"
            SELECT entry_id, attribute_id, plan, actual, index_float, index_string FROM attribute_values WHERE entry_id = $1
        "#)
        .bind(entry_id)
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(|row| row.to_value())
        .collect()
    }
}

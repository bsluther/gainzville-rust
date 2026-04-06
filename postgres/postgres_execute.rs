use gv_core::{
    error::{DomainError, Result},
    models::{
        activity::Activity,
        attribute::{AttributeRow, ValueRow},
        attribute_pair::AttributePairRow,
        entry::EntryRow,
        entry_join::{EntryJoin, EntryJoinRow},
        user::User,
    },
    queries::*,
};
use itertools::Itertools;
use sqlx::PgConnection;
use uuid::Uuid;

#[allow(async_fn_in_trait)]
pub(crate) trait PostgresExecute: Query {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response>;
}

// --- Auth ---

impl PostgresExecute for IsEmailRegistered {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = $1")
            .bind(self.email.as_str())
            .fetch_one(&mut *conn)
            .await?;

        Ok(count > 0)
    }
}

impl PostgresExecute for FindUserById {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        let user = sqlx::query_as::<_, User>(
            "SELECT actor_id, username, email FROM users WHERE actor_id = $1",
        )
        .bind(self.actor_id)
        .fetch_optional(&mut *conn)
        .await?;

        Ok(user)
    }
}

impl PostgresExecute for FindUserByUsername {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
            .bind(self.username.as_str())
            .fetch_optional(&mut *conn)
            .await?;

        Ok(user)
    }
}

impl PostgresExecute for AllActorIds {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        let actor_ids = sqlx::query_scalar("SELECT id FROM actors")
            .fetch_all(&mut *conn)
            .await?;
        Ok(actor_ids)
    }
}

// --- Activity ---

impl PostgresExecute for FindActivityById {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        let activity = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities WHERE id = $1",
        )
        .bind(self.id)
        .fetch_optional(&mut *conn)
        .await?;

        Ok(activity)
    }
}

impl PostgresExecute for AllActivities {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        let activities = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities",
        )
        .fetch_all(&mut *conn)
        .await?;

        Ok(activities)
    }
}

// --- Entry ---

impl PostgresExecute for AllEntries {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        sqlx::query_as::<_, EntryRow>("SELECT * FROM entries")
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .map(|r| r.to_entry())
            .collect()
    }
}

impl PostgresExecute for EntriesRootedInTimeInterval {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
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
        .bind(self.from)
        .bind(self.to)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|r| r.to_entry())
        .collect()
    }
}

impl PostgresExecute for FindAncestors {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
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
        .bind(self.entry_id)
        .fetch_all(&mut *conn)
        .await?;

        if results.is_empty() {
            return Err(DomainError::Other("entry not found".to_string()));
        }

        let results: Vec<AncestorRow> = results
            .into_iter()
            .map(|(id, parent_id)| AncestorRow { id, parent_id })
            .collect();

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

        assert!(
            results.last().unwrap().parent_id.is_none(),
            "root must have no parent"
        );

        Ok(results.into_iter().map(|r| r.id).collect())
    }
}

impl PostgresExecute for FindEntryById {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        sqlx::query_as::<_, EntryRow>(
            r#"
            SELECT id, owner_id, activity_id, parent_id, frac_index, is_template, display_as_sets, is_sequence, is_complete, start_time, end_time, duration_ms
            FROM entries
            WHERE id = $1
            "#,
        )
        .bind(self.entry_id)
        .fetch_optional(&mut *conn)
        .await?
        .map(|e| e.to_entry())
        .transpose()
    }
}

impl PostgresExecute for FindEntryJoinById {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        let row = sqlx::query_as::<_, EntryJoinRow>(
            r#"
            SELECT
                e.id, e.activity_id, e.owner_id, e.parent_id, e.frac_index,
                e.is_template, e.display_as_sets, e.is_sequence, e.is_complete,
                e.start_time, e.end_time, e.duration_ms,
                a.id as act_id, a.owner_id as act_owner_id,
                a.source_activity_id as act_source_activity_id,
                a.name as act_name, a.description as act_description
            FROM entries e
            LEFT JOIN activities a ON e.activity_id = a.id
            WHERE e.id = $1
            "#,
        )
        .bind(self.entry_id)
        .fetch_optional(&mut *conn)
        .await?;

        match row {
            None => Ok(None),
            Some(row) => {
                let pairs = (FindAttributePairsForEntry {
                    entry_id: self.entry_id,
                })
                .execute_postgres(conn)
                .await?;
                let attributes = pairs.into_iter().map(|p| (p.attr_id(), p)).collect();
                Ok(Some(EntryJoin::from_row(row, attributes)?))
            }
        }
    }
}

impl PostgresExecute for FindDescendants {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
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
        .bind(self.entry_id)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|e| e.to_entry())
        .collect()
    }
}

// --- Attribute ---

impl PostgresExecute for FindAttributeById {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        sqlx::query_as::<_, AttributeRow>(
            "SELECT id, owner_id, name, data_type, config FROM attributes WHERE id = $1",
        )
        .bind(self.attribute_id)
        .fetch_optional(&mut *conn)
        .await?
        .map(|row| row.to_attribute())
        .transpose()
    }
}

impl PostgresExecute for AllAttributes {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        sqlx::query_as::<_, AttributeRow>(
            "SELECT id, owner_id, name, data_type, config FROM attributes",
        )
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|row| row.to_attribute())
        .collect()
    }
}

impl PostgresExecute for FindAttributesByOwner {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        sqlx::query_as::<_, AttributeRow>(
            "SELECT id, owner_id, name, data_type, config FROM attributes WHERE owner_id = $1",
        )
        .bind(self.owner_id)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|row| row.to_attribute())
        .collect()
    }
}

// --- Value ---

impl PostgresExecute for FindValueByKey {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        sqlx::query_as::<_, ValueRow>(
            "SELECT entry_id, attribute_id, plan, actual, index_float, index_string FROM attribute_values WHERE entry_id = $1 AND attribute_id = $2",
        )
        .bind(self.entry_id)
        .bind(self.attribute_id)
        .fetch_optional(&mut *conn)
        .await?
        .map(|row| row.to_value())
        .transpose()
    }
}

impl PostgresExecute for FindValuesForEntry {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        sqlx::query_as::<_, ValueRow>(
            "SELECT entry_id, attribute_id, plan, actual, index_float, index_string FROM attribute_values WHERE entry_id = $1",
        )
        .bind(self.entry_id)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|row| row.to_value())
        .collect()
    }
}

impl PostgresExecute for FindValuesForEntries {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        sqlx::query_as::<_, ValueRow>(
            "SELECT entry_id, attribute_id, plan, actual, index_float, index_string FROM attribute_values WHERE entry_id = ANY($1)",
        )
        .bind(&self.entry_ids[..])
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|row| row.to_value())
        .collect()
    }
}

impl PostgresExecute for FindAttributePairsForEntry {
    async fn execute_postgres(&self, conn: &mut PgConnection) -> Result<Self::Response> {
        sqlx::query_as::<_, AttributePairRow>(
            r#"
            SELECT
                a.id as attr_id, a.owner_id as attr_owner_id,
                a.name as attr_name, a.data_type as attr_data_type,
                a.config as attr_config,
                v.entry_id, v.attribute_id, v.plan, v.actual,
                v.index_float, v.index_string
            FROM attribute_values v
            INNER JOIN attributes a ON v.attribute_id = a.id
            WHERE v.entry_id = $1
            "#,
        )
        .bind(self.entry_id)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|row| row.to_attribute_pair())
        .collect()
    }
}

struct AncestorRow {
    id: Uuid,
    parent_id: Option<Uuid>,
}

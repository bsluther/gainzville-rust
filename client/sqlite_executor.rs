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
    query_executor::QueryExecutor,
};
use itertools::Itertools;
use sqlx::{FromRow, SqliteConnection};
use uuid::Uuid;

pub struct SqliteQueryExecutor<'c> {
    conn: &'c mut SqliteConnection,
}

impl<'c> SqliteQueryExecutor<'c> {
    pub fn new(conn: &'c mut SqliteConnection) -> Self {
        SqliteQueryExecutor { conn }
    }
}

// --- Auth ---

impl QueryExecutor<IsEmailRegistered> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: IsEmailRegistered,
    ) -> Result<<IsEmailRegistered as Query>::Response> {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = ?")
            .bind(query.email.as_str())
            .fetch_one(&mut *self.conn)
            .await?;

        Ok(count > 0)
    }
}

impl QueryExecutor<FindUserById> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindUserById,
    ) -> Result<<FindUserById as Query>::Response> {
        let user = sqlx::query_as::<_, User>(
            "SELECT actor_id, username, email FROM users WHERE actor_id = ?",
        )
        .bind(query.actor_id)
        .fetch_optional(&mut *self.conn)
        .await?;

        Ok(user)
    }
}

impl QueryExecutor<FindUserByUsername> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindUserByUsername,
    ) -> Result<<FindUserByUsername as Query>::Response> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
            .bind(query.username.as_str())
            .fetch_optional(&mut *self.conn)
            .await?;

        Ok(user)
    }
}

impl QueryExecutor<AllActorIds> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        _query: AllActorIds,
    ) -> Result<<AllActorIds as Query>::Response> {
        let actor_ids = sqlx::query_scalar("SELECT id FROM actors")
            .fetch_all(&mut *self.conn)
            .await?;
        Ok(actor_ids)
    }
}

// --- Activity ---

impl QueryExecutor<FindActivityById> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindActivityById,
    ) -> Result<<FindActivityById as Query>::Response> {
        let activity = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities WHERE id = ?",
        )
        .bind(query.id)
        .fetch_optional(&mut *self.conn)
        .await?;
        Ok(activity)
    }
}

impl QueryExecutor<AllActivities> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        _query: AllActivities,
    ) -> Result<<AllActivities as Query>::Response> {
        let activities = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities",
        )
        .fetch_all(&mut *self.conn)
        .await?;
        Ok(activities)
    }
}

// --- Entry ---

impl QueryExecutor<AllEntries> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        _query: AllEntries,
    ) -> Result<<AllEntries as Query>::Response> {
        sqlx::query_as::<_, EntryRow>("SELECT * FROM entries")
            .fetch_all(&mut *self.conn)
            .await?
            .into_iter()
            .map(|r| r.to_entry())
            .collect()
    }
}

impl QueryExecutor<EntriesRootedInTimeInterval> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: EntriesRootedInTimeInterval,
    ) -> Result<<EntriesRootedInTimeInterval as Query>::Response> {
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
        .bind(query.from)
        .bind(query.to)
        .fetch_all(&mut *self.conn)
        .await?
        .into_iter()
        .map(|r| r.to_entry())
        .collect()
    }
}

impl QueryExecutor<FindAncestors> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindAncestors,
    ) -> Result<<FindAncestors as Query>::Response> {
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
        .bind(query.entry_id)
        .fetch_all(&mut *self.conn)
        .await?;

        if results.is_empty() {
            return Err(DomainError::Other("entry not found".to_string()));
        }

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

        assert!(
            results.last().unwrap().parent_id.is_none(),
            "root must have no parent"
        );

        Ok(results.into_iter().map(|r| r.id).collect())
    }
}

impl QueryExecutor<FindEntryById> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindEntryById,
    ) -> Result<<FindEntryById as Query>::Response> {
        sqlx::query_as::<_, EntryRow>(
            r#"
            SELECT id, owner_id, activity_id, name, parent_id, frac_index, is_template, display_as_sets, is_sequence, is_complete, start_time, end_time, duration_ms
            FROM entries
            WHERE id = ?
            "#,
        )
        .bind(query.entry_id)
        .fetch_optional(&mut *self.conn)
        .await?
        .map(|e| e.to_entry())
        .transpose()
    }
}

impl QueryExecutor<FindEntryJoinById> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindEntryJoinById,
    ) -> Result<<FindEntryJoinById as Query>::Response> {
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
            WHERE e.id = ?
            "#,
        )
        .bind(query.entry_id)
        .fetch_optional(&mut *self.conn)
        .await?;

        match row {
            None => Ok(None),
            Some(row) => {
                let pairs = self
                    .execute(FindAttributePairsForEntry {
                        entry_id: query.entry_id,
                    })
                    .await?;
                let attributes = pairs.into_iter().map(|p| (p.attr_id(), p)).collect();
                Ok(Some(EntryJoin::from_row(row, attributes)?))
            }
        }
    }
}

impl QueryExecutor<FindDescendants> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindDescendants,
    ) -> Result<<FindDescendants as Query>::Response> {
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
        .bind(query.entry_id)
        .fetch_all(&mut *self.conn)
        .await?
        .into_iter()
        .map(|e| e.to_entry())
        .collect()
    }
}

// --- Attribute ---

impl QueryExecutor<FindAttributeById> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindAttributeById,
    ) -> Result<<FindAttributeById as Query>::Response> {
        sqlx::query_as::<_, AttributeRow>(
            "SELECT id, owner_id, name, data_type, config FROM attributes WHERE id = ?",
        )
        .bind(query.attribute_id)
        .fetch_optional(&mut *self.conn)
        .await?
        .map(|row| row.to_attribute())
        .transpose()
    }
}

impl QueryExecutor<AllAttributes> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        _query: AllAttributes,
    ) -> Result<<AllAttributes as Query>::Response> {
        sqlx::query_as::<_, AttributeRow>(
            "SELECT id, owner_id, name, data_type, config FROM attributes",
        )
        .fetch_all(&mut *self.conn)
        .await?
        .into_iter()
        .map(|row| row.to_attribute())
        .collect()
    }
}

impl QueryExecutor<FindAttributesByOwner> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindAttributesByOwner,
    ) -> Result<<FindAttributesByOwner as Query>::Response> {
        sqlx::query_as::<_, AttributeRow>(
            "SELECT id, owner_id, name, data_type, config FROM attributes WHERE owner_id = ?",
        )
        .bind(query.owner_id)
        .fetch_all(&mut *self.conn)
        .await?
        .into_iter()
        .map(|row| row.to_attribute())
        .collect()
    }
}

// --- Value ---

impl QueryExecutor<FindValueByKey> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindValueByKey,
    ) -> Result<<FindValueByKey as Query>::Response> {
        sqlx::query_as::<_, ValueRow>(
            "SELECT entry_id, attribute_id, plan, actual, index_float, index_string FROM attribute_values WHERE entry_id = ? AND attribute_id = ?",
        )
        .bind(query.entry_id)
        .bind(query.attribute_id)
        .fetch_optional(&mut *self.conn)
        .await?
        .map(|row| row.to_value())
        .transpose()
    }
}

impl QueryExecutor<FindValuesForEntry> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindValuesForEntry,
    ) -> Result<<FindValuesForEntry as Query>::Response> {
        sqlx::query_as::<_, ValueRow>(
            "SELECT entry_id, attribute_id, plan, actual, index_float, index_string FROM attribute_values WHERE entry_id = ?",
        )
        .bind(query.entry_id)
        .fetch_all(&mut *self.conn)
        .await?
        .into_iter()
        .map(|row| row.to_value())
        .collect()
    }
}

impl QueryExecutor<FindValuesForEntries> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindValuesForEntries,
    ) -> Result<<FindValuesForEntries as Query>::Response> {
        if query.entry_ids.is_empty() {
            return Ok(vec![]);
        }
        let mut builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT entry_id, attribute_id, plan, actual, index_float, index_string FROM attribute_values WHERE entry_id IN (",
        );
        let mut separated = builder.separated(", ");
        for id in &query.entry_ids {
            separated.push_bind(*id);
        }
        builder.push(")");
        builder
            .build_query_as::<ValueRow>()
            .fetch_all(&mut *self.conn)
            .await?
            .into_iter()
            .map(|row| row.to_value())
            .collect()
    }
}

impl QueryExecutor<FindAttributePairsForEntry> for SqliteQueryExecutor<'_> {
    async fn execute(
        &mut self,
        query: FindAttributePairsForEntry,
    ) -> Result<<FindAttributePairsForEntry as Query>::Response> {
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
            WHERE v.entry_id = ?
            "#,
        )
        .bind(query.entry_id)
        .fetch_all(&mut *self.conn)
        .await?
        .into_iter()
        .map(|row| row.to_attribute_pair())
        .collect()
    }
}

#[derive(FromRow)]
struct AncestorRow {
    id: Uuid,
    parent_id: Option<Uuid>,
}

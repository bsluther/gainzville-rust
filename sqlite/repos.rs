use itertools::Itertools;
use sqlx::{Sqlite, Transaction};

use gv_core::{
    error::{DomainError, Result},
    models::entry::{Entry, EntryRow},
    models::{activity::Activity, user::User},
    repos::{ActivityRepo, AuthnRepo, EntryRepo},
    validation::{Email, Username},
};

// The Repo lives only as long as the Transaction borrow.
// Need to borrow as mutable because we are going to mutate the transaction.
// Transaction operations must run serially, i.e. the borrow must end before the next call.
// 'c is the SqliteContext lifetime.
// 't is the transction lifetime, needs to outlive the repo.
pub struct SqliteContext<'c, 't> {
    tx: &'c mut Transaction<'t, Sqlite>,
}

impl<'c, 't> SqliteContext<'c, 't> {
    pub fn new(tx: &'c mut Transaction<'t, Sqlite>) -> Self {
        Self { tx }
    }
}

impl<'c, 't> AuthnRepo for SqliteContext<'c, 't> {
    async fn is_email_registered(&mut self, email: Email) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = ?")
            .bind(email.as_str())
            .fetch_one(&mut **self.tx) // Deref magic to get the Executor
            .await?;

        Ok(count > 0)
    }

    async fn find_user_by_id(&mut self, actor_id: uuid::Uuid) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT actor_id, username, email FROM users WHERE actor_id = ?",
        )
        .bind(actor_id)
        .fetch_optional(&mut **self.tx)
        .await?;

        Ok(user)
    }

    async fn find_user_by_username(&mut self, username: Username) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
            .bind(username.as_str())
            .fetch_optional(&mut **self.tx)
            .await?;

        Ok(user)
    }
    async fn all_actor_ids(&mut self) -> Result<Vec<uuid::Uuid>> {
        let actor_ids = sqlx::query_scalar(
            r#"
            SELECT id FROM actors
            "#,
        )
        .fetch_all(&mut **self.tx)
        .await?;
        Ok(actor_ids)
    }
}

impl<'c, 't> ActivityRepo for SqliteContext<'c, 't> {
    async fn find_activity_by_id(&mut self, id: uuid::Uuid) -> Result<Option<Activity>> {
        let activity = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&mut **self.tx)
        .await?;

        Ok(activity)
    }
}

impl<'c, 't> EntryRepo for SqliteContext<'c, 't> {
    async fn find_ancestors(&mut self, entry_id: uuid::Uuid) -> Result<Vec<uuid::Uuid>> {
        let entry_id_str = entry_id.to_string();
        let results = sqlx::query!(
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
            entry_id_str
        )
        .fetch_all(&mut **self.tx)
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
            let parent_id = parent.id.as_ref().expect("all entries must have id");
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
            .map(|r| {
                let id_str = r.id.expect("all entries must have id");
                uuid::Uuid::parse_str(&id_str).expect("all entries must have valid UUID ids")
            })
            .collect();

        Ok(ancestors)
    }

    async fn find_entry_by_id(&mut self, entry_id: uuid::Uuid) -> Result<Option<Entry>> {
        sqlx::query_as::<_, EntryRow>(
            r#"
            SELECT id, owner_id, activity_id, parent_id, frac_index, is_template, display_as_sets, is_sequence, start_time, end_time, duration_ms
            FROM entries
            WHERE id = ?
            "#,
        )
        .bind(entry_id.to_string())
        .fetch_optional(&mut **self.tx)
        .await?
        .map(|e| e.to_entry())
        .transpose()
    }
}

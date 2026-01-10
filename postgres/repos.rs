use itertools::Itertools;
use sqlx::{Postgres, Transaction, query_as};

use gv_core::{
    error::{DomainError, Result},
    models::{
        activity::Activity,
        entry::{Entry, EntryRow, Position},
        user::User,
    },
    repos::{ActivityRepo, AuthnRepo, EntryRepo},
    validation::{Email, Username},
};
use uuid::Uuid;

// The PgContext lives only as long as the Transaction borrow.
// Need to borrow as mutable because we are going to mutate the transaction.
// Transaction operations must run serially, i.e. the borrow must end before the next call.
// 'c is the PgContext lifetime.
// 't is the transction lifetime, needs to outlive the PgContext.
pub struct PgContext<'c, 't> {
    tx: &'c mut Transaction<'t, Postgres>,
}

impl<'c, 't> PgContext<'c, 't> {
    pub fn new(tx: &'c mut Transaction<'t, Postgres>) -> Self {
        Self { tx }
    }
}

impl<'c, 't> AuthnRepo for PgContext<'c, 't> {
    async fn is_email_registered(&mut self, email: Email) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = $1")
            .bind(email.as_str())
            .fetch_one(&mut **self.tx)
            .await?;

        Ok(count > 0)
    }

    async fn find_user_by_id(&mut self, actor_id: uuid::Uuid) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT actor_id, username, email FROM users WHERE actor_id = $1",
        )
        .bind(actor_id)
        .fetch_optional(&mut **self.tx)
        .await?;

        Ok(user)
    }

    async fn find_user_by_username(&mut self, username: Username) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
            .bind(username.as_str())
            .fetch_optional(&mut **self.tx)
            .await?;

        Ok(user)
    }

    async fn all_actor_ids(&mut self) -> Result<Vec<uuid::Uuid>> {
        let actor_ids = sqlx::query_scalar!(
            r#"
            SELECT id FROM actors
            "#
        )
        .fetch_all(&mut **self.tx)
        .await?;
        Ok(actor_ids)
    }
}

impl<'c, 't> ActivityRepo for PgContext<'c, 't> {
    async fn find_activity_by_id(&mut self, id: Uuid) -> Result<Option<Activity>> {
        let activity = sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, source_activity_id, name, description FROM activities WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut **self.tx)
        .await?;

        Ok(activity)
    }
}

impl<'c, 't> EntryRepo for PgContext<'c, 't> {
    async fn find_ancestors(&mut self, entry_id: Uuid) -> Result<Vec<Uuid>> {
        let results = sqlx::query!(
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
            entry_id
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
                .expect("non-root entries must have parent_id");
            let parent_id = parent.id.expect("all entries must have id");
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
        let ancestors = results
            .into_iter()
            .map(|r| r.id.expect("all entries must have id"))
            .collect();

        Ok(ancestors)
    }

    async fn find_entry_by_id(&mut self, entry_id: Uuid) -> Result<Option<Entry>> {
        sqlx::query_as::<_, EntryRow>("SELECT * FROM entries WHERE id = $1")
            .bind(entry_id)
            .fetch_optional(&mut **self.tx)
            .await?
            .map(|e| e.to_entry())
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::PgController;
    use fractional_index::FractionalIndex;
    use gv_core::actions::CreateEntry;
    use gv_core::models::entry::{Entry, Temporal};
    use sqlx::PgPool;

    #[sqlx::test(migrations = "../postgres/migrations")]
    async fn test_find_ancestors_chain(pool: PgPool) {
        let pg_controller = PgController { pool: pool.clone() };
        let mut tx = pg_controller
            .pool
            .begin()
            .await
            .expect("begin transaction should not fail");
        let mut repo = PgContext::new(&mut tx);

        // Get a valid actor_id from the seeded data
        let actor_ids = repo.all_actor_ids().await.unwrap();
        let owner_id = actor_ids[0];

        // Create entry chain: a -> b -> c -> d
        // where d is the root (no parent) and a is the deepest child
        let entry_d_id = Uuid::new_v4();
        let entry_c_id = Uuid::new_v4();
        let entry_b_id = Uuid::new_v4();
        let entry_a_id = Uuid::new_v4();

        // Insert d (root, no parent)
        let entry_d = Entry {
            id: entry_d_id,
            owner_id,
            activity_id: None,
            display_as_sets: false,
            is_sequence: true,
            is_template: false,
            position: None,
            temporal: Temporal::None,
        };
        let create_d: CreateEntry = entry_d.clone().into();
        pg_controller
            .run_action(create_d.into())
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();

        // Insert c (parent = d)
        let entry_c = Entry {
            id: entry_c_id,
            owner_id,
            activity_id: None,
            display_as_sets: false,
            is_sequence: true,
            is_template: false,
            position: Some(Position {
                parent_id: entry_d_id,
                frac_index: FractionalIndex::default(),
            }),
            temporal: Temporal::None,
        };
        let create_c: CreateEntry = entry_c.into();
        pg_controller
            .run_action(create_c.into())
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();

        // Insert b (parent = c)
        let entry_b = Entry {
            id: entry_b_id,
            owner_id,
            activity_id: None,
            display_as_sets: false,
            is_sequence: true,
            is_template: false,
            position: Some(Position {
                parent_id: entry_c_id,
                frac_index: FractionalIndex::default(),
            }),
            temporal: Temporal::None,
        };
        let create_b: CreateEntry = entry_b.into();
        pg_controller
            .run_action(create_b.into())
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();

        // Insert a (parent = b)
        let entry_a = Entry {
            id: entry_a_id,
            owner_id,
            activity_id: None,
            display_as_sets: false,
            is_sequence: true,
            is_template: false,
            position: Some(Position {
                parent_id: entry_b_id,
                frac_index: FractionalIndex::default(),
            }),
            temporal: Temporal::None,
        };
        let create_a: CreateEntry = entry_a.into();
        pg_controller
            .run_action(create_a.into())
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();

        // Now test find_ancestors
        let mut tx = pg_controller.pool.begin().await.unwrap();
        let mut repo = PgContext::new(&mut tx);

        let ancestors = repo.find_ancestors(entry_a_id).await.unwrap();

        // Should return [a, b, c, d] in that order
        assert_eq!(ancestors.len(), 4);
        assert_eq!(ancestors[0], entry_a_id);
        assert_eq!(ancestors[1], entry_b_id);
        assert_eq!(ancestors[2], entry_c_id);
        assert_eq!(ancestors[3], entry_d_id);
    }
}

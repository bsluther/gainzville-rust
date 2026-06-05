use gv_core::{
    actions::Action,
    delta_executor::AnyDeltaExecutor,
    error::{DbErr, Result},
    mutators,
};

use sqlx::PgPool;
use tracing::instrument;

use gv_sql::postgres::{PostgresDeltaExecutor, PostgresQueryExecutor};

pub struct PostgresServer {
    pub pool: PgPool,
}

impl PostgresServer {
    pub fn new(pool: PgPool) -> Self {
        PostgresServer { pool }
    }

    #[instrument(skip(self), level = "info", err(level = "warn"))]
    pub async fn run_action(&self, action: Action) -> Result<mutators::Mutation> {
        // Begin Postgres transaction.
        let mut tx = self.pool.begin().await.db_err()?;
        let mut executor = PostgresQueryExecutor::new(&mut tx);

        // Create mutation.
        let mx = match action {
            Action::CreateActivity(action) => {
                mutators::create_activity(&mut executor, action).await?
            }
            Action::CreateUser(action) => mutators::create_user(&mut executor, action).await?,
            Action::CreateEntry(action) => mutators::create_entry(&mut executor, action).await?,
            Action::CreateEntryFromActivity(action) => {
                mutators::create_entry_from_activity(&mut executor, action).await?
            }
            Action::MoveEntry(action) => mutators::move_entry(&mut executor, action).await?,
            Action::DeleteEntryRecursive(action) => {
                mutators::delete_entry_recursive(&mut executor, action).await?
            }
            Action::CreateAttribute(action) => {
                mutators::create_attribute(&mut executor, action).await?
            }
            Action::CreateValue(action) => mutators::create_value(&mut executor, action).await?,
            Action::AttachValue(action) => mutators::attach_value(&mut executor, action).await?,
            Action::DeleteAttributeValue(action) => {
                mutators::delete_attribute_value(&mut executor, action).await?
            }
            Action::UpdateEntryCompletion(action) => {
                mutators::update_entry_completion(&mut executor, action).await?
            }
            Action::UpdateAttributeValue(action) => {
                mutators::update_attribute_value(&mut executor, action).await?
            }
            Action::UpdateAttribute(action) => {
                mutators::update_attribute(&mut executor, action).await?
            }
            Action::UpdateEntry(action) => mutators::update_entry(&mut executor, action).await?,
        };

        // TODO: log mutation in this transaction.
        // sync_service.log_mutation(mx);

        // Defer FK constraint checking until commit so delta order doesn't matter.
        sqlx::query("SET CONSTRAINTS ALL DEFERRED")
            .execute(&mut *tx)
            .await
            .db_err()?;

        let mut delta_executor = PostgresDeltaExecutor::new(&mut *tx);
        // Apply deltas.
        for delta in mx.changes.iter().cloned() {
            // delta.apply_delta(&mut tx).await?;
            delta_executor.apply_any_delta(delta).await?;
        }

        // Commit the transaction.
        tx.commit().await.db_err()?;

        // TODO: send mutation to service (or add to a pending_mutations queue).
        // sync_service.append_applied_mutation(mx);

        Ok(mx)
    }
}

pub mod tests {
    pub use super::*;
    pub use gv_core::{SYSTEM_ACTOR_ID, actions::CreateActivity};
    pub use gv_core::{
        models::activity::Activity, queries::FindActivityById, query_executor::QueryExecutor,
    };
    pub use gv_sql::postgres::PostgresQueryExecutor;
    pub use uuid::Uuid;

    #[sqlx::test(migrations = "../gv-sql/postgres/migrations")]
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

        let queried_activity = {
            let mut connection = postgres_server.pool.acquire().await.unwrap();
            PostgresQueryExecutor::new(&mut *connection)
                .execute(FindActivityById { id })
                .await
                .unwrap()
        };

        assert!(queried_activity.is_some());
    }
}

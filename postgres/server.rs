use gv_core::{actions::Action, error::Result, mutators};

use sqlx::PgPool;
use tracing::instrument;

use crate::{apply::PgApply, reader::PostgresReader};

pub struct PostgresServer {
    pub pool: PgPool,
}

impl PostgresServer {
    pub fn new(pool: PgPool) -> Self {
        PostgresServer { pool }
    }

    #[instrument(skip(self), level = "info", err(level = "warn"))]
    pub async fn run_action(&self, action: Action) -> Result<()> {
        // Begin Postgres transaction.
        let mut tx = self.pool.begin().await?;

        // Create mutation.
        let mx = match action {
            Action::CreateActivity(action) => {
                mutators::create_activity::<sqlx::Postgres, PostgresReader>(&mut tx, action).await?
            }
            Action::CreateUser(action) => {
                mutators::create_user::<sqlx::Postgres, PostgresReader>(&mut tx, action).await?
            }
            Action::CreateEntry(action) => {
                mutators::create_entry::<sqlx::Postgres, PostgresReader>(&mut tx, action).await?
            }
            Action::MoveEntry(action) => {
                mutators::move_entry::<sqlx::Postgres, PostgresReader>(&mut tx, action).await?
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

pub mod tests {
    pub use super::*;
    pub use crate::reader::PostgresReader;
    pub use gv_core::{SYSTEM_ACTOR_ID, actions::CreateActivity};
    pub use gv_core::{models::activity::Activity, reader::Reader};
    pub use uuid::Uuid;

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

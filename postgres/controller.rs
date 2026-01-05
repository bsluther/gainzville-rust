use sqlx::{PgPool, Postgres, Transaction};
use tracing::instrument;

use crate::{apply::PgApply, repos::PgContext};
use gv_core::{
    actions::{Action, ActionService},
    error::Result,
};

#[derive(Debug)]
pub struct PgController {
    pub pool: PgPool,
}

impl PgController {
    #[instrument(skip_all)]
    pub async fn run_action<'a>(&'a self, action: Action) -> Result<Transaction<'a, Postgres>> {
        // Begin PG transaction.
        let mut tx = self.pool.begin().await?;

        // Create PgContext.
        let pg_context = PgContext::new(&mut tx);

        // Create mutation.
        let mx = match action {
            Action::CreateActivity(action) => {
                ActionService::create_activity(pg_context, action).await?
            }
            Action::CreateUser(action) => ActionService::create_user(pg_context, action).await?,
            Action::CreateEntry(action) => ActionService::create_entry(pg_context, action).await?,
        };

        // Apply deltas.
        for delta in mx.changes {
            delta.apply_delta(&mut tx).await?;
        }

        // Do not commit the transaction, leave it up to the caller. This allows for rollback in
        // testing.
        Ok(tx)
    }
}

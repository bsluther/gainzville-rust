use sqlx::{Sqlite, SqlitePool, Transaction};
use tracing::instrument;

use crate::{
    core::{
        actions::{Action, ActionService, CreateUser},
        error::Result,
        models::user::User,
    },
    postgres::repos::PgContext,
    sqlite::{apply::SqliteApply, repos::SqliteContext},
};

pub struct SqliteController {
    pub pool: SqlitePool,
}

impl SqliteController {
    #[instrument(skip_all)]
    pub async fn run_action<'a>(&'a self, action: Action) -> Result<Transaction<'a, Sqlite>> {
        // Begin Sqlite transaction.
        let mut tx = self.pool.begin().await?;

        // Create SqliteContext.
        let pg_context = SqliteContext::new(&mut tx);

        // Create mutation.
        let mx = match action {
            Action::CreateActivity(action) => {
                ActionService::create_activity(pg_context, action).await?
            }
            Action::CreateUser(action) => ActionService::create_user(pg_context, action).await?,
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

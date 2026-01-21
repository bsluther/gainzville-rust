use sqlx::{Executor, Sqlite, SqlitePool, Transaction};
use tracing::instrument;

use crate::{
    apply::SqliteApply,
    repos::{SqliteContext, SqliteRepo2},
};
use gv_core::{
    actions::{Action, ActionService},
    error::Result,
    repos::ActivityRepo2,
};

#[derive(Debug, Clone)]
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
            Action::CreateEntry(action) => ActionService::create_entry(pg_context, action).await?,
            Action::MoveEntry(action) => ActionService::move_entry(pg_context, action).await?,
        };

        // Apply deltas.
        for delta in mx.changes {
            delta.apply_delta(&mut tx).await?;
        }

        // Do not commit the transaction, leave it up to the caller. This allows for rollback in
        // testing.
        Ok(tx)
    }

    pub async fn compute_mutation2<'t>(
        &self,
        tx: &mut Transaction<'t, sqlx::Sqlite>,
        action: Action,
    ) -> Result<()> {
        let sqlite_repo2 = SqliteRepo2 {};

        // Compute mutation.
        let mx = match action {
            Action::CreateActivity(action) => {
                ActionService::create_activity2(&mut **tx, sqlite_repo2, action).await?
            }
            Action::CreateUser(action) => {
                unimplemented!()
                // ActionService::create_user(pg_context, action).await?
            }
            Action::CreateEntry(action) => {
                unimplemented!()
                // ActionService::create_entry(pg_context, action).await?
            }
            Action::MoveEntry(action) => {
                unimplemented!()
                // ActionService::move_entry(pg_context, action).await?
            }
        };

        // Apply deltas.
        for delta in mx.changes {
            delta.apply_delta(&mut *tx).await?;
        }

        // Do not commit the transaction, leave it up to the caller. This allows for rollback in
        // testing.
        Ok(())
    }
}

use sqlx::SqlitePool;

use crate::{
    core::{actions::ActionService, error::Result, models::user::User},
    sqlite::{apply::SqliteApply, repos::SqliteContext},
};

pub struct SqliteController {
    pub pool: SqlitePool,
}

impl SqliteController {
    pub async fn handle_create_user(&self, user: User) -> Result<()> {
        // Begin SQLite transaction.
        let mut tx = self.pool.begin().await?;

        // Create AuthnRepo (which is a trait on SqliteContext).
        // SqliteContext is a god object, which is unfortunate, but it was the most expedient way to
        // get a transaction piped through.
        let sqlite_context = SqliteContext::new(&mut tx);

        // Create mutation.
        let mx = ActionService::create_user(sqlite_context, user).await?;

        // Apply deltas.
        for delta in mx.changes {
            delta.apply_delta(&mut tx).await?;
        }

        // Commit transacton.
        // TODO: bring in Tracing!
        println!("Committing...");
        let res = tx.commit().await;
        match res {
            Ok(_) => {
                println!("Committed!");
            }
            Err(e) => {
                println!("Error: {e}");
            }
        }
        Ok(())
    }
}

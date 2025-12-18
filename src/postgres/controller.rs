use sqlx::PgPool;

use crate::{
    core::{actions::ActionService, error::Result, models::user::User},
    postgres::{apply::PgApply, repos::PgContext},
};

pub struct PgController {
    pub pool: PgPool,
}

impl PgController {
    pub async fn handle_create_user(&self, user: User) -> Result<()> {
        // Begin PG transaction.
        let mut tx = self.pool.begin().await?;

        // Create AuthnRepo (which is a trait on PgContext).
        // PgContext is a god object, which is unfortunate, but it was the most expedient way to
        // get a transaction piped through.
        let pg_context = PgContext::new(&mut tx);

        // Create mutation.
        let mx = ActionService::create_user(pg_context, user).await?;

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

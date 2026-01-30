use futures_core::Stream;
use gv_core::{
    actions::Action,
    error::Result,
    models::{activity::Activity, entry::Entry, entry_view::EntryView},
    mutators,
    reader::Reader,
};

use sqlx::{
    SqlitePool,
    sqlite::SqlitePoolOptions,
    types::chrono::{DateTime, Utc},
};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    apply::SqliteApply,
    reader::{SqliteReader, entries_rooted_in_time_interval},
};

#[derive(Debug, Clone)]
pub struct SqliteClient {
    pub pool: SqlitePool,
    change_transmitter: broadcast::Sender<()>,
}

impl SqliteClient {
    pub async fn init(db_path: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(db_path)
            .await?;
        let (change_transmitter, _rx) = broadcast::channel::<()>(16);
        let client = SqliteClient {
            pool,
            change_transmitter,
        };
        client.run_migrations().await?;
        Ok(client)
    }

    pub fn from_pool(pool: SqlitePool) -> Self {
        let (change_transmitter, _rx) = broadcast::channel::<()>(16);
        SqliteClient {
            pool: pool,
            change_transmitter,
        }
    }

    /// Run migrations on the database. Safe to call multiple times - sqlx tracks which migrations
    /// have already been applied.
    async fn run_migrations(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| gv_core::error::DomainError::Other(e.to_string()))
    }

    pub async fn run_action(&self, action: Action) -> Result<()> {
        // Begin Sqlite transaction.
        let mut tx = self.pool.begin().await?;

        // Create mutation.
        let mx = match action {
            Action::CreateActivity(action) => {
                mutators::create_activity::<sqlx::Sqlite, SqliteReader>(&mut tx, action).await?
            }
            Action::CreateUser(action) => {
                mutators::create_user::<sqlx::Sqlite, SqliteReader>(&mut tx, action).await?
            }
            Action::CreateEntry(action) => {
                mutators::create_entry::<sqlx::Sqlite, SqliteReader>(&mut tx, action).await?
            }
            Action::MoveEntry(action) => {
                mutators::move_entry::<sqlx::Sqlite, SqliteReader>(&mut tx, action).await?
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

        // Broadcast notification that the database changed.
        let _ = self.change_transmitter.send(());

        // TODO: send mutation to service (or add to a pending_mutations queue).
        // sync_service.append_applied_mutation(mx);

        Ok(())
    }

    // TODO: move out of top-level, try generalizing to stream(query: <Fn...>) -> impl Stream<...>.
    // Perhaps a macro? #[stream]
    pub fn stream_activities(&self) -> impl Stream<Item = Result<Vec<Activity>>> + use<> {
        let pool = self.pool.clone();
        let mut change_rx = self.change_transmitter.subscribe();

        async_stream::stream! {
            yield SqliteReader::all_activities(&pool).await;

            while let Ok(()) = change_rx.recv().await {
                yield SqliteReader::all_activities(&pool).await;
            }
        }
    }

    pub fn stream_entries(&self) -> impl Stream<Item = Result<Vec<Entry>>> + use<> {
        let pool = self.pool.clone();
        let mut change_rx = self.change_transmitter.subscribe();

        async_stream::stream! {
            yield SqliteReader::all_entries(&pool).await;

            while let Ok(()) = change_rx.recv().await {
                yield SqliteReader::all_entries(&pool).await;
            }
        }
    }

    pub fn stream_entries_rooted_in_time_interval(
        &self,
        min: DateTime<Utc>,
        max: DateTime<Utc>,
    ) -> impl Stream<Item = Result<Vec<Entry>>> + use<> {
        let pool = self.pool.clone();
        let mut change_rx = self.change_transmitter.subscribe();

        async_stream::stream! {
            yield entries_rooted_in_time_interval(&pool, min, max).await;

            while let Ok(()) = change_rx.recv().await {
                yield entries_rooted_in_time_interval(&pool, min, max).await;
            }
        }
    }

    pub fn stream_entry_view_by_id(
        &self,
        id: Uuid,
    ) -> impl Stream<Item = Result<EntryView>> + use<> {
        use gv_core::error::DomainError;

        let pool = self.pool.clone();
        let mut change_rx = self.change_transmitter.subscribe();

        async_stream::stream! {
            yield SqliteReader::find_entry_view_by_id(&pool, id)
                .await
                .and_then(|opt| opt.ok_or_else(|| DomainError::Other(format!("Entry not found: {}", id))));

            while let Ok(()) = change_rx.recv().await {
                yield SqliteReader::find_entry_view_by_id(&pool, id)
                    .await
                    .and_then(|opt| opt.ok_or_else(|| DomainError::Other(format!("Entry not found: {}", id))));
            }
        }
    }
}

pub mod tests {
    pub use super::*;
    pub use gv_core::{SYSTEM_ACTOR_ID, actions::CreateActivity, models::activity::ActivityName};
    pub use uuid::Uuid;

    #[sqlx::test(migrations = "./migrations")]
    fn test_create_activity(pool: SqlitePool) {
        let sqlite_client = SqliteClient::from_pool(pool);

        let id = Uuid::new_v4();
        let activity = Activity {
            id: id.clone(),
            owner_id: SYSTEM_ACTOR_ID,
            name: ActivityName::parse("test".to_string()).unwrap(),
            description: None,
            source_activity_id: None,
        };
        let create_activity: CreateActivity = activity.into();
        let action: Action = create_activity.into();

        sqlite_client.run_action(action).await.unwrap();

        let queried_activity = SqliteReader::find_activity_by_id(&sqlite_client.pool, id)
            .await
            .unwrap();

        assert!(queried_activity.is_some());
    }
}

use std::{sync::Arc, time::Duration};

use futures_core::Stream;
use gv_core::error::DomainError;
use gv_core::{
    actions::{Action, CreateActivity},
    error::Result,
    models::{
        activity::{Activity, ActivityName},
        attribute::Attribute,
        entry::Entry,
        entry_join::EntryJoin,
    },
    mutators,
    queries::{
        AllActivities, AllAttributes, AllEntries, AnyQuery, AnyQueryResponse,
        EntriesRootedInTimeInterval, FindActivityById, FindEntryJoinById, Query,
    },
    query_executor::QueryExecutor,
};
use sqlx::{
    SqlitePool,
    sqlite::SqlitePoolOptions,
    types::chrono::{DateTime, Utc},
};
use tokio::sync::broadcast;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use crate::{
    apply::SqliteApply,
    query_store::{QueryStore, QuerySubscription},
    sqlite_executor::SqliteQueryExecutor,
};

#[derive(Debug, Clone)]
pub struct SqliteClient {
    pub pool: SqlitePool,
    change_transmitter: broadcast::Sender<()>,
    cache_ready_transmitter: broadcast::Sender<()>,
    query_store: QueryStore,
}

impl SqliteClient {
    pub async fn init(db_path: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(20)
            .connect(db_path)
            .await?;
        let client = Self::from_pool(pool);
        client.run_migrations().await?;
        Ok(client)
    }

    pub fn from_pool(pool: SqlitePool) -> Self {
        let (change_transmitter, _rx) = broadcast::channel::<()>(16);
        let (cache_ready_transmitter, _rx2) = broadcast::channel::<()>(16);
        let cache_ready_tx = cache_ready_transmitter.clone();
        let query_store = QueryStore::new(
            pool.clone(),
            change_transmitter.clone(),
            Arc::new(move || {
                let _ = cache_ready_tx.send(());
            }),
        );
        SqliteClient {
            pool,
            change_transmitter,
            cache_ready_transmitter,
            query_store,
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

    #[instrument(skip_all)]
    pub async fn run_action(&self, action: Action) -> Result<()> {
        debug!("Began running action = {:?}", action);
        debug!(
            "Active broadcast receivers: {}",
            self.change_transmitter.receiver_count()
        );

        // Begin Sqlite transaction.
        let mut tx = self.pool.begin().await?;
        let mut executor = SqliteQueryExecutor::new(&mut tx);

        // Create mutation.
        let mx = match action {
            Action::CreateActivity(action) => {
                mutators::create_activity(&mut executor, action).await?
            }
            Action::CreateUser(action) => mutators::create_user(&mut executor, action).await?,
            Action::CreateEntry(action) => mutators::create_entry(&mut executor, action).await?,
            Action::MoveEntry(action) => mutators::move_entry(&mut executor, action).await?,
            Action::DeleteEntryRecursive(action) => {
                mutators::delete_entry_recursive(&mut executor, action).await?
            }
            Action::CreateAttribute(action) => {
                mutators::create_attribute(&mut executor, action).await?
            }
            Action::CreateValue(action) => mutators::create_value(&mut executor, action).await?,
            Action::UpdateEntryCompletion(action) => {
                mutators::update_entry_completion(&mut executor, action).await?
            }
            Action::UpdateAttributeValue(action) => {
                mutators::update_attribute_value(&mut executor, action).await?
            }
        };

        // TODO: write this mutation into the local mutation log.
        // sync_service.log_mutation(mx);

        // Defer FK constraint checking until commit so delta order doesn't matter.
        sqlx::query("PRAGMA defer_foreign_keys = ON")
            .execute(&mut *tx)
            .await?;

        info!("delta count = {}", mx.changes.len());
        // Apply deltas.
        for delta in mx.changes {
            delta.apply_delta(&mut tx).await?;
        }

        // Commit the transaction.
        tx.commit().await?;

        // Broadcast notification that the database changed.
        let _ = self.change_transmitter.send(());
        debug!("Broadcast sent, returning from run_action");

        // TODO: send mutation to service (or to a pending_mutations queue).
        // sync_service.append_applied_mutation(mx);

        Ok(())
    }

    pub async fn run_query<Q: Query>(&self, query: Q) -> Result<Q::Response>
    where
        for<'c> SqliteQueryExecutor<'c>: QueryExecutor<Q>,
    {
        self.query_store.run_query(query).await
    }

    pub async fn run_any_query(&self, query: AnyQuery) -> Result<AnyQueryResponse> {
        self.query_store.run_any_query(query).await
    }

    pub async fn subscribe_query(&self, query: AnyQuery) -> Result<Arc<QuerySubscription>> {
        self.query_store.subscribe_query(query).await
    }

    pub fn read_cached_query(&self, query: AnyQuery) -> Option<AnyQueryResponse> {
        self.query_store.read_cached_query(query)
    }

    /// Subscribe to cache-ready notifications. Fires after each database change
    /// has been propagated through all subscribed queries.
    pub fn subscribe_cache_ready(&self) -> broadcast::Receiver<()> {
        self.cache_ready_transmitter.subscribe()
    }

    /// Spawn a background task that creates a new activity every 10 seconds.
    /// Cache refresh and notifications happen automatically via the change broadcast.
    pub fn start_background_ticker(&self, actor_id: Uuid) {
        let client = self.clone();
        let _ = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            interval.tick().await; // skip the immediate first tick
            let mut counter = 0u64;
            loop {
                interval.tick().await;
                counter += 1;
                let activity = Activity {
                    id: Uuid::new_v4(),
                    owner_id: actor_id,
                    name: ActivityName::parse(format!("Auto Activity {counter}"))
                        .unwrap_or_else(|_| ActivityName::parse("Auto".to_string()).unwrap()),
                    description: Some(format!("Created by background ticker (tick #{counter})")),
                    source_activity_id: None,
                };
                let action: Action = CreateActivity { actor_id, activity }.into();
                let _ = client.run_action(action).await;
            }
        });
    }

    // TODO: move out of top-level, try generalizing to stream(query: <Fn...>) -> impl Stream<...>.
    // Perhaps a macro? #[stream]
    pub fn stream_activities(&self) -> impl Stream<Item = Result<Vec<Activity>>> + use<> {
        let pool = self.pool.clone();
        let mut change_rx = self.change_transmitter.subscribe();

        async_stream::stream! {
            let initial = async {
                let mut connection = pool.acquire().await?;
                SqliteQueryExecutor::new(&mut *connection).execute(AllActivities {}).await
            }
            .await;
            yield initial;

            while let Ok(()) = change_rx.recv().await {
                let next = async {
                    let mut connection = pool.acquire().await?;
                    SqliteQueryExecutor::new(&mut *connection).execute(AllActivities {}).await
                }
                .await;
                yield next;
            }
        }
    }

    pub fn stream_attributes(&self) -> impl Stream<Item = Result<Vec<Attribute>>> + use<> {
        let pool = self.pool.clone();
        let mut change_rx = self.change_transmitter.subscribe();

        async_stream::stream! {
            let initial = async {
                let mut connection = pool.acquire().await?;
                SqliteQueryExecutor::new(&mut *connection).execute(AllAttributes {}).await
            }
            .await;
            yield initial;

            while let Ok(()) = change_rx.recv().await {
                let next = async {
                    let mut connection = pool.acquire().await?;
                    SqliteQueryExecutor::new(&mut *connection).execute(AllAttributes {}).await
                }
                .await;
                yield next;
            }
        }
    }

    pub fn stream_entries(&self) -> impl Stream<Item = Result<Vec<Entry>>> + use<> {
        let pool = self.pool.clone();
        let mut change_rx = self.change_transmitter.subscribe();

        async_stream::stream! {
            let initial = async {
                let mut connection = pool.acquire().await?;
                SqliteQueryExecutor::new(&mut *connection).execute(AllEntries {}).await
            }
            .await;
            yield initial;

            while let Ok(()) = change_rx.recv().await {
                let next = async {
                    let mut connection = pool.acquire().await?;
                    SqliteQueryExecutor::new(&mut *connection).execute(AllEntries {}).await
                }
                .await;
                yield next;
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
            let initial = async {
                let mut connection = pool.acquire().await?;
                SqliteQueryExecutor::new(&mut *connection).execute(EntriesRootedInTimeInterval { from: min, to: max }).await
            }
            .await;
            yield initial;

            while let Ok(()) = change_rx.recv().await {
                let next = async {
                    let mut connection = pool.acquire().await?;
                    SqliteQueryExecutor::new(&mut *connection).execute(EntriesRootedInTimeInterval { from: min, to: max }).await
                }
                .await;
                yield next;
            }
        }
    }

    pub fn stream_entry_join_by_id(
        &self,
        id: Uuid,
    ) -> impl Stream<Item = Result<EntryJoin>> + use<> {
        let pool = self.pool.clone();
        let mut change_rx = self.change_transmitter.subscribe();

        async_stream::stream! {
            let initial = async {
                let mut connection = pool.acquire().await?;
                SqliteQueryExecutor::new(&mut *connection).execute(FindEntryJoinById { entry_id: id })
                    .await
                    .and_then(|opt| opt.ok_or_else(|| DomainError::Other(format!("Entry not found: {}", id))))
            }
            .await;
            yield initial;

            while let Ok(()) = change_rx.recv().await {
                let next = async {
                    let mut connection = pool.acquire().await?;
                    SqliteQueryExecutor::new(&mut *connection).execute(FindEntryJoinById { entry_id: id })
                        .await
                        .and_then(|opt| opt.ok_or_else(|| DomainError::Other(format!("Entry not found: {}", id))))
                }
                .await;
                yield next;
            }
        }
    }
}

pub mod tests {
    pub use super::*;
    pub use gv_core::SYSTEM_ACTOR_ID;
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

        let queried_activity = {
            let mut connection = sqlite_client.pool.acquire().await.unwrap();
            SqliteQueryExecutor::new(&mut *connection)
                .execute(FindActivityById { id })
                .await
                .unwrap()
        };

        assert!(queried_activity.is_some());
    }
}

use gv_core::delta_executor::AnyDeltaExecutor;
use gv_core::error::DbErr;
use gv_core::io::{Io, SystemIo};
use gv_core::{
    DEFAULT_USER_ID,
    actions::{Action, CreateActivity, CreateAttribute, CreateUser, CreateValue},
    error::Result,
    models::{
        activity::{Activity, ActivityName},
        user::User,
    },
    mutators,
    queries::{
        DomainQuery, DomainQueryResponse, FindActivityById, FindAttributeById, FindUserById, Query,
    },
    query_executor::QueryExecutor,
    std_lib::StandardLibrary,
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use gv_sql::sqlite::{SqliteDeltaExecutor, SqliteQueryExecutor};

use crate::live::LiveStore;
use crate::query_store::{QueryStore, QuerySubscription};

#[derive(Debug, Clone)]
pub struct SqliteClient {
    pub pool: SqlitePool,
    change_transmitter: broadcast::Sender<()>,
    cache_ready_transmitter: broadcast::Sender<()>,
    query_store: QueryStore,
    live: LiveStore,
    io: Arc<dyn Io>,
}

impl SqliteClient {
    pub async fn init(db_path: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(20)
            .connect(db_path)
            .await
            .db_err()?;
        let client = Self::from_pool(pool, Arc::new(SystemIo::default()));
        client.run_migrations().await?;
        client.seed_default_user().await?;
        client.seed_std_lib().await?;
        Ok(client)
    }

    /// Seed the single default user into a fresh database. Idempotent: a no-op if
    /// the user already exists, so it's safe to call on every launch alongside
    /// migrations. This replaces the system actor that migrations used to insert;
    /// a temporary stand-in until an auth module lands (see [`DEFAULT_USER_ID`]).
    async fn seed_default_user(&self) -> Result<()> {
        let existing = {
            let mut conn = self.pool.acquire().await.db_err()?;
            SqliteQueryExecutor::new(&mut *conn)
                .execute(FindUserById {
                    actor_id: DEFAULT_USER_ID,
                })
                .await?
        };
        if existing.is_none() {
            let action: Action = CreateUser::from(User::default_user()).into();
            self.run_action(action).await?;
        }
        Ok(())
    }

    /// Seed the standard-library attributes and activities into a fresh database.
    /// Idempotent: each item is keyed by a stable id (see `gv_core::std_lib`) and
    /// created only if absent, so this is safe to run on every launch alongside
    /// migrations. It does not reconcile edits — an item whose definition changed
    /// in code is left untouched, and one the user deleted is re-created.
    async fn seed_std_lib(&self) -> Result<()> {
        for attribute in StandardLibrary::attributes() {
            let exists = {
                let mut conn = self.pool.acquire().await.db_err()?;
                SqliteQueryExecutor::new(&mut *conn)
                    .execute(FindAttributeById {
                        attribute_id: attribute.id,
                    })
                    .await?
            };
            if exists.is_none() {
                self.run_action(CreateAttribute::from(attribute).into())
                    .await?;
            }
        }

        for std_activity in StandardLibrary::activities() {
            let exists = {
                let mut conn = self.pool.acquire().await.db_err()?;
                SqliteQueryExecutor::new(&mut *conn)
                    .execute(FindActivityById {
                        id: std_activity.activity.id,
                    })
                    .await?
            };
            if exists.is_some() {
                continue;
            }
            let actor_id = std_activity.activity.owner_id;
            let create = CreateActivity {
                actor_id,
                activity: std_activity.activity,
                template: std_activity.template,
            };
            self.run_action(create.into()).await?;
            for value in std_activity.template_values {
                self.run_action(CreateValue { actor_id, value }.into())
                    .await?;
            }
        }
        Ok(())
    }

    pub fn from_pool(pool: SqlitePool, io: Arc<dyn Io>) -> Self {
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
        let live = LiveStore::new(pool.clone(), change_transmitter.subscribe());
        SqliteClient {
            pool,
            change_transmitter,
            cache_ready_transmitter,
            query_store,
            live,
            io,
        }
    }

    /// Run migrations on the database. Safe to call multiple times - sqlx tracks which migrations
    /// have already been applied.
    async fn run_migrations(&self) -> Result<()> {
        sqlx::migrate!("../gv-sql/sqlite/migrations")
            .run(&self.pool)
            .await
            .map_err(|e| gv_core::error::DomainError::Database(Box::new(e)))
    }

    #[instrument(skip_all)]
    pub async fn run_action(&self, action: Action) -> Result<mutators::Mutation> {
        debug!("Began running action = {:?}", action);
        debug!(
            "Active broadcast receivers: {}",
            self.change_transmitter.receiver_count()
        );

        // Begin Sqlite transaction.
        let mut tx = self.pool.begin().await.db_err()?;
        let mut executor = SqliteQueryExecutor::new(&mut tx);

        // Create mutation.
        let mx = match action {
            Action::CreateActivity(action) => {
                mutators::create_activity(&mut executor, self.io.as_ref(), action).await?
            }
            Action::CreateUser(action) => {
                mutators::create_user(&mut executor, self.io.as_ref(), action).await?
            }
            Action::CreateEntry(action) => {
                mutators::create_entry(&mut executor, self.io.as_ref(), action).await?
            }
            Action::CreateEntryFromActivity(action) => {
                mutators::create_entry_from_activity(&mut executor, self.io.as_ref(), action)
                    .await?
            }
            Action::MoveEntry(action) => {
                mutators::move_entry(&mut executor, self.io.as_ref(), action).await?
            }
            Action::DeleteEntryRecursive(action) => {
                mutators::delete_entry_recursive(&mut executor, self.io.as_ref(), action).await?
            }
            Action::CreateAttribute(action) => {
                mutators::create_attribute(&mut executor, self.io.as_ref(), action).await?
            }
            Action::CreateValue(action) => {
                mutators::create_value(&mut executor, self.io.as_ref(), action).await?
            }
            Action::AttachValue(action) => {
                mutators::attach_value(&mut executor, self.io.as_ref(), action).await?
            }
            Action::DeleteAttributeValue(action) => {
                mutators::delete_attribute_value(&mut executor, self.io.as_ref(), action).await?
            }
            Action::UpdateEntryCompletion(action) => {
                mutators::update_entry_completion(&mut executor, self.io.as_ref(), action).await?
            }
            Action::UpdateAttributeValue(action) => {
                mutators::update_attribute_value(&mut executor, self.io.as_ref(), action).await?
            }
            Action::UpdateAttribute(action) => {
                mutators::update_attribute(&mut executor, self.io.as_ref(), action).await?
            }
            Action::UpdateEntry(action) => {
                mutators::update_entry(&mut executor, self.io.as_ref(), action).await?
            }
            Action::ConvertToSets(action) => {
                mutators::convert_to_sets(&mut executor, self.io.as_ref(), action).await?
            }
            Action::DuplicateEntry(action) => {
                mutators::duplicate_entry(&mut executor, self.io.as_ref(), action).await?
            }
        };

        // TODO: write this mutation into the local mutation log.
        // sync_service.log_mutation(mx);

        // Defer FK constraint checking until commit so delta order doesn't matter.
        sqlx::query("PRAGMA defer_foreign_keys = ON")
            .execute(&mut *tx)
            .await
            .db_err()?;

        info!("delta count = {}", mx.changes.len());
        // Apply deltas.
        // for delta in mx.changes {
        //     delta.apply_delta(&mut tx).await?;
        // }
        let mut delta_executor = SqliteDeltaExecutor::new(&mut *tx);
        for delta in mx.changes.iter().cloned() {
            delta_executor.apply_any_delta(delta).await?;
        }
        // Commit the transaction.
        tx.commit().await.db_err()?;

        // Broadcast notification that the database changed.
        let _ = self.change_transmitter.send(());
        debug!("Broadcast sent, returning from run_action");

        // TODO: send mutation to service (or to a pending_mutations queue).
        // sync_service.append_applied_mutation(mx);

        Ok(mx)
    }

    pub async fn run_query<Q: Query>(&self, query: Q) -> Result<Q::Response>
    where
        for<'c> SqliteQueryExecutor<'c>: QueryExecutor<Q>,
    {
        self.query_store.run_query(query).await
    }

    pub async fn run_domain_query(&self, query: DomainQuery) -> Result<DomainQueryResponse> {
        self.query_store.run_domain_query(query).await
    }

    pub async fn subscribe_query(&self, query: DomainQuery) -> Result<Arc<QuerySubscription>> {
        self.query_store.subscribe_query(query).await
    }

    pub fn read_cached_query(&self, query: DomainQuery) -> Option<DomainQueryResponse> {
        self.query_store.read_cached_query(query)
    }

    /// Live queries: subscriptions with ids and one consistent batch per data version. The
    /// successor to `subscribe_query` + `subscribe_cache_ready` (which stay for unmigrated UI).
    pub fn live(&self) -> &LiveStore {
        &self.live
    }

    /// Subscribe to cache-ready notifications. Fires after each database change
    /// has been propagated through all subscribed queries.
    ///
    /// FFI boundary contract: write-then-notify. On every change the client
    /// refreshes every subscribed query in the cache, *then* fires this signal.
    /// Consumers (Swift, etc.) should treat the notification as "the latest
    /// values are already in the cache — read at your leisure". Don't pass
    /// payloads on this channel; readers fan out to whichever cached queries
    /// they care about.
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
                let create_activity = activity.into_create_activity(client.io.uuid());
                let _ = client.run_action(create_activity.into()).await;
            }
        });
    }
}

pub mod tests {
    pub use super::*;
    pub use gv_core::SYSTEM_ACTOR_ID;
    pub use gv_core::queries::{DomainQuery, FindActivityById, FindEntryJoinById, SnapshotAll};
    pub use uuid::Uuid;

    /// Two subscriptions to the same query key share one cache entry; the entry
    /// must survive until the *last* handle drops. Regression for a refcount bug
    /// where the first drop evicted the key, starving the other subscriber's
    /// refreshes (stale reads until restart).
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    fn test_query_subscription_refcount(pool: SqlitePool) {
        let client = SqliteClient::from_pool(pool, Arc::new(SystemIo::default()));
        let query = DomainQuery::FindEntryJoinById(FindEntryJoinById {
            entry_id: Uuid::new_v4(),
        });

        let sub1 = client.subscribe_query(query.clone()).await.unwrap();
        let sub2 = client.subscribe_query(query.clone()).await.unwrap();

        // Dropping one of two subscribers must NOT evict the shared key.
        drop(sub2);
        assert!(
            client.read_cached_query(query.clone()).is_some(),
            "cache key must survive while another subscriber is alive"
        );

        // Dropping the last subscriber evicts it.
        drop(sub1);
        assert!(
            client.read_cached_query(query).is_none(),
            "cache key must be evicted once the last subscriber drops"
        );
    }

    /// Std-lib seeding runs on every launch, so it must converge rather than
    /// duplicate. This also exercises the seed path itself — template-tree
    /// validation and value-on-template attachment — which the `from_pool`
    /// tests bypass. A second pass over an already-seeded DB must be a no-op.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    fn test_seed_std_lib_is_idempotent(pool: SqlitePool) {
        let client = SqliteClient::from_pool(pool, Arc::new(SystemIo::default()));
        client.seed_default_user().await.unwrap();

        client.seed_std_lib().await.unwrap();
        let first = client.run_query(SnapshotAll).await.unwrap();

        // The first pass seeds every std-lib item exactly once.
        assert_eq!(first.attributes.len(), StandardLibrary::attributes().len());
        assert_eq!(first.activities.len(), StandardLibrary::activities().len());
        assert!(
            !first.attributes.is_empty() && !first.activities.is_empty(),
            "seeding should have created std-lib items"
        );

        // A second pass over the same DB converges instead of duplicating.
        client.seed_std_lib().await.unwrap();
        let second = client.run_query(SnapshotAll).await.unwrap();
        assert_eq!(first.attributes.len(), second.attributes.len());
        assert_eq!(first.activities.len(), second.activities.len());
        assert_eq!(first.entries.len(), second.entries.len());
        assert_eq!(first.values.len(), second.values.len());
    }

    #[cfg(test)]
    fn lag_test_entry() -> gv_core::models::entry::Entry {
        gv_core::models::entry::Entry {
            id: Uuid::new_v4(),
            activity_id: None,
            owner_id: gv_core::DEFAULT_USER_ID,
            name: None,
            position: None,
            is_template: false,
            display_as_sets: false,
            is_sequence: false,
            is_complete: false,
            temporal: gv_core::models::entry::Temporal::None,
        }
    }

    /// Overflow the change broadcast (capacity 16) so every listener is `Lagged`.
    #[cfg(test)]
    fn flood_change_channel(client: &SqliteClient) {
        for _ in 0..100 {
            let _ = client.change_transmitter.send(());
        }
    }

    /// Regression: the refresh loop was `while let Ok(()) = rx.recv().await`, which exits on
    /// `Err(Lagged)` — after a burst of writes the cache silently stopped refreshing until restart.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn test_query_store_survives_lagged_change_channel(pool: SqlitePool) {
        let client = SqliteClient::from_pool(pool, Arc::new(SystemIo::default()));
        client.seed_default_user().await.unwrap();
        let entry = lag_test_entry();
        let query =
            DomainQuery::FindEntryById(gv_core::queries::FindEntryById { entry_id: entry.id });
        let _sub = client.subscribe_query(query.clone()).await.unwrap();

        flood_change_channel(&client);
        client
            .run_action(gv_core::actions::CreateEntry::from(entry.clone()).into())
            .await
            .unwrap();

        let refreshed = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(DomainQueryResponse::FindEntryById(Some(_))) =
                    client.read_cached_query(query.clone())
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(refreshed.is_ok(), "cache must keep refreshing after Lagged");
    }

    /// Same regression for the live store's refresh loop.
    #[sqlx::test(migrations = "../gv-sql/sqlite/migrations")]
    async fn test_live_store_survives_lagged_change_channel(pool: SqlitePool) {
        use crate::live::LiveValue;
        use crate::queries::ClientQueryResponse;
        let client = SqliteClient::from_pool(pool, Arc::new(SystemIo::default()));
        client.seed_default_user().await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        client.live().set_batch_sink(Arc::new(move |batch| {
            let _ = tx.send(batch);
        }));
        let entry = lag_test_entry();
        let sub = client
            .live()
            .subscribe(gv_core::queries::FindEntryById { entry_id: entry.id }.into())
            .await
            .unwrap();
        // Settle: no pending rebuild request may mask the lag (subscribe can request one).
        client.live().sync().await;

        flood_change_channel(&client);
        client
            .run_action(gv_core::actions::CreateEntry::from(entry.clone()).into())
            .await
            .unwrap();

        let batch = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("live store must keep publishing after Lagged")
            .unwrap();
        assert_eq!(batch.updates[0].subscription_id, sub.subscription_id);
        assert!(matches!(
            &batch.updates[0].value,
            LiveValue::Answer(ClientQueryResponse::FindEntryById(Some(e))) if e.id == entry.id
        ));
    }
}

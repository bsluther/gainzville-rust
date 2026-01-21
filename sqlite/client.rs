use futures_core::Stream;
use gv_core::{
    actions::Action,
    error::Result,
    models::activity::Activity,
    repos::{ActivityRepo, ActivityRepo2},
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tokio::sync::broadcast;

use crate::{
    controller::SqliteController,
    repos::{SqliteContext, SqliteRepo2},
};

#[derive(Debug, Clone)]
pub struct Client {
    pub controller: SqliteController,
    change_sender: broadcast::Sender<()>,
}

impl Client {
    pub async fn init(db_path: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(db_path)
            .await?;
        let (change_sender, _rx) = broadcast::channel::<()>(16);
        let client = Client {
            controller: SqliteController { pool },
            change_sender,
        };
        Self::run_migrations(&client.controller).await?;
        Ok(client)
    }

    pub async fn run_action(&self, action: Action) -> Result<()> {
        let tx = self.controller.run_action(action).await?;
        tx.commit().await?;
        let _ = self.change_sender.send(());
        Ok(())
    }
    // YOU ARE HERE: use this!
    pub async fn run_action2(&self, action: Action) -> Result<()> {
        let mut tx = self.controller.pool.begin().await?;
        self.controller.compute_mutation2(&mut tx, action).await?;
        tx.commit().await?;
        let _ = self.change_sender.send(());
        Ok(())
    }

    /// Run migrations on the database. Safe to call multiple times -
    /// sqlx tracks which migrations have already been applied.
    async fn run_migrations(controller: &SqliteController) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&controller.pool)
            .await
            .map_err(|e| gv_core::error::DomainError::Other(e.to_string()))
    }

    pub fn stream_activities(&self) -> impl Stream<Item = Result<Vec<Activity>>> + use<> {
        let pool = self.controller.pool.clone();
        let mut change_rx = self.change_sender.subscribe();

        async_stream::stream! {
            yield Self::query_all_activities(&pool).await;

            while let Ok(()) = change_rx.recv().await {
                yield Self::query_all_activities(&pool).await;
            }
        }
    }
    pub fn stream_activities2(&self) -> impl Stream<Item = Result<Vec<Activity>>> + use<> {
        let pool = self.controller.pool.clone();
        let mut change_rx = self.change_sender.subscribe();

        async_stream::stream! {
            let mut repo = SqliteRepo2 {};
            yield repo.all_activities(&pool).await;

            while let Ok(()) = change_rx.recv().await {
                yield repo.all_activities(&pool).await;
            }
        }
    }

    async fn query_all_activities(pool: &SqlitePool) -> Result<Vec<Activity>> {
        let mut tx = pool.begin().await?;
        let mut repo = SqliteContext::new(&mut tx);
        let activities = repo.all_activities().await?;
        Ok(activities)
    }
}

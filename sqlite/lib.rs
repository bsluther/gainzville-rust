pub mod apply;
// pub mod client;
// pub mod controller;
// pub mod repos;
pub mod sandbox;
// use controller::SqliteController;
// use sqlx::sqlite::SqlitePoolOptions;

// pub async fn init(db_path: &str) -> Result<SqliteController, sqlx::Error> {
//     let pool = SqlitePoolOptions::new()
//         .max_connections(5)
//         .connect(db_path)
//         .await?;

//     Ok(SqliteController { pool })
// }

// /// Run migrations on the database. Safe to call multiple times -
// /// sqlx tracks which migrations have already been applied.
// pub async fn run_migrations(
//     controller: &SqliteController,
// ) -> Result<(), sqlx::migrate::MigrateError> {
//     sqlx::migrate!("./migrations").run(&controller.pool).await
// }

use sqlx::PgPool;

pub async fn setup_test_pool() -> PgPool {
    let _ = dotenvy::dotenv();
    let database_url =
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL not found in env");

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

async fn clear_database(pool: &PgPool) -> Result<(), sqlx::Error> {
    // Truncate all tables with CASCADE to handle FK constraints robustly
    // This works even with circular references
    sqlx::query!(
        r#"
        TRUNCATE TABLE actors, users, activities, entries
        RESTART IDENTITY CASCADE
        "#
    )
    .execute(pool)
    .await?;

    // Re-insert system actor
    sqlx::query!(
        r#"
        INSERT INTO actors (id, actor_kind)
        VALUES ('eee9e6ae-6531-4580-8356-427604a0dc02', 'system')
        "#
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn setup_clear_database() -> PgPool {
    let pool = setup_test_pool().await;
    clear_database(&pool)
        .await
        .expect("Failed to clear test database");
    pool
}

use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::Config;

pub async fn connect(cfg: &Config) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&cfg.database_url)
        .await
        .context("connecting to DATABASE_URL")?;
    Ok(pool)
}

/// Migrations are embedded at compile time. A binary older than the migrations
/// tree therefore ships an older schema silently, which is why deploy.sh
/// refuses to install a binary older than `migrations/`.
pub async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("running migrations")?;
    Ok(())
}

pub async fn applied_version(pool: &PgPool) -> Result<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT max(version) FROM _sqlx_migrations")
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

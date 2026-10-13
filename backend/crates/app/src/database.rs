//! Пулы подключений и миграции (design D3, роли базы — D15).

use std::time::Duration;

use anyhow::Context as _;
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::Config;

/// Миграции, встроенные в бинарь.
pub(crate) static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Применяет миграции от имени роли-владельца.
///
/// cancel-safe: yes — `Migrator` применяет каждую миграцию в транзакции.
pub(crate) async fn migrate(config: &Config) -> anyhow::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(config.database.migrations.expose())
        .await
        .context("подключение роли-владельца")?;
    MIGRATOR.run(&pool).await.context("применение миграций")?;
    pool.close().await;
    Ok(())
}

/// Пул приложения (роль только с DML).
///
/// cancel-safe: yes — только установка соединений.
pub(crate) async fn connect(config: &Config) -> anyhow::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(32)
        .acquire_timeout(Duration::from_secs(5))
        .connect(config.database.app.expose())
        .await
        .context("подключение роли приложения")
}

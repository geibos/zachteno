//! Пулы подключений и миграции (design D3, роли базы — D15).

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context as _, bail};
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::Config;
use crate::secret::Secret;

#[cfg(test)]
mod tests;

/// Миграции, встроенные в бинарь.
pub(crate) static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Применяет миграции от имени роли-владельца.
///
/// cancel-safe: yes — `Migrator` применяет каждую миграцию в транзакции.
pub(crate) async fn migrate(owner_url: &Secret<String>) -> anyhow::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(owner_url.expose())
        .await
        .context("подключение роли-владельца")?;
    MIGRATOR.run(&pool).await.context("применение миграций")?;
    pool.close().await;
    Ok(())
}

/// Готовит схему к работе: с ролью-владельцем (dev) применяет миграции, без неё (production,
/// миграции уже применил разовый `migrate`) — только проверяет, что применены все.
///
/// cancel-safe: yes — миграции транзакционны, проверка только читает.
pub(crate) async fn prepare(config: &Config, pool: &PgPool) -> anyhow::Result<()> {
    match &config.database.migrations {
        Some(owner_url) => migrate(owner_url).await,
        None => ensure_applied(pool).await,
    }
}

/// Подсказка оператору, если схема отстаёт от бинаря.
const RUN_MIGRATE: &str =
    "выполните `migrate` с MIGRATION_DATABASE_URL (в compose — `docker compose run --rm migrate`)";

/// Версия применённой миграции и её контрольная сумма.
type Applied = (i64, Vec<u8>);
/// Контрольные суммы применённых миграций по версиям.
type Checksums = HashMap<i64, Vec<u8>>;

/// Ошибка, если в базе применены не все миграции бинаря или применённая отличается от встроенной.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn ensure_applied(pool: &PgPool) -> anyhow::Result<()> {
    let rows: Vec<Applied> =
        sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations WHERE success")
            .fetch_all(pool)
            .await
            .with_context(|| format!("схема базы не создана: {RUN_MIGRATE}"))?;
    compare_with_binary(&rows.into_iter().collect())
}

/// Сверяет применённые миграции со встроенными в бинарь.
fn compare_with_binary(applied: &Checksums) -> anyhow::Result<()> {
    let mut missing = Vec::new();
    for migration in
        MIGRATOR.iter().filter(|migration| !migration.migration_type.is_down_migration())
    {
        match applied.get(&migration.version) {
            None => missing.push(migration.version),
            Some(checksum) if checksum.as_slice() != &*migration.checksum => {
                bail!("миграция {} в базе отличается от встроенной в бинарь", migration.version)
            }
            Some(_) => {}
        }
    }
    if !missing.is_empty() {
        bail!("не применены миграции {missing:?}: {RUN_MIGRATE}");
    }
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

//! Проверки состояния (spec platform-operations, «Наблюдаемость»).

use std::time::Duration as StdDuration;

use axum::extract::State;
use axum::http::StatusCode;
use object_store::path::Path as StorePath;
use tokio::time::timeout;

use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Сколько ждать ответа каждой зависимости в проверке готовности.
const CHECK_TIMEOUT: StdDuration = StdDuration::from_secs(3);
/// Префикс, по которому проверка готовности перечисляет объекты (там ничего не лежит).
const PROBE_PREFIX: &str = "health-check";

/// Процесс жив.
pub(crate) async fn live() -> &'static str {
    "ok"
}

/// Готов обслуживать запросы: отвечают база и объектное хранилище.
pub(crate) async fn ready(State(state): State<AppState>) -> (StatusCode, &'static str) {
    match dependencies_ready(&state).await {
        Ok(()) => (StatusCode::OK, "ok"),
        Err(failed) => (StatusCode::SERVICE_UNAVAILABLE, failed),
    }
}

/// Проверяет зависимости по очереди; ошибка — текст ответа о первой недоступной.
///
/// cancel-safe: yes — только чтение.
async fn dependencies_ready(state: &AppState) -> Result<(), &'static str> {
    database_ready(state).await.map_err(|error| unavailable("database unavailable", &error))?;
    storage_ready(state).await.map_err(|error| unavailable("storage unavailable", &error))
}

/// Пишет причину в журнал и возвращает текст ответа.
fn unavailable(failed: &'static str, error: &anyhow::Error) -> &'static str {
    tracing::error!(%error, failed, "проверка готовности не прошла");
    failed
}

/// База выполняет запрос.
///
/// cancel-safe: yes — только чтение.
async fn database_ready(state: &AppState) -> anyhow::Result<()> {
    let _row = timeout(CHECK_TIMEOUT, sqlx::query("SELECT 1").execute(&state.db)).await??;
    Ok(())
}

/// Хранилище перечисляет объекты бакета: проверяются адрес, ключ доступа и сам бакет.
///
/// cancel-safe: yes — только чтение.
async fn storage_ready(state: &AppState) -> anyhow::Result<()> {
    let prefix = StorePath::from(PROBE_PREFIX);
    let _listing = timeout(CHECK_TIMEOUT, state.blobs.list_with_delimiter(Some(&prefix))).await??;
    Ok(())
}

use std::fmt::Write as _;

use axum::Router;
use axum::routing::get;
use sqlx::FromRow;
use time::OffsetDateTime;

use crate::jobs::MAX_ATTEMPTS;

#[derive(FromRow)]
struct OpsStatus {
    name: String,
    last_success_at: Option<OffsetDateTime>,
    last_failure_at: Option<OffsetDateTime>,
}

/// Метрики Prometheus: бэкапы, проверка восстановления, очередь задач.
pub(crate) async fn metrics(State(state): State<AppState>) -> (StatusCode, String) {
    match render_metrics(&state).await {
        Ok(text) => (StatusCode::OK, text),
        Err(error) => {
            tracing::error!(%error, "метрики недоступны");
            (StatusCode::SERVICE_UNAVAILABLE, "# database unavailable\nzachteno_up 0\n".to_owned())
        }
    }
}

async fn render_metrics(state: &AppState) -> Result<String, sqlx::Error> {
    let statuses: Vec<OpsStatus> =
        sqlx::query_as("SELECT name, last_success_at, last_failure_at FROM ops_status")
            .fetch_all(&state.db)
            .await?;
    let (pending, failed) = job_counts(state).await?;
    let mut text = String::from("# TYPE zachteno_up gauge\nzachteno_up 1\n");
    let _ = writeln!(text, "# TYPE zachteno_jobs_pending gauge\nzachteno_jobs_pending {pending}");
    let _ = writeln!(text, "# TYPE zachteno_jobs_failed gauge\nzachteno_jobs_failed {failed}");
    for status in statuses {
        write_status(&mut text, &status);
    }
    Ok(text)
}

/// Незавершённые задачи и те из них, что исчерпали попытки.
///
/// cancel-safe: yes — только чтение.
async fn job_counts(state: &AppState) -> Result<(i64, i64), sqlx::Error> {
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE done_at IS NULL")
        .fetch_one(&state.db)
        .await?;
    let failed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE done_at IS NULL AND attempts >= $1")
            .bind(MAX_ATTEMPTS)
            .fetch_one(&state.db)
            .await?;
    Ok((pending, failed))
}

/// Метки времени последнего успеха и последней неудачи операции.
fn write_status(text: &mut String, status: &OpsStatus) {
    let name: String = status
        .name
        .chars()
        .filter(|symbol| symbol.is_ascii_alphanumeric() || *symbol == '_')
        .collect();
    let seconds = |moment: Option<OffsetDateTime>| moment.map_or(0, OffsetDateTime::unix_timestamp);
    let _ = writeln!(
        text,
        "# TYPE zachteno_{name}_last_success_timestamp gauge\nzachteno_{name}_last_success_timestamp {}",
        seconds(status.last_success_at)
    );
    let _ = writeln!(
        text,
        "# TYPE zachteno_{name}_last_failure_timestamp gauge\nzachteno_{name}_last_failure_timestamp {}",
        seconds(status.last_failure_at)
    );
}

/// Отдельный роутер метрик — на внутреннем порту, не через Traefik.
pub(crate) fn metrics_router(state: AppState) -> Router {
    Router::new().route("/metrics", get(metrics)).with_state(state)
}

//! Проверки состояния (spec platform-operations, «Наблюдаемость»).

use axum::extract::State;
use axum::http::StatusCode;

use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Процесс жив.
pub(crate) async fn live() -> &'static str {
    "ok"
}

/// Готов обслуживать запросы: база отвечает.
pub(crate) async fn ready(State(state): State<AppState>) -> (StatusCode, &'static str) {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => (StatusCode::OK, "ok"),
        Err(error) => {
            tracing::error!(%error, "база недоступна");
            (StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
        }
    }
}

use std::fmt::Write as _;

use axum::Router;
use axum::routing::get;
use sqlx::FromRow;
use time::OffsetDateTime;

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
    let statuses: Vec<OpsStatus> = sqlx::query_as("SELECT name, last_success_at, last_failure_at FROM ops_status").fetch_all(&state.db).await?;
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE done_at IS NULL").fetch_one(&state.db).await?;
    let failed: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE done_at IS NULL AND attempts >= $1").bind(crate::jobs::MAX_ATTEMPTS).fetch_one(&state.db).await?;
    let mut text = String::from("# TYPE zachteno_up gauge\nzachteno_up 1\n");
    let _ = writeln!(text, "# TYPE zachteno_jobs_pending gauge\nzachteno_jobs_pending {pending}");
    let _ = writeln!(text, "# TYPE zachteno_jobs_failed gauge\nzachteno_jobs_failed {failed}");
    for status in statuses {
        let name: String = status.name.chars().filter(|symbol| symbol.is_ascii_alphanumeric() || *symbol == '_').collect();
        let seconds = |moment: Option<OffsetDateTime>| moment.map_or(0, OffsetDateTime::unix_timestamp);
        let _ = writeln!(text, "# TYPE zachteno_{name}_last_success_timestamp gauge\nzachteno_{name}_last_success_timestamp {}", seconds(status.last_success_at));
        let _ = writeln!(text, "# TYPE zachteno_{name}_last_failure_timestamp gauge\nzachteno_{name}_last_failure_timestamp {}", seconds(status.last_failure_at));
    }
    Ok(text)
}

/// Отдельный роутер метрик — на внутреннем порту, не через Traefik.
pub(crate) fn metrics_router(state: AppState) -> Router {
    Router::new().route("/metrics", get(metrics)).with_state(state)
}

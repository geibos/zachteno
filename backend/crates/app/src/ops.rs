//! Проверки состояния (spec platform-operations, «Наблюдаемость»).

use axum::extract::State;
use axum::http::StatusCode;

use crate::state::AppState;

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

//! Прохождение тестов (spec `test-taking`).

mod dashboard;
pub(crate) mod lifecycle;
pub(crate) mod review;
pub(crate) mod runner;
mod start;

#[cfg(test)]
mod tests;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

/// Маршруты прохождения.
pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(dashboard::dashboard))
        .routes(routes!(start::start))
        .routes(routes!(start::preview))
        .routes(routes!(runner::get_attempt))
        .routes(routes!(runner::save_answer))
        .routes(routes!(runner::navigate))
        .routes(routes!(runner::submit))
        .routes(routes!(runner::client_events))
        .routes(routes!(review::get_review))
}

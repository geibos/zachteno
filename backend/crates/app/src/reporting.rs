//! Результаты, выгрузки и статистика (spec `results-reporting`).

pub(crate) mod data;
mod details;
mod export;
mod stats;
pub(crate) mod tables;

#[cfg(test)]
mod tests;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

/// Маршруты отчётов.
pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(tables::test_results))
        .routes(routes!(tables::course_summary))
        .routes(routes!(export::test_csv))
        .routes(routes!(export::course_csv))
        .routes(routes!(stats::question_stats))
        .routes(routes!(details::attempt_details))
}

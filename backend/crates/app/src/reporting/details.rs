//! Детали попытки для преподавателя: ответы, правильные варианты, журнал (spec results-reporting, audit-log).

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::{self, CourseRole};
use crate::auth::session::CurrentUser;
use crate::authoring::model;
use crate::error::{ApiResult, AppError};
use crate::reporting::data::{self, Cell, ResultsQuery};
use crate::state::AppState;

/// Событие попытки.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct AttemptEvent {
    pub kind: String,
    pub question_id: Option<Uuid>,
    #[schema(value_type = Object)]
    pub details: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
}

/// Детали попытки.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AttemptDetails {
    pub attempt_id: Uuid,
    pub test_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub cells: Vec<Cell>,
    pub events: Vec<AttemptEvent>,
}

/// Все сданные попытки всех потоков.
const ALL_ATTEMPTS: ResultsQuery = ResultsQuery { stream_id: None, mode: data::AttemptsMode::All };

/// Ответы попытки по вопросам и хронология её событий.
#[utoipa::path(get, path = "/api/attempts/{attempt_id}/details", tag = "reports", params(("attempt_id" = Uuid, Path)),
    responses((status = 200, body = AttemptDetails)))]
pub(crate) async fn attempt_details(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(attempt_id): Path<Uuid>,
) -> ApiResult<Json<AttemptDetails>> {
    let test_id = test_of(&state, attempt_id).await?;
    let test = model::load_test(&state.db, test_id).await?;
    let _role = access::require_on(&state.db, &user, test.course_id, CourseRole::can_view).await?;
    let report = data::load(&state, test, &ALL_ATTEMPTS).await?;
    let row = report
        .rows
        .iter()
        .find(|candidate| candidate.attempt.attempt_id == attempt_id)
        .ok_or(AppError::NotFound)?;
    let events = events_of(&state, attempt_id).await?;
    Ok(Json(AttemptDetails {
        attempt_id,
        test_id,
        display_name: row.attempt.display_name.clone(),
        email: row.attempt.email.clone(),
        cells: report.cells(row),
        events,
    }))
}

/// Тест попытки (кроме предпросмотра).
///
/// cancel-safe: yes — только чтение.
async fn test_of(state: &AppState, attempt_id: Uuid) -> ApiResult<Uuid> {
    sqlx::query_scalar("SELECT test_id FROM attempts WHERE id = $1 AND NOT is_preview")
        .bind(attempt_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

/// Хронология событий попытки.
///
/// cancel-safe: yes — только чтение.
async fn events_of(state: &AppState, attempt_id: Uuid) -> ApiResult<Vec<AttemptEvent>> {
    Ok(sqlx::query_as(
        "SELECT kind, question_id, details, at FROM attempt_events WHERE attempt_id = $1 ORDER BY at, id",
    )
    .bind(attempt_id)
    .fetch_all(&state.db)
    .await?)
}

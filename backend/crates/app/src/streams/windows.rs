//! Окна прохождения «тест × поток» (spec streams-enrollment, «Окно прохождения»).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::{self, CourseRole};
use crate::attempts::lifecycle;
use crate::audit::AuditEntry;
use crate::auth::session::CurrentUser;
use crate::authoring::model::{self, TestRow, TestStatus};
use crate::error::{ApiResult, AppError};
use crate::http::extract::{ClientIp, PathPair};
use crate::state::AppState;

/// Окно для одного потока.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct WindowState {
    pub stream_id: Uuid,
    pub stream_name: String,
    pub is_open: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub open_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub close_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub closed_at: Option<OffsetDateTime>,
    pub active_attempts: i64,
}

/// Окна теста по всем потокам курса.
#[utoipa::path(get, path = "/api/tests/{test_id}/windows", tag = "streams", params(("test_id" = Uuid, Path)),
    responses((status = 200, body = [WindowState])))]
pub(crate) async fn list_windows(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(test_id): Path<Uuid>,
) -> ApiResult<Json<Vec<WindowState>>> {
    let test = model::load_test(&state.db, test_id).await?;
    let _role = access::require_on(&state.db, &user, test.course_id, CourseRole::can_view).await?;
    let windows = sqlx::query_as(
        "SELECT s.id AS stream_id, s.name AS stream_name, coalesce(w.is_open, false) AS is_open,
                w.open_at, w.close_at, w.closed_at,
                (SELECT count(*) FROM attempts a WHERE a.test_id = $1 AND a.stream_id = s.id AND a.submitted_at IS NULL) AS active_attempts
         FROM stream_courses sc JOIN streams s ON s.id = sc.stream_id
         LEFT JOIN test_windows w ON w.test_id = $1 AND w.stream_id = s.id
         WHERE sc.course_id = $2 ORDER BY s.name",
    )
    .bind(test_id)
    .bind(test.course_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(windows))
}

/// Действие с окном.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum WindowChange {
    /// Открыть сейчас (отменяет запланированное открытие).
    Open,
    /// Закрыть сейчас (отменяет запланированное закрытие).
    Close,
    /// Запланировать (время в UTC; интерфейс вводит по МСК).
    Schedule {
        #[serde(default, with = "time::serde::rfc3339::option")]
        open_at: Option<OffsetDateTime>,
        #[serde(default, with = "time::serde::rfc3339::option")]
        close_at: Option<OffsetDateTime>,
    },
}

/// Открывает, закрывает или планирует окно для потока.
#[utoipa::path(put, path = "/api/tests/{test_id}/windows/{stream_id}", tag = "streams",
    params(("test_id" = Uuid, Path), ("stream_id" = Uuid, Path)), request_body = WindowChange, responses((status = 204)))]
pub(crate) async fn change_window(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path((test_id, stream_id)): PathPair,
    Json(change): Json<WindowChange>,
) -> ApiResult<StatusCode> {
    let test = model::load_test(&state.db, test_id).await?;
    let _role =
        access::require_on(&state.db, &user, test.course_id, CourseRole::can_manage).await?;
    ensure_openable(&test, &change)?;
    let mut tx = state.db.begin().await?;
    ensure_assigned(&mut tx, test.course_id, stream_id).await?;
    let now = state.clock.now();
    let action = apply(&mut tx, (test_id, stream_id), &change, (user.id, now)).await?;
    let details = serde_json::json!({ "stream_id": stream_id, "test_id": test_id });
    AuditEntry::new(action, "test")
        .by(Some(user.id), Some(ip))
        .on(test_id)
        .in_course(test.course_id)
        .with(details)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_assigned(tx: &mut PgConnection, course_id: Uuid, stream_id: Uuid) -> ApiResult<()> {
    let assigned: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM stream_courses WHERE course_id = $1 AND stream_id = $2)",
    )
    .bind(course_id)
    .bind(stream_id)
    .fetch_one(&mut *tx)
    .await?;
    if assigned {
        Ok(())
    } else {
        Err(AppError::BadRequest("Поток не назначен на этот курс".to_owned()))
    }
}

/// Окно: тест и поток.
type WindowKey = (Uuid, Uuid);

/// Запланированные открытие и закрытие.
type Planned = (Option<OffsetDateTime>, Option<OffsetDateTime>);

impl WindowChange {
    /// Начало UPDATE окна и действие для аудит-лога.
    const fn statement(&self) -> (&'static str, &'static str) {
        match self {
            Self::Open => (
                "UPDATE test_windows SET is_open = true, open_at = NULL, closed_at = NULL",
                "window.open",
            ),
            Self::Close => (
                "UPDATE test_windows SET is_open = false, close_at = NULL, closed_at = $3",
                "window.close",
            ),
            Self::Schedule { .. } => {
                ("UPDATE test_windows SET open_at = $4, close_at = $5", "window.schedule")
            }
        }
    }

    const fn planned(&self) -> Planned {
        match self {
            Self::Schedule { open_at, close_at } => (*open_at, *close_at),
            Self::Open | Self::Close => (None, None),
        }
    }
}

/// Открывать (и планировать) можно только опубликованный тест; закрыть — любой.
fn ensure_openable(test: &TestRow, change: &WindowChange) -> ApiResult<()> {
    if test.status() != TestStatus::Published && !matches!(change, WindowChange::Close) {
        return Err(AppError::Conflict("Открыть можно только опубликованный тест".to_owned()));
    }
    Ok(())
}

/// Закрытие, если задано вместе с открытием, должно быть позже него.
fn check_planned((open_at, close_at): Planned) -> ApiResult<Planned> {
    if let (Some(opens), Some(closes)) = (open_at, close_at)
        && closes <= opens
    {
        return Err(AppError::BadRequest("Закрытие должно быть позже открытия".to_owned()));
    }
    Ok((open_at, close_at))
}

/// cancel-safe: yes — в транзакции вызывающего.
async fn ensure_window(tx: &mut PgConnection, (test_id, stream_id): WindowKey) -> ApiResult<()> {
    let _row = sqlx::query(
        "INSERT INTO test_windows (test_id, stream_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(test_id)
    .bind(stream_id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// cancel-safe: yes — в транзакции вызывающего.
async fn apply(
    tx: &mut PgConnection,
    (test_id, stream_id): WindowKey,
    change: &WindowChange,
    (user_id, now): (Uuid, OffsetDateTime),
) -> ApiResult<&'static str> {
    ensure_window(tx, (test_id, stream_id)).await?;
    let (sql, action) = change.statement();
    let (open_at, close_at) = check_planned(change.planned())?;
    let statement =
        format!("{sql}, updated_by = $6, updated_at = $3 WHERE test_id = $1 AND stream_id = $2");
    let _updated = sqlx::query(sqlx::AssertSqlSafe(statement))
        .bind(test_id)
        .bind(stream_id)
        .bind(now)
        .bind(open_at)
        .bind(close_at)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    if matches!(change, WindowChange::Close) {
        lifecycle::close_untimed(tx, (test_id, stream_id), now).await?;
    }
    Ok(action)
}

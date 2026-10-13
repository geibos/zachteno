//! Потоки: создание, состав, назначение на курсы, исключение студентов.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::{self, CourseRole};
use crate::audit::AuditEntry;
use crate::auth::session::CurrentUser;
use crate::authoring::courses::{Created, non_empty};
use crate::error::{ApiResult, AppError};
use crate::http::extract::{ClientIp, PathPair};
use crate::state::AppState;
use crate::streams::access::{can_create, require_manager};

/// Поток в списке.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct StreamSummary {
    pub id: Uuid,
    pub name: String,
    pub members: i64,
    pub courses: Vec<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Потоки, которыми я управляю.
#[utoipa::path(get, path = "/api/streams", tag = "streams", responses((status = 200, body = [StreamSummary])))]
pub(crate) async fn list_streams(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<StreamSummary>>> {
    let streams = sqlx::query_as(
        "SELECT s.id, s.name, s.created_at,
                (SELECT count(*) FROM stream_members m WHERE m.stream_id = s.id AND m.removed_at IS NULL) AS members,
                coalesce((SELECT array_agg(c.title ORDER BY c.title) FROM stream_courses sc JOIN courses c ON c.id = sc.course_id
                          WHERE sc.stream_id = s.id), '{}') AS courses
         FROM streams s
         WHERE $2 OR s.created_by = $1 OR EXISTS (
            SELECT 1 FROM stream_courses sc JOIN courses c ON c.id = sc.course_id
            WHERE sc.stream_id = s.id AND (c.owner_id = $1
              OR EXISTS (SELECT 1 FROM course_teachers t WHERE t.course_id = c.id AND t.user_id = $1)))
         ORDER BY s.created_at DESC",
    )
    .bind(user.id)
    .bind(user.roles.superuser)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(streams))
}

/// Новый поток.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct NewStream {
    pub name: String,
}

/// Создаёт поток.
#[utoipa::path(post, path = "/api/streams", tag = "streams", request_body = NewStream, responses((status = 201, body = Created)))]
pub(crate) async fn create_stream(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Json(request): Json<NewStream>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    if !can_create(&state.db, &user).await? {
        return Err(AppError::Forbidden);
    }
    let name = non_empty(&request.name, "Название потока")?;
    let mut tx = state.db.begin().await?;
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO streams (name, created_by) VALUES ($1, $2) RETURNING id")
            .bind(name)
            .bind(user.id)
            .fetch_one(&mut *tx)
            .await?;
    AuditEntry::new("stream.create", "stream")
        .by(Some(user.id), Some(ip))
        .on(id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

/// Участник потока.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct Member {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub email_verified: bool,
    pub trusted_registration: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub joined_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub removed_at: Option<OffsetDateTime>,
}

/// Курс потока.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct StreamCourse {
    pub course_id: Uuid,
    pub title: String,
}

/// Поток целиком.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct StreamDetail {
    pub id: Uuid,
    pub name: String,
    pub courses: Vec<StreamCourse>,
    pub members: Vec<Member>,
}

const STREAM_COURSES: &str =
    "SELECT c.id AS course_id, c.title FROM stream_courses sc JOIN courses c ON c.id = sc.course_id
         WHERE sc.stream_id = $1 ORDER BY c.title";

const STREAM_MEMBERS: &str = "SELECT u.id AS user_id, u.email, u.display_name, u.email_verified_at IS NOT NULL AS email_verified,
                u.trusted_registration, m.joined_at, m.removed_at
         FROM stream_members m JOIN users u ON u.id = m.user_id WHERE m.stream_id = $1
         ORDER BY m.removed_at IS NOT NULL, u.display_name";

/// Поток: курсы и участники.
#[utoipa::path(get, path = "/api/streams/{stream_id}", tag = "streams", params(("stream_id" = Uuid, Path)),
    responses((status = 200, body = StreamDetail)))]
pub(crate) async fn get_stream(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(stream_id): Path<Uuid>,
) -> ApiResult<Json<StreamDetail>> {
    require_manager(&state.db, &user, stream_id).await?;
    let name: String = sqlx::query_scalar("SELECT name FROM streams WHERE id = $1")
        .bind(stream_id)
        .fetch_one(&state.db)
        .await?;
    let courses = sqlx::query_as(STREAM_COURSES).bind(stream_id).fetch_all(&state.db).await?;
    let members = sqlx::query_as(STREAM_MEMBERS).bind(stream_id).fetch_all(&state.db).await?;
    Ok(Json(StreamDetail { id: stream_id, name, courses, members }))
}

/// Курс для назначения.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CourseRef {
    pub course_id: Uuid,
}

/// Назначает поток на курс (методист-владелец или преподаватель курса).
#[utoipa::path(post, path = "/api/streams/{stream_id}/courses", tag = "streams", params(("stream_id" = Uuid, Path)),
    request_body = CourseRef, responses((status = 204)))]
pub(crate) async fn assign_course(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(stream_id): Path<Uuid>,
    Json(request): Json<CourseRef>,
) -> ApiResult<StatusCode> {
    let _role =
        access::require_on(&state.db, &user, request.course_id, CourseRole::can_manage).await?;
    ensure_stream(&state, stream_id).await?;
    let mut tx = state.db.begin().await?;
    let _inserted = sqlx::query("INSERT INTO stream_courses (stream_id, course_id, assigned_by) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(stream_id)
        .bind(request.course_id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    AuditEntry::new("stream.assign_course", "stream")
        .by(Some(user.id), Some(ip))
        .on(stream_id)
        .in_course(request.course_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// «Не найдено», если потока нет.
///
/// cancel-safe: yes — только чтение.
async fn ensure_stream(state: &AppState, stream_id: Uuid) -> ApiResult<()> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM streams WHERE id = $1)")
        .bind(stream_id)
        .fetch_one(&state.db)
        .await?;
    if exists { Ok(()) } else { Err(AppError::NotFound) }
}

/// Снимает поток с курса.
#[utoipa::path(delete, path = "/api/streams/{stream_id}/courses/{course_id}", tag = "streams",
    params(("stream_id" = Uuid, Path), ("course_id" = Uuid, Path)), responses((status = 204)))]
pub(crate) async fn unassign_course(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path((stream_id, course_id)): PathPair,
) -> ApiResult<StatusCode> {
    let _role = access::require_on(&state.db, &user, course_id, CourseRole::can_manage).await?;
    let mut tx = state.db.begin().await?;
    let _deleted =
        sqlx::query("DELETE FROM stream_courses WHERE stream_id = $1 AND course_id = $2")
            .bind(stream_id)
            .bind(course_id)
            .execute(&mut *tx)
            .await?;
    AuditEntry::new("stream.unassign_course", "stream")
        .by(Some(user.id), Some(ip))
        .on(stream_id)
        .in_course(course_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

const REMOVE_MEMBER: &str = "UPDATE stream_members SET removed_at = $3, removed_by = $4 WHERE stream_id = $1 AND user_id = $2 AND removed_at IS NULL";

/// Исключает студента: доступ к тестам потока пропадает, попытки остаются.
#[utoipa::path(delete, path = "/api/streams/{stream_id}/members/{user_id}", tag = "streams",
    params(("stream_id" = Uuid, Path), ("user_id" = Uuid, Path)), responses((status = 204)))]
pub(crate) async fn remove_member(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path((stream_id, member_id)): PathPair,
) -> ApiResult<StatusCode> {
    require_manager(&state.db, &user, stream_id).await?;
    let mut tx = state.db.begin().await?;
    let _updated = sqlx::query(REMOVE_MEMBER)
        .bind(stream_id)
        .bind(member_id)
        .bind(state.clock.now())
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    AuditEntry::new("stream.remove_member", "user")
        .by(Some(user.id), Some(ip))
        .on(member_id)
        .with(serde_json::json!({ "stream_id": stream_id }))
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

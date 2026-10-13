//! Курсы: список, создание, просмотр, изменение, архив (spec course-authoring, «Курсы»).

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
use crate::authoring::model::TestStatus;
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::state::AppState;

/// Курс в списке.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct CourseSummary {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub owner_name: String,
    pub owner_id: Uuid,
    pub archived: bool,
    pub tests_count: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Курсы, которые пользователь ведёт или видит: методист и суперпользователь — все, преподаватель — свои.
#[utoipa::path(get, path = "/api/courses", tag = "courses", responses((status = 200, body = [CourseSummary])))]
pub(crate) async fn list_courses(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Vec<CourseSummary>>> {
    let sees_all = user.roles.methodist || user.roles.superuser;
    let courses = sqlx::query_as(
        "SELECT c.id, c.title, c.description, o.display_name AS owner_name, c.owner_id,
                c.archived_at IS NOT NULL AS archived, c.created_at,
                (SELECT count(*) FROM tests t WHERE t.course_id = c.id AND t.status <> 'archived') AS tests_count
         FROM courses c JOIN users o ON o.id = c.owner_id
         WHERE $2 OR EXISTS (SELECT 1 FROM course_teachers t WHERE t.course_id = c.id AND t.user_id = $1)
         ORDER BY c.archived_at IS NOT NULL, c.created_at DESC",
    )
    .bind(user.id)
    .bind(sees_all)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(courses))
}

/// Новый курс.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct NewCourse {
    pub title: String,
    #[serde(default)]
    pub description: String,
}

/// Созданный объект.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct Created {
    pub id: Uuid,
}

/// Создаёт курс; создатель становится владельцем.
#[utoipa::path(post, path = "/api/courses", tag = "courses", request_body = NewCourse, responses((status = 201, body = Created)))]
pub(crate) async fn create_course(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Json(request): Json<NewCourse>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    access::require_methodist(&user)?;
    let title = non_empty(&request.title, "Название курса")?;
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO courses (title, description, owner_id) VALUES ($1, $2, $3) RETURNING id")
        .bind(title)
        .bind(request.description.trim())
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    AuditEntry::new("course.create", "course").by(Some(user.id), Some(ip)).on(id).in_course(id).record(&mut *tx).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

/// Обрезанная непустая строка до 300 символов.
pub(crate) fn non_empty<'text>(raw: &'text str, what: &str) -> ApiResult<&'text str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 300 {
        return Err(AppError::BadRequest(format!("{what}: от 1 до 300 символов")));
    }
    Ok(trimmed)
}

/// Тест в карточке курса.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct TestSummary {
    pub id: Uuid,
    pub title: String,
    #[schema(value_type = TestStatus)]
    pub status: String,
    pub position: i32,
    pub question_count: i64,
    pub locked: bool,
    pub anonymous_attempts: bool,
}

/// Курс целиком: описание, мои права и тесты.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CourseDetail {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub owner_id: Uuid,
    pub owner_name: String,
    pub archived: bool,
    pub can_edit: bool,
    pub can_manage: bool,
    pub tests: Vec<TestSummary>,
}

#[derive(FromRow)]
struct CourseRow {
    title: String,
    description: String,
    owner_id: Uuid,
    owner_name: String,
    archived: bool,
}

/// Карточка курса.
#[utoipa::path(get, path = "/api/courses/{course_id}", tag = "courses", params(("course_id" = Uuid, Path)),
    responses((status = 200, body = CourseDetail), (status = 404)))]
pub(crate) async fn get_course(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(course_id): Path<Uuid>,
) -> ApiResult<Json<CourseDetail>> {
    let role = access::require(access::course_role(&state.db, &user, course_id).await?, CourseRole::can_view)?;
    let row: CourseRow = sqlx::query_as(
        "SELECT c.title, c.description, c.owner_id, o.display_name AS owner_name, c.archived_at IS NOT NULL AS archived
         FROM courses c JOIN users o ON o.id = c.owner_id WHERE c.id = $1",
    )
    .bind(course_id)
    .fetch_one(&state.db)
    .await?;
    let tests = course_tests(&state, course_id).await?;
    Ok(Json(CourseDetail {
        id: course_id,
        title: row.title,
        description: row.description,
        owner_id: row.owner_id,
        owner_name: row.owner_name,
        archived: row.archived,
        can_edit: role.can_edit(),
        can_manage: role.can_manage(),
        tests,
    }))
}

async fn course_tests(state: &AppState, course_id: Uuid) -> ApiResult<Vec<TestSummary>> {
    Ok(sqlx::query_as(
        "SELECT t.id, t.title, t.status, t.position, t.locked_at IS NOT NULL AS locked, false AS anonymous_attempts,
                (SELECT count(*) FROM questions q WHERE q.test_id = t.id) AS question_count
         FROM tests t WHERE t.course_id = $1 ORDER BY t.position, t.created_at",
    )
    .bind(course_id)
    .fetch_all(&state.db)
    .await?)
}

/// Изменение курса.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CoursePatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub archived: Option<bool>,
}

/// Меняет название, описание или архивирует курс.
#[utoipa::path(patch, path = "/api/courses/{course_id}", tag = "courses", params(("course_id" = Uuid, Path)),
    request_body = CoursePatch, responses((status = 204)))]
pub(crate) async fn update_course(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(course_id): Path<Uuid>,
    Json(patch): Json<CoursePatch>,
) -> ApiResult<StatusCode> {
    access::require(access::course_role(&state.db, &user, course_id).await?, CourseRole::can_edit)?;
    let title = patch.title.as_deref().map(|title| non_empty(title, "Название курса")).transpose()?;
    let mut tx = state.db.begin().await?;
    let _updated = sqlx::query(
        "UPDATE courses SET title = coalesce($2, title), description = coalesce($3, description),
                archived_at = CASE WHEN $4 IS NULL THEN archived_at WHEN $4 THEN coalesce(archived_at, $5) END
         WHERE id = $1",
    )
    .bind(course_id)
    .bind(title)
    .bind(patch.description.as_deref().map(str::trim))
    .bind(patch.archived)
    .bind(state.clock.now())
    .execute(&mut *tx)
    .await?;
    let action = if patch.archived == Some(true) { "course.archive" } else { "course.update" };
    AuditEntry::new(action, "course").by(Some(user.id), Some(ip)).on(course_id).in_course(course_id).record(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

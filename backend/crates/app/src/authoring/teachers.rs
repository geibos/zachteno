//! Преподаватели курса (spec access-control, «Права методиста»).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::{self, CourseRole};
use crate::audit::AuditEntry;
use crate::auth::account;
use crate::auth::session::CurrentUser;
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::state::AppState;

/// Преподаватель курса.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct Teacher {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
}

/// Преподаватели курса.
#[utoipa::path(get, path = "/api/courses/{course_id}/teachers", tag = "courses", params(("course_id" = Uuid, Path)),
    responses((status = 200, body = [Teacher])))]
pub(crate) async fn list_teachers(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(course_id): Path<Uuid>,
) -> ApiResult<Json<Vec<Teacher>>> {
    access::require(access::course_role(&state.db, &user, course_id).await?, CourseRole::can_view)?;
    let teachers = sqlx::query_as(
        "SELECT u.id AS user_id, u.email, u.display_name FROM course_teachers t JOIN users u ON u.id = t.user_id
         WHERE t.course_id = $1 ORDER BY u.display_name",
    )
    .bind(course_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(teachers))
}

/// Кого назначить.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct AssignTeacher {
    pub email: String,
}

/// Назначает преподавателя на свой курс по email зарегистрированного пользователя.
#[utoipa::path(post, path = "/api/courses/{course_id}/teachers", tag = "courses", params(("course_id" = Uuid, Path)),
    request_body = AssignTeacher, responses((status = 204), (status = 404, description = "пользователь не найден")))]
pub(crate) async fn assign_teacher(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(course_id): Path<Uuid>,
    Json(request): Json<AssignTeacher>,
) -> ApiResult<StatusCode> {
    access::require(access::course_role(&state.db, &user, course_id).await?, CourseRole::can_edit)?;
    let email = account::normalize_email(&request.email)?;
    let teacher = account::find_for_login(&state.db, &email)
        .await?
        .ok_or_else(|| AppError::BadRequest("Пользователь с таким email не зарегистрирован".to_owned()))?;
    let mut tx = state.db.begin().await?;
    let _inserted = sqlx::query(
        "INSERT INTO course_teachers (course_id, user_id, assigned_by) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(course_id)
    .bind(teacher.id)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    AuditEntry::new("course.teacher_assign", "user").by(Some(user.id), Some(ip)).on(teacher.id).in_course(course_id).record(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Снимает преподавателя с курса.
#[utoipa::path(delete, path = "/api/courses/{course_id}/teachers/{user_id}", tag = "courses",
    params(("course_id" = Uuid, Path), ("user_id" = Uuid, Path)), responses((status = 204)))]
pub(crate) async fn remove_teacher(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path((course_id, teacher_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    access::require(access::course_role(&state.db, &user, course_id).await?, CourseRole::can_edit)?;
    let mut tx = state.db.begin().await?;
    let _deleted = sqlx::query("DELETE FROM course_teachers WHERE course_id = $1 AND user_id = $2")
        .bind(course_id)
        .bind(teacher_id)
        .execute(&mut *tx)
        .await?;
    AuditEntry::new("course.teacher_remove", "user").by(Some(user.id), Some(ip)).on(teacher_id).in_course(course_id).record(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

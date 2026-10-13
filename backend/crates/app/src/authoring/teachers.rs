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
use crate::http::extract::{ClientIp, PathPair};
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
    let _role = access::require_on(&state.db, &user, course_id, CourseRole::can_view).await?;
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

const ASSIGN_TEACHER: &str = "INSERT INTO course_teachers (course_id, user_id, assigned_by) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING";

/// Назначает преподавателя на свой курс по email зарегистрированного пользователя.
///
/// Email должен быть подтверждён: иначе можно было бы зарегистрироваться на чужой адрес
/// и получить доступ к результатам курса, когда методист назначит «коллегу».
#[utoipa::path(post, path = "/api/courses/{course_id}/teachers", tag = "courses", params(("course_id" = Uuid, Path)),
    request_body = AssignTeacher, responses((status = 204), (status = 400, description = "пользователь не найден или не подтвердил email")))]
pub(crate) async fn assign_teacher(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(course_id): Path<Uuid>,
    Json(request): Json<AssignTeacher>,
) -> ApiResult<StatusCode> {
    let _role = access::require_on(&state.db, &user, course_id, CourseRole::can_edit).await?;
    let teacher_id = verified_user(&state, &request.email).await?;
    let mut tx = state.db.begin().await?;
    let _inserted = sqlx::query(ASSIGN_TEACHER)
        .bind(course_id)
        .bind(teacher_id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    AuditEntry::new("course.teacher_assign", "user")
        .by(Some(user.id), Some(ip))
        .on(teacher_id)
        .in_course(course_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Пользователь с этим email, если адрес подтверждён.
///
/// cancel-safe: yes — только чтение.
async fn verified_user(state: &AppState, raw_email: &str) -> ApiResult<Uuid> {
    let email = account::normalize_email(raw_email)?;
    let (user_id, verified): (Uuid, bool) = sqlx::query_as(
        "SELECT id, email_verified_at IS NOT NULL FROM users WHERE email_lower = lower($1)",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(not_registered)?;
    if verified {
        Ok(user_id)
    } else {
        Err(AppError::BadRequest("Пользователь ещё не подтвердил email".to_owned()))
    }
}

fn not_registered() -> AppError {
    AppError::BadRequest("Пользователь с таким email не зарегистрирован".to_owned())
}

/// Снимает преподавателя с курса.
#[utoipa::path(delete, path = "/api/courses/{course_id}/teachers/{user_id}", tag = "courses",
    params(("course_id" = Uuid, Path), ("user_id" = Uuid, Path)), responses((status = 204)))]
pub(crate) async fn remove_teacher(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path((course_id, teacher_id)): PathPair,
) -> ApiResult<StatusCode> {
    let _role = access::require_on(&state.db, &user, course_id, CourseRole::can_edit).await?;
    let mut tx = state.db.begin().await?;
    let _deleted = sqlx::query("DELETE FROM course_teachers WHERE course_id = $1 AND user_id = $2")
        .bind(course_id)
        .bind(teacher_id)
        .execute(&mut *tx)
        .await?;
    AuditEntry::new("course.teacher_remove", "user")
        .by(Some(user.id), Some(ip))
        .on(teacher_id)
        .in_course(course_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

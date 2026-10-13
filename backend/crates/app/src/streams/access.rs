//! Кто управляет потоком.

use sqlx::PgExecutor;
use uuid::Uuid;

use crate::auth::session::CurrentUser;
use crate::error::{ApiResult, AppError};

/// Управляет ли пользователь потоком: создатель, суперпользователь, преподаватель или
/// методист-владелец курса, на который назначен поток.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn require_manager<'conn>(
    executor: impl PgExecutor<'conn>,
    user: &CurrentUser,
    stream_id: Uuid,
) -> ApiResult<()> {
    let found: Option<bool> = sqlx::query_scalar(
        "SELECT s.created_by = $2 OR $3
                OR EXISTS (SELECT 1 FROM stream_courses sc JOIN courses c ON c.id = sc.course_id
                           WHERE sc.stream_id = s.id AND (c.owner_id = $2
                             OR EXISTS (SELECT 1 FROM course_teachers t WHERE t.course_id = c.id AND t.user_id = $2)))
         FROM streams s WHERE s.id = $1",
    )
    .bind(stream_id)
    .bind(user.id)
    .bind(user.roles.superuser)
    .fetch_optional(executor)
    .await?;
    match found {
        Some(true) => Ok(()),
        Some(false) => Err(AppError::Forbidden),
        None => Err(AppError::NotFound),
    }
}

/// Может ли пользователь создавать потоки: методист, суперпользователь или преподаватель какого-либо курса.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn can_create<'conn>(
    executor: impl PgExecutor<'conn>,
    user: &CurrentUser,
) -> ApiResult<bool> {
    if user.roles.methodist || user.roles.superuser {
        return Ok(true);
    }
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM course_teachers WHERE user_id = $1)")
        .bind(user.id)
        .fetch_one(executor)
        .await?)
}

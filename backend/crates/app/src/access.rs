//! Роли и области полномочий (spec `access-control`).
//!
//! Нет доступа у студента — тот же ответ, что «не найдено».

use sqlx::{FromRow, PgExecutor};
use uuid::Uuid;

use crate::auth::session::CurrentUser;
use crate::error::AppError;

#[cfg(test)]
mod tests;

/// Что пользователь может делать с курсом.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CourseRole {
    /// Студент потока, назначенного на курс.
    Student,
    /// Методист, но не владелец: только чтение содержимого и результатов.
    Observer,
    /// Преподаватель курса: потоки, окна, результаты, антифрод.
    Teacher,
    /// Методист-владелец: всё, включая редактирование.
    Owner,
    /// Суперпользователь.
    Superuser,
}

impl CourseRole {
    /// Видит содержимое тестов и результаты всех участников.
    pub(crate) const fn can_view(self) -> bool {
        !matches!(self, Self::Student)
    }

    /// Управляет потоками, окнами и приглашениями курса.
    pub(crate) const fn can_manage(self) -> bool {
        matches!(self, Self::Teacher | Self::Owner | Self::Superuser)
    }

    /// Редактирует курс и его тесты, назначает преподавателей.
    pub(crate) const fn can_edit(self) -> bool {
        matches!(self, Self::Owner | Self::Superuser)
    }
}

#[derive(FromRow)]
struct Membership {
    is_owner: bool,
    is_teacher: bool,
    is_student: bool,
}

/// Роль пользователя на курсе; `None` — курса нет или он недоступен.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn course_role<'conn>(
    executor: impl PgExecutor<'conn>,
    user: &CurrentUser,
    course_id: Uuid,
) -> Result<Option<CourseRole>, sqlx::Error> {
    let found: Option<Membership> = sqlx::query_as(
        "SELECT c.owner_id = $2 AS is_owner,
                EXISTS (SELECT 1 FROM course_teachers t WHERE t.course_id = c.id AND t.user_id = $2) AS is_teacher,
                EXISTS (SELECT 1 FROM stream_courses sc JOIN stream_members m ON m.stream_id = sc.stream_id
                        WHERE sc.course_id = c.id AND m.user_id = $2 AND m.removed_at IS NULL) AS is_student
         FROM courses c WHERE c.id = $1",
    )
    .bind(course_id)
    .bind(user.id)
    .fetch_optional(executor)
    .await?;
    Ok(found.and_then(|membership| resolve(user, &membership)))
}

const fn resolve(user: &CurrentUser, membership: &Membership) -> Option<CourseRole> {
    if user.roles.superuser {
        Some(CourseRole::Superuser)
    } else if membership.is_owner && user.roles.methodist {
        Some(CourseRole::Owner)
    } else if membership.is_teacher {
        Some(CourseRole::Teacher)
    } else if user.roles.methodist {
        Some(CourseRole::Observer)
    } else if membership.is_student {
        Some(CourseRole::Student)
    } else {
        None
    }
}

/// Проверка права роли на курсе, например [`CourseRole::can_edit`].
pub(crate) type RoleCheck = fn(CourseRole) -> bool;

/// Требует роль, для которой `allowed` истинно; иначе «не найдено» или «нет доступа».
pub(crate) fn require(
    role: Option<CourseRole>,
    allowed: RoleCheck,
) -> Result<CourseRole, AppError> {
    match role {
        Some(granted) if allowed(granted) => Ok(granted),
        Some(CourseRole::Student) | None => Err(AppError::NotFound),
        Some(_) => Err(AppError::Forbidden),
    }
}

/// Роль пользователя на курсе, если для неё `allowed` истинно; иначе «не найдено» или
/// «нет доступа» — как [`require`].
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn require_on<'conn>(
    executor: impl PgExecutor<'conn>,
    user: &CurrentUser,
    course_id: Uuid,
    allowed: RoleCheck,
) -> Result<CourseRole, AppError> {
    require(course_role(executor, user, course_id).await?, allowed)
}

/// Только методист или суперпользователь.
pub(crate) const fn require_methodist(user: &CurrentUser) -> Result<(), AppError> {
    if user.roles.methodist || user.roles.superuser { Ok(()) } else { Err(AppError::Forbidden) }
}

/// Только суперпользователь.
pub(crate) const fn require_superuser(user: &CurrentUser) -> Result<(), AppError> {
    if user.roles.superuser { Ok(()) } else { Err(AppError::Forbidden) }
}

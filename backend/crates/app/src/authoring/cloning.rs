//! Клонирование тестов и курсов (spec course-authoring, «Клонирование»).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use sqlx::PgConnection;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::{self, CourseRole};
use crate::audit::AuditEntry;
use crate::auth::session::CurrentUser;
use crate::authoring::courses::Created;
use crate::authoring::model;
use crate::error::ApiResult;
use crate::http::extract::ClientIp;
use crate::state::AppState;

/// Куда клонировать тест.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CloneTarget {
    pub course_id: Uuid,
}

/// Клонирует тест (свой или чужой) в свой курс: черновик без попыток и без блокировки.
#[utoipa::path(post, path = "/api/tests/{test_id}/clone", tag = "tests", params(("test_id" = Uuid, Path)),
    request_body = CloneTarget, responses((status = 201, body = Created)))]
pub(crate) async fn clone_test(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(test_id): Path<Uuid>,
    Json(target): Json<CloneTarget>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let source = model::load_test(&state.db, test_id).await?;
    let _source_role =
        access::require_on(&state.db, &user, source.course_id, CourseRole::can_view).await?;
    let _target_role =
        access::require_on(&state.db, &user, target.course_id, CourseRole::can_edit).await?;
    let mut tx = state.db.begin().await?;
    let id = copy_test(&mut tx, test_id, target.course_id).await?;
    let details = serde_json::json!({ "source_test_id": test_id });
    AuditEntry::new("test.clone", "test")
        .by(Some(user.id), Some(ip))
        .on(id)
        .in_course(target.course_id)
        .with(details)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

/// Клонирует курс целиком; владелец клона — тот, кто клонирует.
#[utoipa::path(post, path = "/api/courses/{course_id}/clone", tag = "courses", params(("course_id" = Uuid, Path)),
    responses((status = 201, body = Created)))]
pub(crate) async fn clone_course(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(course_id): Path<Uuid>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    access::require_methodist(&user)?;
    let _role = access::require_on(&state.db, &user, course_id, CourseRole::can_view).await?;
    let mut tx = state.db.begin().await?;
    let id = copy_course(&mut tx, course_id, user.id).await?;
    let details = serde_json::json!({ "source_course_id": course_id });
    AuditEntry::new("course.clone", "course")
        .by(Some(user.id), Some(ip))
        .on(id)
        .in_course(id)
        .with(details)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

/// Копирует курс и его неархивные тесты по порядку; возвращает id копии.
///
/// cancel-safe: yes — в транзакции вызывающего.
async fn copy_course(tx: &mut PgConnection, course_id: Uuid, owner_id: Uuid) -> ApiResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO courses (title, description, owner_id)
         SELECT title || ' (копия)', description, $2 FROM courses WHERE id = $1 RETURNING id",
    )
    .bind(course_id)
    .bind(owner_id)
    .fetch_one(&mut *tx)
    .await?;
    let tests: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM tests WHERE course_id = $1 AND status <> 'archived' ORDER BY position",
    )
    .bind(course_id)
    .fetch_all(&mut *tx)
    .await?;
    for test_id in tests {
        let _copy = copy_test(tx, test_id, id).await?;
    }
    Ok(id)
}

/// Копирует тест с вопросами и вариантами; картинки неизменяемы, поэтому ссылки на них общие.
///
/// cancel-safe: yes — в транзакции вызывающего.
pub(crate) async fn copy_test(
    tx: &mut PgConnection,
    test_id: Uuid,
    course_id: Uuid,
) -> ApiResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tests (course_id, title, description, status, position, attempts_limit, time_limit_minutes,
             questions_per_attempt, free_navigation, shuffle_questions, shuffle_options, review_policy,
             review_not_before, final_policy)
         SELECT $2, title, description, 'draft',
                coalesce((SELECT max(position) + 1 FROM tests WHERE course_id = $2), 0), attempts_limit,
                time_limit_minutes, questions_per_attempt, free_navigation, shuffle_questions, shuffle_options,
                review_policy, review_not_before, final_policy
         FROM tests WHERE id = $1 RETURNING id",
    )
    .bind(test_id)
    .bind(course_id)
    .fetch_one(&mut *tx)
    .await?;
    copy_questions(tx, test_id, id).await?;
    Ok(id)
}

async fn copy_questions(tx: &mut PgConnection, from: Uuid, to: Uuid) -> ApiResult<()> {
    let _copied = sqlx::query(
        "WITH source AS (SELECT q.*, gen_random_uuid() AS new_id FROM questions q WHERE q.test_id = $1),
              inserted AS (
                INSERT INTO questions (id, test_id, position, kind, body, weight, image_id, image_alt, hint)
                SELECT new_id, $2, position, kind, body, weight, image_id, image_alt, hint FROM source RETURNING id)
         INSERT INTO options (question_id, position, body, image_id, image_alt, is_correct, penalty, pinned, penalty_custom)
         SELECT s.new_id, o.position, o.body, o.image_id, o.image_alt, o.is_correct, o.penalty, o.pinned, o.penalty_custom
         FROM options o JOIN source s ON s.id = o.question_id",
    )
    .bind(from)
    .bind(to)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

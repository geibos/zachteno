//! Тесты: создание, настройки, публикация, архив, клонирование (spec course-authoring).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use domain::validation::check_question;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::{self, CourseRole};
use crate::audit::AuditEntry;
use crate::auth::session::CurrentUser;
use crate::authoring::courses::{Created, non_empty};
use crate::authoring::model::{self, QuestionDetail, TestRow, TestSettings, TestStatus};
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::state::AppState;

/// Сообщение о блокировке начатого теста.
pub(crate) const LOCKED: &str =
    "Тест уже начали проходить: вопросы, баллы и настройки менять нельзя. Сделайте копию.";

/// Новый тест.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct NewTest {
    pub title: String,
}

const INSERT_TEST: &str = "INSERT INTO tests (course_id, title, attempts_limit, position)
         VALUES ($1, $2, 1, coalesce((SELECT max(position) + 1 FROM tests WHERE course_id = $1), 0)) RETURNING id";

/// Создаёт тест в курсе с настройками по умолчанию.
#[utoipa::path(post, path = "/api/courses/{course_id}/tests", tag = "tests", params(("course_id" = Uuid, Path)),
    request_body = NewTest, responses((status = 201, body = Created)))]
pub(crate) async fn create_test(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(course_id): Path<Uuid>,
    Json(request): Json<NewTest>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let _role = access::require_on(&state.db, &user, course_id, CourseRole::can_edit).await?;
    let title = non_empty(&request.title, "Название теста")?;
    let mut tx = state.db.begin().await?;
    let id: Uuid =
        sqlx::query_scalar(INSERT_TEST).bind(course_id).bind(title).fetch_one(&mut *tx).await?;
    AuditEntry::new("test.create", "test")
        .by(Some(user.id), Some(ip))
        .on(id)
        .in_course(course_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

/// Тест для редактора и предпросмотра.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TestDetail {
    pub id: Uuid,
    pub course_id: Uuid,
    pub title: String,
    pub description: String,
    pub status: TestStatus,
    pub settings: TestSettings,
    #[serde(with = "time::serde::rfc3339::option")]
    pub locked_at: Option<OffsetDateTime>,
    pub can_edit: bool,
    pub questions: Vec<QuestionDetail>,
}

/// Тест целиком (с правильными ответами) — только тем, кто видит курс.
#[utoipa::path(get, path = "/api/tests/{test_id}", tag = "tests", params(("test_id" = Uuid, Path)),
    responses((status = 200, body = TestDetail), (status = 404)))]
pub(crate) async fn get_test(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(test_id): Path<Uuid>,
) -> ApiResult<Json<TestDetail>> {
    let test = model::load_test(&state.db, test_id).await?;
    let role = access::require_on(&state.db, &user, test.course_id, CourseRole::can_view).await?;
    let questions = model::load_questions(&state.db, test_id).await?;
    let (status, settings) = (test.status(), test.settings());
    Ok(Json(TestDetail {
        id: test.id,
        course_id: test.course_id,
        title: test.title,
        description: test.description,
        status,
        settings,
        locked_at: test.locked_at,
        can_edit: role.can_edit(),
        questions,
    }))
}

/// Блокирует строку теста для правки и проверяет права (design D8).
///
/// cancel-safe: yes — в транзакции вызывающего.
pub(crate) async fn lock_for_edit(
    tx: &mut PgConnection,
    user: &CurrentUser,
    test_id: Uuid,
) -> ApiResult<TestRow> {
    let query = format!("SELECT {} FROM tests WHERE id = $1 FOR UPDATE", model::TEST_COLUMNS);
    let test: TestRow = sqlx::query_as(sqlx::AssertSqlSafe(query))
        .bind(test_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    let _role = access::require_on(&mut *tx, user, test.course_id, CourseRole::can_edit).await?;
    Ok(test)
}

/// Изменение теста.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct TestPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub settings: Option<TestSettings>,
}

/// Меняет название, описание и настройки. У заблокированного теста — только название,
/// описание и политику показа результатов.
#[utoipa::path(patch, path = "/api/tests/{test_id}", tag = "tests", params(("test_id" = Uuid, Path)),
    request_body = TestPatch, responses((status = 204), (status = 409)))]
pub(crate) async fn update_test(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(test_id): Path<Uuid>,
    Json(patch): Json<TestPatch>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let test = lock_for_edit(&mut tx, &user, test_id).await?;
    let settings = checked_settings(&test, patch.settings)?;
    let title =
        patch.title.as_deref().map(|title| non_empty(title, "Название теста")).transpose()?;
    let change = TestChange { title, description: patch.description.as_deref(), settings };
    save_test(&mut tx, test_id, &change, state.clock.now()).await?;
    AuditEntry::new("test.update", "test")
        .by(Some(user.id), Some(ip))
        .on(test_id)
        .in_course(test.course_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Новые настройки (или прежние, если не переданы), проверенные; у заблокированного теста
/// меняться может только политика показа результатов.
fn checked_settings(test: &TestRow, requested: Option<TestSettings>) -> ApiResult<TestSettings> {
    let settings = requested.unwrap_or_else(|| test.settings());
    settings.validate()?;
    if test.locked_at.is_some() && !settings.same_except_review(&test.settings()) {
        return Err(AppError::Conflict(LOCKED.to_owned()));
    }
    Ok(settings)
}

/// Проверенное изменение теста.
struct TestChange<'patch> {
    title: Option<&'patch str>,
    description: Option<&'patch str>,
    settings: TestSettings,
}

const UPDATE_TEST: &str = "UPDATE tests SET title = coalesce($2, title), description = coalesce($3, description),
            attempts_limit = $4, time_limit_minutes = $5, questions_per_attempt = $6, free_navigation = $7,
            shuffle_questions = $8, shuffle_options = $9, review_policy = $10, review_not_before = $11,
            final_policy = $12, updated_at = $13
         WHERE id = $1";

/// cancel-safe: yes — в транзакции вызывающего.
async fn save_test(
    tx: &mut PgConnection,
    test_id: Uuid,
    change: &TestChange<'_>,
    now: OffsetDateTime,
) -> ApiResult<()> {
    let settings = &change.settings;
    let _updated = sqlx::query(UPDATE_TEST)
        .bind(test_id)
        .bind(change.title)
        .bind(change.description.map(str::trim))
        .bind(settings.attempts_limit)
        .bind(settings.time_limit_minutes)
        .bind(settings.questions_per_attempt)
        .bind(settings.free_navigation)
        .bind(settings.shuffle_questions)
        .bind(settings.shuffle_options)
        .bind(settings.review_policy.as_str())
        .bind(settings.review_not_before)
        .bind(settings.final_policy.as_str())
        .bind(now)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Публикует тест после проверки всех вопросов.
#[utoipa::path(post, path = "/api/tests/{test_id}/publish", tag = "tests", params(("test_id" = Uuid, Path)),
    responses((status = 204), (status = 400, description = "нарушены правила вопросов")))]
pub(crate) async fn publish_test(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(test_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let test = lock_for_edit(&mut tx, &user, test_id).await?;
    let questions = model::load_questions(&state.db, test_id).await?;
    check_publishable(&test, &questions)?;
    set_status(&mut tx, test_id, TestStatus::Published).await?;
    AuditEntry::new("test.publish", "test")
        .by(Some(user.id), Some(ip))
        .on(test_id)
        .in_course(test.course_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Проверяет правила публикации; ошибка называет номер проблемного вопроса.
pub(crate) fn check_publishable(test: &TestRow, questions: &[QuestionDetail]) -> ApiResult<()> {
    if questions.is_empty() {
        return Err(AppError::BadRequest("В тесте нет вопросов".to_owned()));
    }
    for (number, question) in (1_usize..).zip(questions) {
        check_question(&question.scored())
            .map_err(|violation| AppError::BadRequest(format!("Вопрос {number}: {violation}")))?;
    }
    let count = i32::try_from(questions.len()).unwrap_or(i32::MAX);
    if test.questions_per_attempt.is_some_and(|limit| limit > count) {
        return Err(AppError::BadRequest(format!(
            "В попытке не может быть больше вопросов, чем в тесте ({count})"
        )));
    }
    Ok(())
}

async fn set_status(tx: &mut PgConnection, test_id: Uuid, status: TestStatus) -> ApiResult<()> {
    let _updated = sqlx::query("UPDATE tests SET status = $2 WHERE id = $1")
        .bind(test_id)
        .bind(status.as_str())
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Отправляет тест в архив.
#[utoipa::path(post, path = "/api/tests/{test_id}/archive", tag = "tests", params(("test_id" = Uuid, Path)),
    responses((status = 204)))]
pub(crate) async fn archive_test(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(test_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let test = lock_for_edit(&mut tx, &user, test_id).await?;
    set_status(&mut tx, test_id, TestStatus::Archived).await?;
    AuditEntry::new("test.archive", "test")
        .by(Some(user.id), Some(ip))
        .on(test_id)
        .in_course(test.course_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

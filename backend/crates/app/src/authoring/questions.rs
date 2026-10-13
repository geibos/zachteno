//! Вопросы и варианты (spec course-authoring, «Вопросы», «Блокировка редактирования»).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use domain::scoring::{QuestionKind, ScoredOption, ScoredQuestion};
use domain::validation::{QuestionRuleViolation, check_question};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgConnection;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::audit::AuditEntry;
use crate::auth::session::CurrentUser;
use crate::authoring::courses::Created;
use crate::authoring::model::{TestRow, TestStatus};
use crate::authoring::quizzes::{LOCKED, lock_for_edit};
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::state::AppState;

/// Вариант ответа от редактора.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct OptionInput {
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub image_id: Option<Uuid>,
    #[serde(default)]
    pub image_alt: String,
    #[serde(default)]
    pub is_correct: bool,
    /// Штраф (≤ 0); `null` — по умолчанию для режима оценки.
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub penalty: Option<Decimal>,
    #[serde(default)]
    pub pinned: bool,
}

/// Вопрос от редактора: заменяет вопрос целиком вместе с вариантами.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct QuestionInput {
    pub kind: QuestionKind,
    #[serde(default)]
    pub body: String,
    /// Вес; по умолчанию 1.
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub weight: Option<Decimal>,
    #[serde(default)]
    pub image_id: Option<Uuid>,
    #[serde(default)]
    pub image_alt: String,
    #[serde(default)]
    pub options: Vec<OptionInput>,
}

impl QuestionInput {
    fn weight(&self) -> Decimal {
        self.weight.unwrap_or(Decimal::ONE)
    }

    fn penalty(&self, option: &OptionInput) -> Decimal {
        if option.is_correct {
            return Decimal::ZERO;
        }
        option.penalty.unwrap_or_else(|| self.kind.default_penalty(self.weight()))
    }

    /// Вес > 0, штрафы ≤ 0; у опубликованного теста — все правила публикации.
    fn validate(&self, status: TestStatus) -> ApiResult<()> {
        match check_question(&self.scored()) {
            Err(violation) if status == TestStatus::Published || is_structural(violation) => {
                Err(AppError::BadRequest(violation.to_string()))
            }
            Ok(()) | Err(_) => Ok(()),
        }
    }

    /// Вопрос в виде, понятном проверке правил.
    fn scored(&self) -> ScoredQuestion {
        ScoredQuestion {
            kind: self.kind,
            weight: self.weight(),
            options: self
                .options
                .iter()
                .map(|option| ScoredOption {
                    option_id: Uuid::nil(),
                    is_correct: option.is_correct,
                    penalty: self.penalty(option),
                })
                .collect(),
        }
    }
}

const fn is_structural(violation: QuestionRuleViolation) -> bool {
    use domain::validation::QuestionRuleViolation as Rule;
    matches!(violation, Rule::NonPositiveWeight | Rule::PositivePenalty | Rule::FreeTextHasOptions)
}

/// Добавляет вопрос в конец теста.
#[utoipa::path(post, path = "/api/tests/{test_id}/questions", tag = "tests", params(("test_id" = Uuid, Path)),
    request_body = QuestionInput, responses((status = 201, body = Created), (status = 409)))]
pub(crate) async fn create_question(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(test_id): Path<Uuid>,
    Json(input): Json<QuestionInput>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let mut tx = state.db.begin().await?;
    let test = editable(&mut tx, &user, test_id).await?;
    input.validate(test.status())?;
    let question_id: Uuid = sqlx::query_scalar(
        "INSERT INTO questions (test_id, position, kind) VALUES ($1,
             coalesce((SELECT max(position) + 1 FROM questions WHERE test_id = $1), 0), $2) RETURNING id",
    )
    .bind(test_id)
    .bind(input.kind.as_str())
    .fetch_one(&mut *tx)
    .await?;
    write_question(&mut tx, question_id, &input).await?;
    audit(&mut tx, (&user, ClientIp(ip)), "question.create", (&test, question_id)).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(Created { id: question_id })))
}

/// Заменяет вопрос целиком.
#[utoipa::path(put, path = "/api/questions/{question_id}", tag = "tests", params(("question_id" = Uuid, Path)),
    request_body = QuestionInput, responses((status = 204), (status = 409)))]
pub(crate) async fn update_question(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(question_id): Path<Uuid>,
    Json(input): Json<QuestionInput>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let test_id = test_of(&mut tx, question_id).await?;
    let test = editable(&mut tx, &user, test_id).await?;
    input.validate(test.status())?;
    write_question(&mut tx, question_id, &input).await?;
    audit(&mut tx, (&user, ClientIp(ip)), "question.update", (&test, question_id)).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Удаляет вопрос.
#[utoipa::path(delete, path = "/api/questions/{question_id}", tag = "tests", params(("question_id" = Uuid, Path)),
    responses((status = 204), (status = 409)))]
pub(crate) async fn delete_question(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(question_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let test_id = test_of(&mut tx, question_id).await?;
    let test = editable(&mut tx, &user, test_id).await?;
    let _deleted = sqlx::query("DELETE FROM questions WHERE id = $1")
        .bind(question_id)
        .execute(&mut *tx)
        .await?;
    audit(&mut tx, (&user, ClientIp(ip)), "question.delete", (&test, question_id)).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Новый порядок вопросов.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct QuestionOrder {
    pub question_ids: Vec<Uuid>,
}

/// Меняет порядок вопросов.
#[utoipa::path(post, path = "/api/tests/{test_id}/questions/order", tag = "tests", params(("test_id" = Uuid, Path)),
    request_body = QuestionOrder, responses((status = 204), (status = 409)))]
pub(crate) async fn reorder_questions(
    State(state): State<AppState>,
    (user, _ip): (CurrentUser, ClientIp),
    Path(test_id): Path<Uuid>,
    Json(order): Json<QuestionOrder>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let _test = editable(&mut tx, &user, test_id).await?;
    let _updated = sqlx::query(
        "UPDATE questions q SET position = ordered.position - 1
         FROM unnest($2::uuid[]) WITH ORDINALITY AS ordered(id, position)
         WHERE q.id = ordered.id AND q.test_id = $1",
    )
    .bind(test_id)
    .bind(&order.question_ids)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn editable(tx: &mut PgConnection, user: &CurrentUser, test_id: Uuid) -> ApiResult<TestRow> {
    let test = lock_for_edit(tx, user, test_id).await?;
    if test.locked_at.is_some() {
        return Err(AppError::Conflict(LOCKED.to_owned()));
    }
    Ok(test)
}

async fn test_of(tx: &mut PgConnection, question_id: Uuid) -> ApiResult<Uuid> {
    sqlx::query_scalar("SELECT test_id FROM questions WHERE id = $1")
        .bind(question_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)
}

async fn write_question(
    tx: &mut PgConnection,
    question_id: Uuid,
    input: &QuestionInput,
) -> ApiResult<()> {
    let _updated = sqlx::query("UPDATE questions SET kind = $2, body = $3, weight = $4, image_id = $5, image_alt = $6 WHERE id = $1")
        .bind(question_id)
        .bind(input.kind.as_str())
        .bind(&input.body)
        .bind(input.weight())
        .bind(input.image_id)
        .bind(input.image_alt.trim())
        .execute(&mut *tx)
        .await?;
    let _cleared = sqlx::query("DELETE FROM options WHERE question_id = $1")
        .bind(question_id)
        .execute(&mut *tx)
        .await?;
    for (position, option) in (0_i32..).zip(&input.options) {
        insert_option(tx, question_id, position, (option, input.penalty(option))).await?;
    }
    Ok(())
}

async fn insert_option(
    tx: &mut PgConnection,
    question_id: Uuid,
    position: i32,
    (option, penalty): (&OptionInput, Decimal),
) -> ApiResult<()> {
    let _inserted = sqlx::query(
        "INSERT INTO options (question_id, position, body, image_id, image_alt, is_correct, penalty, pinned, penalty_custom)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(question_id)
    .bind(position)
    .bind(&option.body)
    .bind(option.image_id)
    .bind(option.image_alt.trim())
    .bind(option.is_correct)
    .bind(penalty)
    .bind(option.pinned)
    .bind(option.penalty.is_some() && !option.is_correct)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

async fn audit(
    tx: &mut PgConnection,
    (user, ClientIp(ip)): (&CurrentUser, ClientIp),
    action: &'static str,
    (test, question_id): (&TestRow, Uuid),
) -> ApiResult<()> {
    let details = serde_json::json!({ "test_id": test.id });
    AuditEntry::new(action, "question")
        .by(Some(user.id), Some(ip))
        .on(question_id)
        .in_course(test.course_id)
        .with(details)
        .record(&mut *tx)
        .await?;
    Ok(())
}

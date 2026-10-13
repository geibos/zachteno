//! Прохождение попытки (spec test-taking): выдача без правильных ответов, автосохранение,
//! навигация, сдача, клиентские события.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use domain::attempt_rules::{self, AttemptStatus, CurrentAnswer, Cursor, Navigation};
use domain::layout::PlacedQuestion;
use domain::scoring::QuestionKind;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::attempts::lifecycle::{self, SubmitReason, log_event};
use crate::auth::session::CurrentUser;
use crate::authoring::model::{self, ImageRef, QuestionDetail};
use crate::error::{ApiResult, AppError};
use crate::state::AppState;

/// Попытка из базы.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct AttemptRow {
    pub id: Uuid,
    pub test_id: Uuid,
    pub user_id: Uuid,
    pub stream_id: Option<Uuid>,
    pub is_preview: bool,
    pub started_at: OffsetDateTime,
    pub deadline_at: Option<OffsetDateTime>,
    pub submitted_at: Option<OffsetDateTime>,
    pub submit_reason: Option<String>,
    pub layout: sqlx::types::Json<Vec<PlacedQuestion>>,
    pub current_index: i32,
    pub lock_applied: bool,
    pub score: Option<rust_decimal::Decimal>,
    pub max_score: rust_decimal::Decimal,
}

const ATTEMPT_COLUMNS: &str = "id, test_id, user_id, stream_id, is_preview, started_at, deadline_at, submitted_at,
    submit_reason, layout, current_index, lock_applied, score, max_score";

impl AttemptRow {
    pub(crate) const fn status(&self) -> AttemptStatus {
        if self.submitted_at.is_some() { AttemptStatus::Submitted } else { AttemptStatus::InProgress }
    }

    fn index_of(&self, question_id: Uuid) -> Option<usize> {
        self.layout.0.iter().position(|slot| slot.question_id == question_id)
    }
}

/// Своя попытка пользователя (чужая — «не найдено»).
///
/// cancel-safe: yes — только чтение (или блокировка строки в транзакции вызывающего).
pub(crate) async fn own_attempt(executor: &mut PgConnection, user: &CurrentUser, attempt_id: Uuid, for_update: bool) -> ApiResult<AttemptRow> {
    let lock = if for_update { " FOR UPDATE" } else { "" };
    let query = format!("SELECT {ATTEMPT_COLUMNS} FROM attempts WHERE id = $1 AND user_id = $2{lock}");
    sqlx::query_as(sqlx::AssertSqlSafe(query))
        .bind(attempt_id)
        .bind(user.id)
        .fetch_optional(&mut *executor)
        .await?
        .ok_or(AppError::NotFound)
}

/// Вид вопроса для студента: без режима оценки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Choice {
    Single,
    Multiple,
    FreeText,
}

impl From<QuestionKind> for Choice {
    fn from(kind: QuestionKind) -> Self {
        match kind {
            QuestionKind::Single => Self::Single,
            QuestionKind::MultipleAll | QuestionKind::MultipleAny => Self::Multiple,
            QuestionKind::FreeText => Self::FreeText,
        }
    }
}

/// Вариант для студента: без правильности и штрафа.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct RunnerOption {
    pub id: Uuid,
    pub body: String,
    pub image: Option<ImageRef>,
}

/// Сохранённый ответ.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct SavedAnswer {
    pub question_id: Uuid,
    pub selected_option_ids: Vec<Uuid>,
    pub text_answer: Option<String>,
    pub client_seq: i64,
    pub finalized: bool,
}

/// Вопрос попытки для студента.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct RunnerQuestion {
    pub id: Uuid,
    pub index: usize,
    pub choice: Choice,
    pub body: String,
    pub image: Option<ImageRef>,
    pub options: Vec<RunnerOption>,
}

/// Попытка для прохождения.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AttemptView {
    pub id: Uuid,
    pub test_id: Uuid,
    pub title: String,
    pub navigation: Navigation,
    pub is_preview: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub server_now: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub deadline_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub submitted_at: Option<OffsetDateTime>,
    pub current_index: i32,
    pub questions: Vec<RunnerQuestion>,
    pub answers: Vec<SavedAnswer>,
}

/// Попытка с вопросами в порядке раскладки, без признаков правильности.
#[utoipa::path(get, path = "/api/attempts/{attempt_id}", tag = "attempts", params(("attempt_id" = Uuid, Path)),
    responses((status = 200, body = AttemptView), (status = 404)))]
pub(crate) async fn get_attempt(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(attempt_id): Path<Uuid>,
) -> ApiResult<Json<AttemptView>> {
    let mut connection = state.db.acquire().await?;
    let attempt = own_attempt(&mut connection, &user, attempt_id, false).await?;
    let test = model::load_test(&mut *connection, attempt.test_id).await?;
    let questions = model::load_questions_in(&mut connection, attempt.test_id).await?;
    let answers = sqlx::query_as(
        "SELECT question_id, selected_option_ids, text_answer, client_seq, finalized FROM answers WHERE attempt_id = $1",
    )
    .bind(attempt_id)
    .fetch_all(&mut *connection)
    .await?;
    let navigation = if test.free_navigation { Navigation::Free } else { Navigation::Sequential };
    Ok(Json(AttemptView {
        id: attempt.id,
        test_id: attempt.test_id,
        title: test.title,
        navigation,
        is_preview: attempt.is_preview,
        server_now: state.clock.now(),
        deadline_at: attempt.deadline_at,
        submitted_at: attempt.submitted_at,
        current_index: attempt.current_index,
        questions: runner_questions(&attempt.layout.0, &questions),
        answers,
    }))
}

fn runner_questions(layout: &[PlacedQuestion], questions: &[QuestionDetail]) -> Vec<RunnerQuestion> {
    layout
        .iter()
        .filter_map(|slot| questions.iter().find(|question| question.id == slot.question_id).map(|question| (slot, question)))
        .enumerate()
        .map(|(index, (slot, question))| RunnerQuestion {
            id: question.id,
            index,
            choice: Choice::from(question.kind),
            body: question.body.clone(),
            image: question.image.clone(),
            options: slot
                .option_ids
                .iter()
                .filter_map(|option_id| question.options.iter().find(|option| option.id == *option_id))
                .map(|option| RunnerOption { id: option.id, body: option.body.clone(), image: option.image.clone() })
                .collect(),
        })
        .collect()
}

/// Ответ на вопрос.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct AnswerInput {
    #[serde(default)]
    pub selected_option_ids: Vec<Uuid>,
    #[serde(default)]
    pub text: Option<String>,
    /// Монотонный номер изменения на клиенте: старые запросы игнорируются.
    pub client_seq: i64,
}

/// Подтверждение сохранения.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AnswerSaved {
    pub client_seq: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub saved_at: OffsetDateTime,
}

/// Наибольшая длина свободного ответа.
const MAX_TEXT_CHARS: usize = 10_000;

/// Сохраняет ответ (идемпотентно по `client_seq`).
#[utoipa::path(put, path = "/api/attempts/{attempt_id}/answers/{question_id}", tag = "attempts",
    params(("attempt_id" = Uuid, Path), ("question_id" = Uuid, Path)), request_body = AnswerInput,
    responses((status = 200, body = AnswerSaved), (status = 409, description = "время вышло или вопрос закрыт")))]
pub(crate) async fn save_answer(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((attempt_id, question_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<AnswerInput>,
) -> ApiResult<Json<AnswerSaved>> {
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let attempt = own_attempt(&mut tx, &user, attempt_id, true).await?;
    let test = model::load_test(&mut *tx, attempt.test_id).await?;
    ensure_accepting(&attempt, now)?;
    let index = attempt.index_of(question_id).ok_or(AppError::NotFound)?;
    let navigation = if test.free_navigation { Navigation::Free } else { Navigation::Sequential };
    let cursor = Cursor { navigation, current_index: usize::try_from(attempt.current_index).unwrap_or(0) };
    if !cursor.can_answer(index) {
        return Err(AppError::Conflict("К этому вопросу вернуться нельзя".to_owned()));
    }
    let question = question_of(&mut tx, attempt.test_id, question_id).await?;
    validate(&question, &attempt, &input)?;
    upsert(&mut tx, attempt_id, question_id, (&input, now)).await?;
    lock_test_once(&mut tx, &attempt, now).await?;
    let details = serde_json::json!({ "selected": input.selected_option_ids, "text_length": input.text.as_deref().map_or(0, |text| text.chars().count()) });
    log_event(&mut tx, attempt_id, ("answer_saved", Some(question_id)), (details, now)).await?;
    tx.commit().await?;
    Ok(Json(AnswerSaved { client_seq: input.client_seq, saved_at: now }))
}

fn ensure_accepting(attempt: &AttemptRow, now: OffsetDateTime) -> ApiResult<()> {
    if attempt_rules::accepts_answer(now, attempt.deadline_at, attempt.status()) {
        Ok(())
    } else {
        Err(AppError::Conflict("Попытка завершена или время вышло".to_owned()))
    }
}

async fn question_of(tx: &mut PgConnection, test_id: Uuid, question_id: Uuid) -> ApiResult<QuestionDetail> {
    model::load_questions_in(tx, test_id).await?.into_iter().find(|question| question.id == question_id).ok_or(AppError::NotFound)
}

fn validate(question: &QuestionDetail, attempt: &AttemptRow, input: &AnswerInput) -> ApiResult<()> {
    let allowed = attempt.layout.0.iter().find(|slot| slot.question_id == question.id).map(|slot| slot.option_ids.clone()).unwrap_or_default();
    if input.selected_option_ids.iter().any(|option_id| !allowed.contains(option_id)) {
        return Err(AppError::BadRequest("Вариант не из этого вопроса".to_owned()));
    }
    let too_many = question.kind == QuestionKind::Single && input.selected_option_ids.len() > 1;
    let text_len = input.text.as_deref().map_or(0, |text| text.chars().count());
    let wrong_shape = question.kind == QuestionKind::FreeText && !input.selected_option_ids.is_empty();
    if too_many || wrong_shape || text_len > MAX_TEXT_CHARS {
        return Err(AppError::BadRequest("Ответ не подходит к типу вопроса".to_owned()));
    }
    Ok(())
}

async fn upsert(tx: &mut PgConnection, attempt_id: Uuid, question_id: Uuid, (input, now): (&AnswerInput, OffsetDateTime)) -> ApiResult<()> {
    let _saved = sqlx::query(
        "INSERT INTO answers (attempt_id, question_id, selected_option_ids, text_answer, client_seq, saved_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (attempt_id, question_id) DO UPDATE
           SET selected_option_ids = EXCLUDED.selected_option_ids, text_answer = EXCLUDED.text_answer,
               client_seq = EXCLUDED.client_seq, saved_at = EXCLUDED.saved_at
           WHERE answers.client_seq < EXCLUDED.client_seq AND NOT answers.finalized",
    )
    .bind(attempt_id)
    .bind(question_id)
    .bind(&input.selected_option_ids)
    .bind(input.text.as_deref())
    .bind(input.client_seq)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Первый ответ попытки блокирует тест от правок (design D8). Предпросмотр не блокирует.
async fn lock_test_once(tx: &mut PgConnection, attempt: &AttemptRow, now: OffsetDateTime) -> ApiResult<()> {
    if attempt.lock_applied || attempt.is_preview {
        return Ok(());
    }
    let _locked = sqlx::query("UPDATE tests SET locked_at = $2 WHERE id = $1 AND locked_at IS NULL")
        .bind(attempt.test_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let _flag = sqlx::query("UPDATE attempts SET lock_applied = true WHERE id = $1").bind(attempt.id).execute(&mut *tx).await?;
    Ok(())
}

/// Переход к вопросу.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct NavigateInput {
    pub to_index: usize,
}

/// Переходит к вопросу; в последовательном режиме — только вперёд и только с ответом.
#[utoipa::path(post, path = "/api/attempts/{attempt_id}/navigate", tag = "attempts", params(("attempt_id" = Uuid, Path)),
    request_body = NavigateInput, responses((status = 204), (status = 409)))]
pub(crate) async fn navigate(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(attempt_id): Path<Uuid>,
    Json(input): Json<NavigateInput>,
) -> ApiResult<StatusCode> {
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let attempt = own_attempt(&mut tx, &user, attempt_id, true).await?;
    ensure_accepting(&attempt, now)?;
    let test = model::load_test(&mut *tx, attempt.test_id).await?;
    let current = usize::try_from(attempt.current_index).unwrap_or(0);
    let navigation = if test.free_navigation { Navigation::Free } else { Navigation::Sequential };
    let answered = current_answered(&mut tx, &attempt, current).await?;
    if input.to_index >= attempt.layout.0.len() || !(Cursor { navigation, current_index: current }).can_move_to(input.to_index, answered) {
        return Err(AppError::Conflict("Сначала ответьте на текущий вопрос".to_owned()));
    }
    finalize_if_sequential(&mut tx, &attempt, (navigation, current)).await?;
    let target = i32::try_from(input.to_index).unwrap_or(i32::MAX);
    let _moved = sqlx::query("UPDATE attempts SET current_index = $2 WHERE id = $1").bind(attempt_id).bind(target).execute(&mut *tx).await?;
    log_event(&mut tx, attempt_id, ("navigate", None), (serde_json::json!({ "from": current, "to": input.to_index }), now)).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn current_answered(tx: &mut PgConnection, attempt: &AttemptRow, current: usize) -> ApiResult<CurrentAnswer> {
    let Some(slot) = attempt.layout.0.get(current) else { return Ok(CurrentAnswer::Missing) };
    let present: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM answers WHERE attempt_id = $1 AND question_id = $2
           AND (cardinality(selected_option_ids) > 0 OR length(coalesce(text_answer, '')) > 0))",
    )
    .bind(attempt.id)
    .bind(slot.question_id)
    .fetch_one(&mut *tx)
    .await?;
    Ok(if present { CurrentAnswer::Present } else { CurrentAnswer::Missing })
}

async fn finalize_if_sequential(tx: &mut PgConnection, attempt: &AttemptRow, (navigation, current): (Navigation, usize)) -> ApiResult<()> {
    let Some(slot) = attempt.layout.0.get(current).filter(|_| navigation == Navigation::Sequential) else { return Ok(()) };
    let _final = sqlx::query("UPDATE answers SET finalized = true WHERE attempt_id = $1 AND question_id = $2")
        .bind(attempt.id)
        .bind(slot.question_id)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Итог сдачи.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct Submitted {
    #[serde(with = "time::serde::rfc3339::option")]
    pub submitted_at: Option<OffsetDateTime>,
    pub submit_reason: Option<String>,
}

/// Сдаёт попытку.
#[utoipa::path(post, path = "/api/attempts/{attempt_id}/submit", tag = "attempts", operation_id = "submit_attempt", params(("attempt_id" = Uuid, Path)),
    responses((status = 200, body = Submitted)))]
pub(crate) async fn submit(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(attempt_id): Path<Uuid>,
) -> ApiResult<Json<Submitted>> {
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let attempt = own_attempt(&mut tx, &user, attempt_id, true).await?;
    let reason = if attempt.deadline_at.is_some_and(|deadline| now > deadline) { SubmitReason::Timeout } else { SubmitReason::Manual };
    let _changed = lifecycle::submit(&mut tx, attempt_id, reason, attempt.deadline_at.filter(|_| reason == SubmitReason::Timeout).unwrap_or(now)).await?;
    let done = own_attempt(&mut tx, &user, attempt_id, false).await?;
    tx.commit().await?;
    Ok(Json(Submitted { submitted_at: done.submitted_at, submit_reason: done.submit_reason }))
}

/// Событие клиента.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClientEventKind {
    TabHidden,
    TabVisible,
    Offline,
    Online,
}

/// Пачка событий клиента.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ClientEvents {
    pub events: Vec<ClientEventKind>,
}

/// Записывает события вкладки и связи (с серверным временем).
#[utoipa::path(post, path = "/api/attempts/{attempt_id}/events", tag = "attempts", params(("attempt_id" = Uuid, Path)),
    request_body = ClientEvents, responses((status = 204)))]
pub(crate) async fn client_events(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(attempt_id): Path<Uuid>,
    Json(batch): Json<ClientEvents>,
) -> ApiResult<StatusCode> {
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let _attempt = own_attempt(&mut tx, &user, attempt_id, false).await?;
    for event in batch.events.iter().take(50) {
        let kind = match event {
            ClientEventKind::TabHidden => "tab_hidden",
            ClientEventKind::TabVisible => "tab_visible",
            ClientEventKind::Offline => "offline",
            ClientEventKind::Online => "online",
        };
        log_event(&mut tx, attempt_id, (kind, None), (serde_json::json!({}), now)).await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

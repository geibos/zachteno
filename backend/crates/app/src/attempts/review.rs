//! Разбор попытки по политике показа (spec test-taking, «Просмотр результатов»).

use axum::Json;
use axum::extract::{Path, State};
use domain::layout::PlacedQuestion;
use domain::review::{self, Context};
use domain::scoring;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{FromRow, PgConnection};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::attempts::runner::{AttemptRow, Choice, ReadMode, own_attempt, placed};
use crate::auth::session::CurrentUser;
use crate::authoring::model::{self, ImageRef, QuestionDetail, ReviewMode, TestRow};
use crate::error::{ApiResult, AppError};
use crate::state::AppState;

/// Доступен ли разбор и когда станет.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ReviewAvailability {
    pub available: bool,
    pub mode: ReviewMode,
    #[serde(with = "time::serde::rfc3339::option")]
    pub not_before: Option<OffsetDateTime>,
    /// Ждём закрытия окна или завершения чужих попыток потока.
    pub waiting_for_stream: bool,
}

#[derive(FromRow)]
struct StreamState {
    window_closed: bool,
    active: bool,
}

const STREAM_STATE: &str = "SELECT coalesce((SELECT NOT w.is_open AND w.closed_at IS NOT NULL FROM test_windows w
                          WHERE w.test_id = $1 AND w.stream_id = $2), false) AS window_closed,
                EXISTS (SELECT 1 FROM attempts a WHERE a.test_id = $1 AND a.stream_id = $2 AND a.submitted_at IS NULL
                        AND NOT a.is_preview) AS active";

/// Считает доступность разбора для сданной попытки.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn availability(
    executor: &mut PgConnection,
    test: &TestRow,
    attempt: &AttemptRow,
    now: OffsetDateTime,
) -> ApiResult<ReviewAvailability> {
    let stream: StreamState = sqlx::query_as(STREAM_STATE)
        .bind(test.id)
        .bind(attempt.stream_id)
        .fetch_one(&mut *executor)
        .await?;
    Ok(resolve(test, attempt, &stream, now))
}

fn resolve(
    test: &TestRow,
    attempt: &AttemptRow,
    stream: &StreamState,
    now: OffsetDateTime,
) -> ReviewAvailability {
    let mode = ReviewMode::parse(&test.review_policy);
    let context = Context {
        now,
        submitted: attempt.submitted_at.is_some(),
        window_closed: stream.window_closed,
        stream_has_active_attempts: stream.active,
    };
    let available = attempt.is_preview && attempt.submitted_at.is_some()
        || review::is_allowed(mode.policy(test.review_not_before), context);
    let waiting = mode == ReviewMode::AfterClose && (!stream.window_closed || stream.active);
    ReviewAvailability {
        available,
        mode,
        not_before: test.review_not_before,
        waiting_for_stream: waiting,
    }
}

/// Вариант в разборе.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ReviewOption {
    pub id: Uuid,
    pub body: String,
    pub image: Option<ImageRef>,
    pub is_correct: bool,
    pub selected: bool,
}

/// Вопрос в разборе.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ReviewQuestion {
    pub index: usize,
    pub choice: Choice,
    pub body: String,
    pub image: Option<ImageRef>,
    #[schema(value_type = String)]
    pub weight: Decimal,
    /// Балл за полностью верный ответ на этот вопрос.
    #[schema(value_type = String)]
    pub max_score: Decimal,
    /// Балл; `null` — свободный ответ («не оценивается»).
    #[schema(value_type = Option<String>)]
    pub score: Option<Decimal>,
    pub text_answer: Option<String>,
    pub options: Vec<ReviewOption>,
}

/// Разбор попытки.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ReviewView {
    pub attempt_id: Uuid,
    pub title: String,
    #[schema(value_type = Option<String>)]
    pub score: Option<Decimal>,
    #[schema(value_type = String)]
    pub max_score: Decimal,
    /// Процент от максимума — когда балл виден.
    #[schema(value_type = Option<String>)]
    pub percent: Option<Decimal>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub submitted_at: Option<OffsetDateTime>,
    pub submit_reason: Option<String>,
    pub availability: ReviewAvailability,
    /// Вопросы — только когда разбор доступен.
    pub questions: Vec<ReviewQuestion>,
}

#[derive(FromRow)]
struct AnswerRow {
    question_id: Uuid,
    selected_option_ids: Vec<Uuid>,
    text_answer: Option<String>,
    score: Option<Decimal>,
}

/// Разбор своей попытки; до разрешённого момента — только факт сдачи.
#[utoipa::path(get, path = "/api/attempts/{attempt_id}/review", tag = "attempts", params(("attempt_id" = Uuid, Path)),
    responses((status = 200, body = ReviewView), (status = 409, description = "попытка не сдана")))]
pub(crate) async fn get_review(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(attempt_id): Path<Uuid>,
) -> ApiResult<Json<ReviewView>> {
    let mut connection = state.db.acquire().await?;
    let attempt = own_attempt(&mut connection, &user, attempt_id, ReadMode::Plain).await?;
    ensure_submitted(&attempt)?;
    let test = model::load_test(&mut *connection, attempt.test_id).await?;
    let available = availability(&mut connection, &test, &attempt, state.clock.now()).await?;
    let questions = review_questions(&mut connection, &attempt, &available).await?;
    let score = attempt.score.filter(|_| available.available);
    Ok(Json(ReviewView {
        attempt_id,
        title: test.title,
        score: score.map(|shown| shown.normalize()),
        max_score: attempt.max_score.normalize(),
        percent: shown_percent(score, attempt.max_score),
        submitted_at: attempt.submitted_at,
        submit_reason: attempt.submit_reason.clone(),
        availability: available,
        questions,
    }))
}

/// Процент попытки, если её балл виден студенту.
fn shown_percent(score: Option<Decimal>, max_score: Decimal) -> Option<Decimal> {
    scoring::percent(score?, max_score).map(|percent| percent.normalize())
}

fn ensure_submitted(attempt: &AttemptRow) -> ApiResult<()> {
    if attempt.submitted_at.is_none() {
        return Err(AppError::Conflict("Попытка ещё не сдана".to_owned()));
    }
    Ok(())
}

/// Вопросы разбора; пусто, пока разбор недоступен.
///
/// cancel-safe: yes — только чтение.
async fn review_questions(
    connection: &mut PgConnection,
    attempt: &AttemptRow,
    availability: &ReviewAvailability,
) -> ApiResult<Vec<ReviewQuestion>> {
    if !availability.available {
        return Ok(Vec::new());
    }
    let questions = model::load_questions_in(connection, attempt.test_id).await?;
    let answers: Vec<AnswerRow> = sqlx::query_as(
        "SELECT question_id, selected_option_ids, text_answer, score FROM answers WHERE attempt_id = $1",
    )
    .bind(attempt.id)
    .fetch_all(&mut *connection)
    .await?;
    Ok(placed(&attempt.layout.0, &questions)
        .enumerate()
        .map(|(index, (slot, question))| review_question(index, slot, question, &answers))
        .collect())
}

fn review_question(
    index: usize,
    slot: &PlacedQuestion,
    question: &QuestionDetail,
    answers: &[AnswerRow],
) -> ReviewQuestion {
    let answer = answers.iter().find(|row| row.question_id == question.id);
    let selected = answer.map(|row| row.selected_option_ids.clone()).unwrap_or_default();
    ReviewQuestion {
        index,
        choice: Choice::from(question.kind),
        body: question.body.clone(),
        image: question.image.clone(),
        weight: question.weight,
        max_score: question.scored().max_score().normalize(),
        score: question
            .kind
            .is_scored()
            .then(|| answer.and_then(|row| row.score).unwrap_or_default().normalize()),
        text_answer: answer.and_then(|row| row.text_answer.clone()),
        options: review_options(slot, question, &selected),
    }
}

fn review_options(
    slot: &PlacedQuestion,
    question: &QuestionDetail,
    selected: &[Uuid],
) -> Vec<ReviewOption> {
    slot.option_ids
        .iter()
        .filter_map(|option_id| question.options.iter().find(|option| option.id == *option_id))
        .map(|option| ReviewOption {
            id: option.id,
            body: option.body.clone(),
            image: option.image.clone(),
            is_correct: option.is_correct,
            selected: selected.contains(&option.id),
        })
        .collect()
}

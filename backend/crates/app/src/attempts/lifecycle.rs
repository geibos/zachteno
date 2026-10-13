//! Сдача попыток и подсчёт баллов (spec test-taking, scoring; design D11, D12).

use std::collections::HashMap;

use domain::layout::PlacedQuestion;
use domain::scoring;
use rust_decimal::Decimal;
use sqlx::types::Json;
use sqlx::{FromRow, PgConnection};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::authoring::model::{self, QuestionDetail};
use crate::error::ApiResult;

/// Почему попытка сдана.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubmitReason {
    Manual,
    Timeout,
    WindowClosed,
}

impl SubmitReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Timeout => "timeout",
            Self::WindowClosed => "window_closed",
        }
    }
}

#[derive(FromRow)]
struct AttemptHead {
    test_id: Uuid,
    layout: Json<Vec<PlacedQuestion>>,
    submitted: bool,
}

#[derive(FromRow)]
struct SavedAnswer {
    question_id: Uuid,
    selected_option_ids: Vec<Uuid>,
}

/// Сдаёт попытку: считает баллы по сохранённым ответам и фиксирует их. Повторная сдача ничего не меняет.
///
/// cancel-safe: yes — в транзакции вызывающего; строка попытки блокируется.
pub(crate) async fn submit(
    tx: &mut PgConnection,
    attempt_id: Uuid,
    reason: SubmitReason,
    now: OffsetDateTime,
) -> ApiResult<bool> {
    let head = lock_head(tx, attempt_id).await?;
    if head.submitted {
        return Ok(false);
    }
    let questions = questions_by_id(tx, head.test_id).await?;
    let answers = saved_answers(tx, attempt_id).await?;
    let placed: Vec<&QuestionDetail> =
        head.layout.0.iter().filter_map(|slot| questions.get(&slot.question_id)).collect();
    let scores = score_answers(tx, attempt_id, &placed, &answers).await?;
    let max =
        scoring::max_score(&placed.iter().map(|question| question.scored()).collect::<Vec<_>>());
    finish(tx, attempt_id, (scoring::attempt_score(&scores), max), (reason, now)).await?;
    Ok(true)
}

/// Заголовок попытки с блокировкой строки до конца транзакции.
///
/// cancel-safe: yes — в транзакции вызывающего.
async fn lock_head(tx: &mut PgConnection, attempt_id: Uuid) -> ApiResult<AttemptHead> {
    Ok(sqlx::query_as(
        "SELECT test_id, layout, submitted_at IS NOT NULL AS submitted FROM attempts WHERE id = $1 FOR UPDATE",
    )
    .bind(attempt_id)
    .fetch_one(&mut *tx)
    .await?)
}

/// cancel-safe: yes — только чтение.
async fn saved_answers(tx: &mut PgConnection, attempt_id: Uuid) -> ApiResult<Vec<SavedAnswer>> {
    Ok(sqlx::query_as("SELECT question_id, selected_option_ids FROM answers WHERE attempt_id = $1")
        .bind(attempt_id)
        .fetch_all(&mut *tx)
        .await?)
}

async fn questions_by_id(
    tx: &mut PgConnection,
    test_id: Uuid,
) -> ApiResult<HashMap<Uuid, QuestionDetail>> {
    let questions = model::load_questions_in(tx, test_id).await?;
    Ok(questions.into_iter().map(|question| (question.id, question)).collect())
}

async fn score_answers(
    tx: &mut PgConnection,
    attempt_id: Uuid,
    placed: &[&QuestionDetail],
    answers: &[SavedAnswer],
) -> ApiResult<Vec<Decimal>> {
    let mut scores = Vec::new();
    for question in placed {
        let score = score_of(question, answers);
        if let Some(points) = score {
            scores.push(points);
        }
        store_score(tx, attempt_id, question.id, score).await?;
    }
    Ok(scores)
}

/// Балл за вопрос по сохранённому ответу; `None` — вопрос не оценивается.
fn score_of(question: &QuestionDetail, answers: &[SavedAnswer]) -> Option<Decimal> {
    let selected = answers
        .iter()
        .find(|answer| answer.question_id == question.id)
        .map(|answer| answer.selected_option_ids.as_slice());
    question.kind.is_scored().then(|| question.scored().score(selected.unwrap_or_default()))
}

/// cancel-safe: yes — в транзакции вызывающего.
async fn store_score(
    tx: &mut PgConnection,
    attempt_id: Uuid,
    question_id: Uuid,
    score: Option<Decimal>,
) -> ApiResult<()> {
    let _scored =
        sqlx::query("UPDATE answers SET score = $3 WHERE attempt_id = $1 AND question_id = $2")
            .bind(attempt_id)
            .bind(question_id)
            .bind(score)
            .execute(&mut *tx)
            .await?;
    Ok(())
}

async fn finish(
    tx: &mut PgConnection,
    attempt_id: Uuid,
    (score, max): (Decimal, Decimal),
    (reason, now): (SubmitReason, OffsetDateTime),
) -> ApiResult<()> {
    let _done = sqlx::query("UPDATE attempts SET submitted_at = $2, submit_reason = $3, score = $4, max_score = $5 WHERE id = $1")
        .bind(attempt_id)
        .bind(now)
        .bind(reason.as_str())
        .bind(score)
        .bind(max)
        .execute(&mut *tx)
        .await?;
    let details = serde_json::json!({ "reason": reason.as_str(), "score": score.to_string() });
    log_event(tx, attempt_id, ("submit", None), (details, now)).await
}

/// Вид события и вопрос, к которому оно относится.
pub(crate) type EventSubject = (&'static str, Option<Uuid>);

/// Пишет событие в журнал попытки.
///
/// cancel-safe: yes — в транзакции вызывающего.
pub(crate) async fn log_event(
    tx: &mut PgConnection,
    attempt_id: Uuid,
    (kind, question_id): EventSubject,
    (details, now): (serde_json::Value, OffsetDateTime),
) -> ApiResult<()> {
    let _logged = sqlx::query("INSERT INTO attempt_events (attempt_id, kind, question_id, details, at) VALUES ($1, $2, $3, $4, $5)")
        .bind(attempt_id)
        .bind(kind)
        .bind(question_id)
        .bind(details)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Сдаёт незавершённые попытки без лимита времени при закрытии окна потока.
///
/// cancel-safe: yes — в транзакции вызывающего.
pub(crate) async fn close_untimed(
    tx: &mut PgConnection,
    (test_id, stream_id): (Uuid, Uuid),
    now: OffsetDateTime,
) -> ApiResult<()> {
    let open: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM attempts WHERE test_id = $1 AND stream_id = $2 AND submitted_at IS NULL AND deadline_at IS NULL",
    )
    .bind(test_id)
    .bind(stream_id)
    .fetch_all(&mut *tx)
    .await?;
    for attempt_id in open {
        let _submitted = submit(tx, attempt_id, SubmitReason::WindowClosed, now).await?;
    }
    Ok(())
}

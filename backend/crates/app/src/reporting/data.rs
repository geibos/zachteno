//! Загрузка результатов теста: попытки, ответы, вопросы.

use std::collections::HashMap;

use domain::layout::PlacedQuestion;
use domain::scoring::{self, FinalScorePolicy};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::authoring::model::{self, QuestionDetail, TestRow};
use crate::error::ApiResult;
use crate::state::AppState;

/// Какие попытки показывать.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AttemptsMode {
    /// Одна строка на студента — итоговая попытка.
    #[default]
    Final,
    /// Все сданные попытки.
    All,
}

/// Фильтры отчётов.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ResultsQuery {
    pub stream_id: Option<Uuid>,
    #[serde(default)]
    #[param(inline)]
    pub mode: AttemptsMode,
}

/// Сданная попытка с участником.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct AttemptRecord {
    pub attempt_id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub stream_name: Option<String>,
    pub removed: bool,
    pub started_at: OffsetDateTime,
    pub submitted_at: OffsetDateTime,
    pub submit_reason: Option<String>,
    pub score: Decimal,
    pub max_score: Decimal,
    pub layout: Json<Vec<PlacedQuestion>>,
    pub has_links: bool,
}

/// Ответ из базы.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct AnswerRecord {
    pub attempt_id: Uuid,
    pub question_id: Uuid,
    pub selected_option_ids: Vec<Uuid>,
    pub text_answer: Option<String>,
    pub score: Option<Decimal>,
}

/// Ответы по паре (попытка, вопрос).
pub(crate) type AnswersByKey = HashMap<(Uuid, Uuid), AnswerRecord>;

/// Строка таблицы: попытка с номером и итогом студента.
#[derive(Debug, Clone)]
pub(crate) struct ResultRow {
    pub attempt: AttemptRecord,
    pub number: usize,
    pub final_score: Option<Decimal>,
    pub final_percent: Option<Decimal>,
}

/// Всё, что нужно таблице, CSV и статистике.
#[derive(Debug, Clone)]
pub(crate) struct Results {
    pub test: TestRow,
    pub questions: Vec<QuestionDetail>,
    pub rows: Vec<ResultRow>,
    pub answers: AnswersByKey,
}

const ATTEMPTS_QUERY: &str = "SELECT a.id AS attempt_id, a.user_id, u.display_name, u.email, s.name AS stream_name,
                coalesce(m.removed_at IS NOT NULL, false) AS removed, a.started_at, a.submitted_at, a.submit_reason,
                coalesce(a.score, 0) AS score, a.max_score, a.layout,
                EXISTS (SELECT 1 FROM identity_links l WHERE (l.user_a = a.user_id OR l.user_b = a.user_id)
                        AND l.level IN ('medium', 'high') AND l.decision IS DISTINCT FROM 'rejected') AS has_links
         FROM attempts a JOIN users u ON u.id = a.user_id
         LEFT JOIN streams s ON s.id = a.stream_id
         LEFT JOIN stream_members m ON m.stream_id = a.stream_id AND m.user_id = a.user_id
         WHERE a.test_id = $1 AND NOT a.is_preview AND a.submitted_at IS NOT NULL AND ($2::uuid IS NULL OR a.stream_id = $2)
         ORDER BY u.display_name, a.user_id, a.submitted_at";

const ANSWERS_QUERY: &str =
    "SELECT r.attempt_id, r.question_id, r.selected_option_ids, r.text_answer, r.score
         FROM answers r JOIN attempts a ON a.id = r.attempt_id
         WHERE a.test_id = $1 AND NOT a.is_preview AND a.submitted_at IS NOT NULL";

/// Загружает сданные попытки теста (без предпросмотра) с фильтрами.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn load(
    state: &AppState,
    test: TestRow,
    query: &ResultsQuery,
) -> ApiResult<Results> {
    let attempts: Vec<AttemptRecord> = sqlx::query_as(ATTEMPTS_QUERY)
        .bind(test.id)
        .bind(query.stream_id)
        .fetch_all(&state.db)
        .await?;
    let answers: Vec<AnswerRecord> =
        sqlx::query_as(ANSWERS_QUERY).bind(test.id).fetch_all(&state.db).await?;
    let questions = model::load_questions(&state.db, test.id).await?;
    let policy = FinalScorePolicy::parse(&test.final_policy).unwrap_or(FinalScorePolicy::Best);
    Ok(Results {
        test,
        questions,
        rows: select_rows(attempts, policy, query.mode),
        answers: answers
            .into_iter()
            .map(|answer| ((answer.attempt_id, answer.question_id), answer))
            .collect(),
    })
}

/// Попытки, сгруппированные по студенту подряд.
type ByUser = Vec<(Uuid, Vec<AttemptRecord>)>;

fn select_rows(
    attempts: Vec<AttemptRecord>,
    policy: FinalScorePolicy,
    mode: AttemptsMode,
) -> Vec<ResultRow> {
    let mut by_user = ByUser::new();
    for attempt in attempts {
        match by_user.last_mut() {
            Some((user_id, group)) if *user_id == attempt.user_id => group.push(attempt),
            Some(_) | None => by_user.push((attempt.user_id, vec![attempt])),
        }
    }
    by_user.into_iter().flat_map(|(_, group)| rows_for_student(group, policy, mode)).collect()
}

fn rows_for_student(
    group: Vec<AttemptRecord>,
    policy: FinalScorePolicy,
    mode: AttemptsMode,
) -> Vec<ResultRow> {
    let scores: Vec<Decimal> = group.iter().map(|attempt| attempt.score).collect();
    let final_score = scoring::final_score(policy, &scores).map(|score| score.normalize());
    let final_percent = final_percent(&group, policy);
    let chosen = match policy {
        FinalScorePolicy::Best => {
            scores.iter().enumerate().max_by_key(|(_, score)| **score).map(|(index, _)| index)
        }
        FinalScorePolicy::Last | FinalScorePolicy::Average => group.len().checked_sub(1),
    };
    group
        .into_iter()
        .enumerate()
        .filter(|(index, _)| mode == AttemptsMode::All || Some(*index) == chosen)
        .map(|(index, attempt)| ResultRow {
            attempt,
            number: index.saturating_add(1),
            final_score,
            final_percent,
        })
        .collect()
}

/// Итоговый процент студента по его сданным попыткам.
fn final_percent(group: &[AttemptRecord], policy: FinalScorePolicy) -> Option<Decimal> {
    let attempts: Vec<scoring::ScoreOfMax> =
        group.iter().map(|attempt| (attempt.score, attempt.max_score)).collect();
    scoring::final_percent(policy, &attempts).map(|percent| percent.normalize())
}

impl ResultRow {
    /// Процент попытки от её максимума.
    pub(crate) fn percent(&self) -> Option<Decimal> {
        scoring::percent(self.attempt.score, self.attempt.max_score)
            .map(|percent| percent.normalize())
    }
}

/// Ячейка вопроса.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct Cell {
    pub question_id: Uuid,
    /// Был ли вопрос в этой попытке (при случайной выборке — не всегда).
    pub in_attempt: bool,
    /// Балл; `null` для свободного ответа.
    #[schema(value_type = Option<String>)]
    pub score: Option<Decimal>,
    /// Выбранные варианты текстом (картинка без текста — «[картинка]»).
    pub selected: Vec<String>,
    pub text_answer: Option<String>,
    pub free_text: bool,
}

impl Results {
    /// Ячейки строки в порядке вопросов теста.
    pub(crate) fn cells(&self, row: &ResultRow) -> Vec<Cell> {
        self.questions
            .iter()
            .map(|question| {
                let in_attempt =
                    row.attempt.layout.0.iter().any(|slot| slot.question_id == question.id);
                let answer = self.answers.get(&(row.attempt.attempt_id, question.id));
                Cell {
                    question_id: question.id,
                    in_attempt,
                    score: (in_attempt && question.kind.is_scored()).then(|| {
                        answer.and_then(|found| found.score).unwrap_or_default().normalize()
                    }),
                    selected: answer
                        .map(|found| option_labels(question, &found.selected_option_ids))
                        .unwrap_or_default(),
                    text_answer: answer.and_then(|found| found.text_answer.clone()),
                    free_text: !question.kind.is_scored(),
                }
            })
            .collect()
    }
}

fn option_labels(question: &QuestionDetail, selected: &[Uuid]) -> Vec<String> {
    question
        .options
        .iter()
        .filter(|option| selected.contains(&option.id))
        .map(|option| {
            if option.body.trim().is_empty() {
                "[картинка]".to_owned()
            } else {
                option.body.clone()
            }
        })
        .collect()
}

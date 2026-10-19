//! Статистика по вопросам по итоговым попыткам (spec results-reporting).

use axum::Json;
use axum::extract::{Path, Query, State};
use rust_decimal::Decimal;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::session::CurrentUser;
use crate::authoring::model::QuestionDetail;
use crate::error::ApiResult;
use crate::reporting::data::{AnswerRecord, AttemptsMode, Results, ResultsQuery};
use crate::reporting::tables;
use crate::state::AppState;

/// Частота выбора варианта.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct OptionStat {
    pub option_id: Uuid,
    pub body: String,
    pub is_correct: bool,
    pub chosen: usize,
}

/// Статистика вопроса.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct QuestionStat {
    pub question_id: Uuid,
    pub number: usize,
    pub body: String,
    /// Сколько итоговых попыток содержали вопрос (размер выборки).
    pub sample: usize,
    pub answered: usize,
    pub fully_correct: usize,
    pub skipped: usize,
    #[schema(value_type = Option<String>)]
    pub average_score: Option<Decimal>,
    pub options: Vec<OptionStat>,
}

/// Статистика по вопросам теста.
#[utoipa::path(get, path = "/api/tests/{test_id}/stats", tag = "reports", params(("test_id" = Uuid, Path), ResultsQuery),
    responses((status = 200, body = [QuestionStat])))]
pub(crate) async fn question_stats(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(test_id): Path<Uuid>,
    Query(query): Query<ResultsQuery>,
) -> ApiResult<Json<Vec<QuestionStat>>> {
    let final_only = ResultsQuery { mode: AttemptsMode::Final, ..query };
    let report = tables::load_visible(&state, &user, test_id, &final_only).await?;
    Ok(Json(compute(&report)))
}

fn compute(report: &Results) -> Vec<QuestionStat> {
    (1_usize..)
        .zip(&report.questions)
        .map(|(number, question)| question_stat(report, number, question))
        .collect()
}

fn question_stat(report: &Results, number: usize, question: &QuestionDetail) -> QuestionStat {
    let answers = sampled_answers(report, question);
    let sample = answers.len();
    let given: Vec<&AnswerRecord> =
        answers.iter().flatten().copied().filter(|answer| is_given(answer)).collect();
    let scores: Vec<Decimal> = given.iter().filter_map(|answer| answer.score).collect();
    let max_score = question.scored().max_score();
    QuestionStat {
        question_id: question.id,
        number,
        body: question.body.clone(),
        sample,
        answered: given.len(),
        fully_correct: scores.iter().filter(|score| **score >= max_score).count(),
        skipped: sample.saturating_sub(given.len()),
        average_score: average(question, &scores, sample),
        options: option_stats(question, &given),
    }
}

/// Ответы итоговых попыток, в которых был вопрос (`None` — попытка без ответа на него).
fn sampled_answers<'report>(
    report: &'report Results,
    question: &QuestionDetail,
) -> Vec<Option<&'report AnswerRecord>> {
    report
        .rows
        .iter()
        .filter(|row| row.attempt.layout.0.iter().any(|slot| slot.question_id == question.id))
        .map(|row| report.answers.get(&(row.attempt.attempt_id, question.id)))
        .collect()
}

/// Дан ли ответ: выбран вариант или написан непустой текст.
fn is_given(answer: &AnswerRecord) -> bool {
    !answer.selected_option_ids.is_empty()
        || answer.text_answer.as_deref().is_some_and(|text| !text.is_empty())
}

/// Средний балл по выборке; для свободного ответа и без баллов — `None`.
fn average(question: &QuestionDetail, scores: &[Decimal], sample: usize) -> Option<Decimal> {
    (!scores.is_empty() && question.kind.is_scored()).then(|| {
        let total = scores.iter().fold(Decimal::ZERO, |acc, score| acc.saturating_add(*score));
        total.checked_div(Decimal::from(sample)).unwrap_or_default().round_dp(2).normalize()
    })
}

fn option_stats(question: &QuestionDetail, given: &[&AnswerRecord]) -> Vec<OptionStat> {
    question
        .options
        .iter()
        .map(|option| OptionStat {
            option_id: option.id,
            body: option.body.clone(),
            is_correct: option.is_correct,
            chosen: given
                .iter()
                .filter(|answer| answer.selected_option_ids.contains(&option.id))
                .count(),
        })
        .collect()
}

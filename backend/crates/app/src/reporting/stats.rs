//! Статистика по вопросам по итоговым попыткам (spec results-reporting).

use axum::Json;
use axum::extract::{Path, Query, State};
use rust_decimal::Decimal;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::session::CurrentUser;
use crate::error::ApiResult;
use crate::reporting::data::{AttemptsMode, Results, ResultsQuery};
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
    let results = tables::load_visible(&state, &user, test_id, &final_only).await?;
    Ok(Json(compute(&results)))
}

fn compute(results: &Results) -> Vec<QuestionStat> {
    (1_usize..)
        .zip(&results.questions)
        .map(|(number, question)| {
            let answers: Vec<_> = results
                .rows
                .iter()
                .filter(|row| row.attempt.layout.0.iter().any(|slot| slot.question_id == question.id))
                .map(|row| results.answers.get(&(row.attempt.attempt_id, question.id)))
                .collect();
            let sample = answers.len();
            let given: Vec<_> = answers.iter().flatten().filter(|answer| !answer.selected_option_ids.is_empty() || answer.text_answer.as_deref().is_some_and(|text| !text.is_empty())).collect();
            let scores: Vec<Decimal> = given.iter().filter_map(|answer| answer.score).collect();
            let average = (!scores.is_empty() && question.kind.is_scored()).then(|| {
                let total = scores.iter().fold(Decimal::ZERO, |acc, score| acc.saturating_add(*score));
                total.checked_div(Decimal::from(sample)).unwrap_or_default().round_dp(2).normalize()
            });
            QuestionStat {
                question_id: question.id,
                number,
                body: question.body.clone(),
                sample,
                answered: given.len(),
                fully_correct: scores.iter().filter(|score| **score >= question.weight).count(),
                skipped: sample.saturating_sub(given.len()),
                average_score: average,
                options: question
                    .options
                    .iter()
                    .map(|option| OptionStat {
                        option_id: option.id,
                        body: option.body.clone(),
                        is_correct: option.is_correct,
                        chosen: given.iter().filter(|answer| answer.selected_option_ids.contains(&option.id)).count(),
                    })
                    .collect(),
            }
        })
        .collect()
}

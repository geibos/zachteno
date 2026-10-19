//! Начисление баллов: за вопрос, за попытку и итог по тесту (spec `scoring`).

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[cfg(test)]
mod tests;

/// Тип вопроса вместе с режимом оценки множественного выбора.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    /// Одиночный выбор.
    Single,
    /// Множественный выбор, режим «только все».
    MultipleAll,
    /// Множественный выбор, режим «хотя бы один».
    MultipleAny,
    /// Множественный выбор, режим «балл за каждый верный».
    MultipleEach,
    /// Множественный выбор, режим «доля верных».
    MultipleFraction,
    /// Свободный ответ, не оценивается.
    FreeText,
}

impl QuestionKind {
    /// Все типы вопросов.
    pub const ALL: [Self; 6] = [
        Self::Single,
        Self::MultipleAll,
        Self::MultipleAny,
        Self::MultipleEach,
        Self::MultipleFraction,
        Self::FreeText,
    ];

    /// Имя в базе и API.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::MultipleAll => "multiple_all",
            Self::MultipleAny => "multiple_any",
            Self::MultipleEach => "multiple_each",
            Self::MultipleFraction => "multiple_fraction",
            Self::FreeText => "free_text",
        }
    }

    /// Разбор имени из базы.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }

    /// Оценивается ли вопрос этого типа баллами.
    #[must_use]
    pub const fn is_scored(self) -> bool {
        !matches!(self, Self::FreeText)
    }

    /// Засчитывается ли ответ частично: режимы «балл за каждый верный» и «доля верных».
    #[must_use]
    pub const fn gives_partial_credit(self) -> bool {
        matches!(self, Self::MultipleEach | Self::MultipleFraction)
    }

    /// Штраф неверного варианта по умолчанию для вопроса с весом `weight`.
    #[must_use]
    pub fn default_penalty(self, weight: Decimal) -> Decimal {
        match self {
            Self::MultipleAny => Decimal::ZERO.saturating_sub(weight),
            Self::Single
            | Self::MultipleAll
            | Self::MultipleEach
            | Self::MultipleFraction
            | Self::FreeText => Decimal::ZERO,
        }
    }
}

type OptionFilter = fn(&ScoredOption) -> bool;

/// Вариант ответа так, как его видит оценивание.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoredOption {
    /// Идентификатор варианта.
    pub option_id: Uuid,
    /// Верный ли вариант.
    pub is_correct: bool,
    /// Штраф за выбор неверного варианта (≤ 0).
    pub penalty: Decimal,
}

/// Вопрос так, как его видит оценивание.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredQuestion {
    /// Тип вопроса и режим оценки.
    pub kind: QuestionKind,
    /// Вес вопроса: балл за полностью верный ответ.
    pub weight: Decimal,
    /// Варианты ответа.
    pub options: Vec<ScoredOption>,
}

impl ScoredQuestion {
    /// Балл за ответ `selected` (идентификаторы выбранных вариантов),
    /// округлённый до двух знаков.
    ///
    /// Пустой ответ даёт 0. Неизвестные идентификаторы игнорируются.
    #[must_use]
    pub fn score(&self, selected: &[Uuid]) -> Decimal {
        if !self.kind.is_scored() || selected.is_empty() {
            return Decimal::ZERO;
        }
        let picked_correct = Decimal::from(self.count_picked(selected, |option| option.is_correct));
        let penalties = self.penalties(selected);
        let raw = match self.kind {
            QuestionKind::Single | QuestionKind::MultipleAll | QuestionKind::MultipleAny => {
                self.base_score(selected).saturating_add(penalties)
            }
            QuestionKind::MultipleEach => {
                self.weight.saturating_mul(picked_correct).saturating_add(penalties)
            }
            QuestionKind::MultipleFraction => self.fraction_score(picked_correct, penalties),
            QuestionKind::FreeText => Decimal::ZERO,
        };
        raw.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
    }

    /// «Доля верных»: W × c / C + W × P / N.
    fn fraction_score(&self, picked_correct: Decimal, penalties: Decimal) -> Decimal {
        let correct = self.correct_count();
        let wrong = Decimal::from(self.options.len().saturating_sub(correct));
        share(self.weight.saturating_mul(picked_correct), Decimal::from(correct))
            .saturating_add(share(self.weight.saturating_mul(penalties), wrong))
    }

    /// Балл за полностью верный ответ: W × число верных в режиме
    /// «балл за каждый верный», W в остальных, 0 у свободного ответа.
    #[must_use]
    pub fn max_score(&self) -> Decimal {
        match self.kind {
            QuestionKind::MultipleEach => {
                self.weight.saturating_mul(Decimal::from(self.correct_count()))
            }
            QuestionKind::Single
            | QuestionKind::MultipleAll
            | QuestionKind::MultipleAny
            | QuestionKind::MultipleFraction => self.weight,
            QuestionKind::FreeText => Decimal::ZERO,
        }
    }

    fn correct_count(&self) -> usize {
        self.options.iter().filter(|option| option.is_correct).count()
    }

    /// Балл «всё или ничего» для одиночного выбора и режимов «только все» и «хотя бы один».
    fn base_score(&self, selected: &[Uuid]) -> Decimal {
        let picked_correct = self.count_picked(selected, |option| option.is_correct);
        let picked_wrong = self.count_picked(selected, |option| !option.is_correct);
        let earned = match self.kind {
            QuestionKind::Single | QuestionKind::MultipleAll => {
                picked_correct == self.correct_count() && picked_wrong == 0
            }
            QuestionKind::MultipleAny => picked_correct > 0,
            QuestionKind::MultipleEach
            | QuestionKind::MultipleFraction
            | QuestionKind::FreeText => false,
        };
        if earned { self.weight } else { Decimal::ZERO }
    }

    fn count_picked(&self, selected: &[Uuid], predicate: OptionFilter) -> usize {
        self.selected_options(selected).filter(|option| predicate(option)).count()
    }

    fn penalties(&self, selected: &[Uuid]) -> Decimal {
        self.selected_options(selected)
            .filter(|option| !option.is_correct)
            .fold(Decimal::ZERO, |sum, option| sum.saturating_add(option.penalty))
    }

    fn selected_options<'question>(
        &'question self,
        selected: &'question [Uuid],
    ) -> impl Iterator<Item = &'question ScoredOption> {
        self.options.iter().filter(move |option| selected.contains(&option.option_id))
    }
}

/// `part / whole`; при пустом `whole` слагаемого нет.
fn share(part: Decimal, whole: Decimal) -> Decimal {
    part.checked_div(whole).unwrap_or(Decimal::ZERO)
}

/// Максимальный балл набора вопросов: сумма максимумов вопросов.
#[must_use]
pub fn max_score(questions: &[ScoredQuestion]) -> Decimal {
    questions.iter().fold(Decimal::ZERO, |sum, question| sum.saturating_add(question.max_score()))
}

/// Балл попытки: сумма баллов за вопросы, может быть отрицательной.
#[must_use]
pub fn attempt_score(question_scores: &[Decimal]) -> Decimal {
    question_scores.iter().fold(Decimal::ZERO, |acc, score| acc.saturating_add(*score))
}

/// Как выбирается итоговый балл из сданных попыток.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FinalScorePolicy {
    /// Максимальный балл.
    Best,
    /// Балл последней сданной попытки.
    Last,
    /// Среднее арифметическое, округлённое до двух знаков.
    Average,
}

impl FinalScorePolicy {
    /// Имя в базе и API.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Best => "best",
            Self::Last => "last",
            Self::Average => "average",
        }
    }

    /// Разбор имени из базы.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        [Self::Best, Self::Last, Self::Average].into_iter().find(|policy| policy.as_str() == name)
    }
}

/// Итоговый балл по баллам сданных попыток в порядке сдачи.
///
/// Без сданных попыток итогового балла нет.
#[must_use]
pub fn final_score(policy: FinalScorePolicy, attempt_scores: &[Decimal]) -> Option<Decimal> {
    match policy {
        FinalScorePolicy::Best => attempt_scores.iter().copied().max(),
        FinalScorePolicy::Last => attempt_scores.last().copied(),
        FinalScorePolicy::Average => average(attempt_scores),
    }
}

fn average(attempt_scores: &[Decimal]) -> Option<Decimal> {
    let count = Decimal::from(attempt_scores.len());
    let sum = attempt_scores.iter().fold(Decimal::ZERO, |acc, score| acc.saturating_add(*score));
    sum.checked_div(count)
        .map(|mean| mean.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero))
}

/// Балл попытки и её максимальный балл.
pub type ScoreOfMax = (Decimal, Decimal);

/// Процент балла `score` от максимума `max`, один знак после запятой.
///
/// Без положительного максимума (только свободные ответы) процента нет.
#[must_use]
pub fn percent(score: Decimal, max: Decimal) -> Option<Decimal> {
    if max <= Decimal::ZERO {
        return None;
    }
    score.saturating_mul(Decimal::ONE_HUNDRED).checked_div(max).map(round_percent)
}

fn round_percent(raw: Decimal) -> Decimal {
    raw.round_dp_with_strategy(1, RoundingStrategy::MidpointAwayFromZero)
}

fn percent_of(attempt: Option<&ScoreOfMax>) -> Option<Decimal> {
    let (score, max) = attempt?;
    percent(*score, *max)
}

/// Итоговый процент по сданным попыткам `(балл, максимум)` в порядке сдачи.
///
/// «Лучшая» — процент попытки с лучшим баллом (та же, что у `final_score`),
/// «последняя» — процент последней, «средняя» — среднее процентов попыток.
#[must_use]
pub fn final_percent(policy: FinalScorePolicy, attempts: &[ScoreOfMax]) -> Option<Decimal> {
    match policy {
        FinalScorePolicy::Best => percent_of(attempts.iter().max_by_key(|(score, _)| *score)),
        FinalScorePolicy::Last => percent_of(attempts.last()),
        FinalScorePolicy::Average => {
            let percents: Vec<Decimal> =
                attempts.iter().filter_map(|attempt| percent_of(Some(attempt))).collect();
            let sum = percents.iter().fold(Decimal::ZERO, |acc, share| acc.saturating_add(*share));
            sum.checked_div(Decimal::from(percents.len())).map(round_percent)
        }
    }
}

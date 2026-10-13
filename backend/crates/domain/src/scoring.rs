//! Начисление баллов: за вопрос, за попытку и итог по тесту (spec `scoring`).

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(test)]
mod tests;

/// Тип вопроса вместе с режимом оценки множественного выбора.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    /// Одиночный выбор.
    Single,
    /// Множественный выбор, режим «только все».
    MultipleAll,
    /// Множественный выбор, режим «хотя бы один».
    MultipleAny,
    /// Свободный ответ, не оценивается.
    FreeText,
}

impl QuestionKind {
    /// Оценивается ли вопрос этого типа баллами.
    #[must_use]
    pub const fn is_scored(self) -> bool {
        !matches!(self, Self::FreeText)
    }

    /// Штраф неверного варианта по умолчанию для вопроса с весом `weight`.
    #[must_use]
    pub fn default_penalty(self, weight: Decimal) -> Decimal {
        match self {
            Self::MultipleAny => Decimal::ZERO.saturating_sub(weight),
            Self::Single | Self::MultipleAll | Self::FreeText => Decimal::ZERO,
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
    /// Балл за ответ `selected` (идентификаторы выбранных вариантов).
    ///
    /// Пустой ответ даёт 0. Неизвестные идентификаторы игнорируются.
    #[must_use]
    pub fn score(&self, selected: &[Uuid]) -> Decimal {
        if !self.kind.is_scored() || selected.is_empty() {
            return Decimal::ZERO;
        }
        self.base_score(selected).saturating_add(self.penalties(selected))
    }

    fn base_score(&self, selected: &[Uuid]) -> Decimal {
        let picked_correct = self.count_picked(selected, |option| option.is_correct);
        let picked_wrong = self.count_picked(selected, |option| !option.is_correct);
        let total_correct = self.options.iter().filter(|option| option.is_correct).count();
        let earned = match self.kind {
            QuestionKind::Single | QuestionKind::MultipleAll => {
                picked_correct == total_correct && picked_wrong == 0
            }
            QuestionKind::MultipleAny => picked_correct > 0,
            QuestionKind::FreeText => false,
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

/// Максимальный балл набора вопросов: сумма весов оцениваемых вопросов.
#[must_use]
pub fn max_score(questions: &[ScoredQuestion]) -> Decimal {
    questions
        .iter()
        .filter(|question| question.kind.is_scored())
        .fold(Decimal::ZERO, |sum, question| sum.saturating_add(question.weight))
}

/// Балл попытки: сумма баллов за вопросы, но не меньше нуля.
#[must_use]
pub fn attempt_score(question_scores: &[Decimal]) -> Decimal {
    let sum = question_scores.iter().fold(Decimal::ZERO, |acc, score| acc.saturating_add(*score));
    sum.max(Decimal::ZERO)
}

/// Как выбирается итоговый балл из сданных попыток.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalScorePolicy {
    /// Максимальный балл.
    Best,
    /// Балл последней сданной попытки.
    Last,
    /// Среднее арифметическое, округлённое до двух знаков.
    Average,
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

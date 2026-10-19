//! Правила, без которых тест нельзя опубликовать (spec `course-authoring`).

use rust_decimal::Decimal;
use thiserror::Error;

use crate::scoring::{QuestionKind, ScoredQuestion};

#[cfg(test)]
mod tests;

/// Нарушение правил публикации для одного вопроса.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum QuestionRuleViolation {
    /// У одиночного выбора верных вариантов не ровно один.
    #[error("у вопроса с одиночным выбором должен быть ровно один верный вариант")]
    SingleNeedsExactlyOneCorrect,
    /// У множественного выбора нет верных вариантов.
    #[error("у вопроса с множественным выбором должен быть хотя бы один верный вариант")]
    MultipleNeedsCorrect,
    /// У множественного выбора меньше двух вариантов.
    #[error("у вопроса с множественным выбором должно быть не меньше двух вариантов")]
    MultipleNeedsTwoOptions,
    /// У свободного ответа есть варианты.
    #[error("у вопроса со свободным ответом не должно быть вариантов")]
    FreeTextHasOptions,
    /// Вес не положительный.
    #[error("вес вопроса должен быть больше нуля")]
    NonPositiveWeight,
    /// Штраф положительный.
    #[error("штраф варианта не может быть положительным")]
    PositivePenalty,
    /// У вопроса с выбором меньше двух вариантов.
    #[error("у вопроса с выбором должно быть не меньше двух вариантов")]
    TooFewOptions,
}

/// Проверяет вопрос на правила публикации.
///
/// # Errors
///
/// Возвращает первое найденное нарушение.
pub fn check_question(question: &ScoredQuestion) -> Result<(), QuestionRuleViolation> {
    if question.weight <= Decimal::ZERO {
        return Err(QuestionRuleViolation::NonPositiveWeight);
    }
    if question.options.iter().any(|option| option.penalty > Decimal::ZERO) {
        return Err(QuestionRuleViolation::PositivePenalty);
    }
    check_options(question.kind, question)
}

fn check_options(
    kind: QuestionKind,
    question: &ScoredQuestion,
) -> Result<(), QuestionRuleViolation> {
    let total = question.options.len();
    let correct = question.options.iter().filter(|option| option.is_correct).count();
    let violation = match kind {
        QuestionKind::FreeText => (total > 0).then_some(QuestionRuleViolation::FreeTextHasOptions),
        QuestionKind::Single => single_violation(total, correct),
        QuestionKind::MultipleAll
        | QuestionKind::MultipleAny
        | QuestionKind::MultipleEach
        | QuestionKind::MultipleFraction => multiple_violation(total, correct),
    };
    violation.map_or(Ok(()), Err)
}

const fn single_violation(total: usize, correct: usize) -> Option<QuestionRuleViolation> {
    if total < 2 {
        Some(QuestionRuleViolation::TooFewOptions)
    } else if correct == 1 {
        None
    } else {
        Some(QuestionRuleViolation::SingleNeedsExactlyOneCorrect)
    }
}

const fn multiple_violation(total: usize, correct: usize) -> Option<QuestionRuleViolation> {
    if total < 2 {
        Some(QuestionRuleViolation::MultipleNeedsTwoOptions)
    } else if correct == 0 {
        Some(QuestionRuleViolation::MultipleNeedsCorrect)
    } else {
        None
    }
}

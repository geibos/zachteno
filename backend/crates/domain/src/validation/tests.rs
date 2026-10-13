use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use super::{QuestionRuleViolation, check_question};
use crate::scoring::{QuestionKind, ScoredOption, ScoredQuestion};

fn build(kind: QuestionKind, correct: &[bool]) -> ScoredQuestion {
    let options = correct
        .iter()
        .map(|is_correct| ScoredOption {
            option_id: Uuid::new_v4(),
            is_correct: *is_correct,
            penalty: Decimal::ZERO,
        })
        .collect();
    ScoredQuestion { kind, weight: dec!(1), options }
}

#[test]
fn multiple_choice_without_correct_option_is_rejected() {
    let question = build(QuestionKind::MultipleAll, &[false, false, false]);
    assert_eq!(
        check_question(&question),
        Err(QuestionRuleViolation::MultipleNeedsCorrect),
        "нет верного"
    );
}

#[test]
fn single_choice_needs_exactly_one_correct() {
    let two = build(QuestionKind::Single, &[true, true]);
    assert_eq!(
        check_question(&two),
        Err(QuestionRuleViolation::SingleNeedsExactlyOneCorrect),
        "два верных"
    );
    let one = build(QuestionKind::Single, &[true, false]);
    assert_eq!(check_question(&one), Ok(()), "один верный");
}

#[test]
fn multiple_choice_needs_two_options() {
    let question = build(QuestionKind::MultipleAny, &[true]);
    assert_eq!(
        check_question(&question),
        Err(QuestionRuleViolation::MultipleNeedsTwoOptions),
        "один вариант"
    );
}

#[test]
fn free_text_must_not_have_options() {
    assert_eq!(check_question(&build(QuestionKind::FreeText, &[])), Ok(()), "без вариантов");
    let with_options = build(QuestionKind::FreeText, &[true]);
    assert_eq!(
        check_question(&with_options),
        Err(QuestionRuleViolation::FreeTextHasOptions),
        "с вариантами"
    );
}

#[test]
fn weight_and_penalty_signs_are_checked() {
    let mut question = build(QuestionKind::Single, &[true, false]);
    question.weight = Decimal::ZERO;
    assert_eq!(
        check_question(&question),
        Err(QuestionRuleViolation::NonPositiveWeight),
        "нулевой вес"
    );
    question.weight = dec!(1);
    if let Some(wrong) = question.options.get_mut(1) {
        wrong.penalty = dec!(0.5);
    }
    assert_eq!(
        check_question(&question),
        Err(QuestionRuleViolation::PositivePenalty),
        "положительный штраф"
    );
}

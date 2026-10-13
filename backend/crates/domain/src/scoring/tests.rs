use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use super::{
    FinalScorePolicy, QuestionKind, ScoredOption, ScoredQuestion, attempt_score, final_score,
    max_score,
};

fn question(kind: QuestionKind, weight: Decimal, correct: &[bool]) -> ScoredQuestion {
    let options = correct
        .iter()
        .map(|is_correct| ScoredOption {
            option_id: Uuid::new_v4(),
            is_correct: *is_correct,
            penalty: if *is_correct { Decimal::ZERO } else { kind.default_penalty(weight) },
        })
        .collect();
    ScoredQuestion { kind, weight, options }
}

fn ids(question: &ScoredQuestion, positions: &[usize]) -> Vec<Uuid> {
    positions.iter().filter_map(|pos| question.options.get(*pos)).map(|opt| opt.option_id).collect()
}

#[test]
fn default_weight_question_scores_its_weight_when_correct() {
    let single = question(QuestionKind::Single, dec!(2), &[true, false, false]);
    assert_eq!(single.score(&ids(&single, &[0])), dec!(2), "верный ответ даёт вес");
}

#[test]
fn single_choice_wrong_option_gives_its_penalty() {
    let mut single = question(QuestionKind::Single, dec!(1), &[true, false]);
    if let Some(wrong) = single.options.get_mut(1) {
        wrong.penalty = dec!(-0.5);
    }
    assert_eq!(single.score(&ids(&single, &[1])), dec!(-0.5), "штраф неверного варианта");
}

#[test]
fn only_all_mode_partial_answer_scores_zero() {
    let multi = question(QuestionKind::MultipleAll, dec!(1), &[true, true, false]);
    assert_eq!(multi.score(&ids(&multi, &[0])), Decimal::ZERO, "частично верно — 0");
}

#[test]
fn any_mode_selecting_everything_scores_zero_with_default_penalty() {
    let multi = question(QuestionKind::MultipleAny, dec!(1), &[true, true, false]);
    assert_eq!(multi.score(&ids(&multi, &[0, 1, 2])), Decimal::ZERO, "1 + (−1) = 0");
}

#[test]
fn any_mode_one_correct_without_mistakes_scores_weight() {
    let multi = question(QuestionKind::MultipleAny, dec!(1), &[true, true, false]);
    assert_eq!(multi.score(&ids(&multi, &[1])), dec!(1), "один верный без ошибок");
}

#[test]
fn free_text_is_excluded_from_max_score() {
    let questions = vec![
        question(QuestionKind::Single, dec!(1), &[true, false]),
        question(QuestionKind::Single, dec!(1), &[true, false]),
        question(QuestionKind::FreeText, dec!(1), &[]),
    ];
    assert_eq!(max_score(&questions), dec!(2), "свободный ответ не в максимуме");
}

#[test]
fn negative_sum_attempt_is_floored_at_zero() {
    assert_eq!(attempt_score(&[dec!(-1), dec!(-0.5)]), Decimal::ZERO, "не ниже нуля");
}

#[test]
fn average_policy_rounds_to_two_places() {
    let average = final_score(FinalScorePolicy::Average, &[dec!(4), dec!(7)]);
    assert_eq!(average, Some(dec!(5.5)), "среднее 4 и 7");
}

#[test]
fn best_and_last_policies_pick_expected_attempt() {
    let scores = [dec!(3), dec!(9), dec!(5)];
    assert_eq!(final_score(FinalScorePolicy::Best, &scores), Some(dec!(9)), "лучшая");
    assert_eq!(final_score(FinalScorePolicy::Last, &scores), Some(dec!(5)), "последняя");
}

#[test]
fn no_attempts_means_no_final_score() {
    assert_eq!(final_score(FinalScorePolicy::Best, &[]), None, "нет попыток — нет итога");
    assert_eq!(final_score(FinalScorePolicy::Average, &[]), None, "нет попыток — нет среднего");
}

fn kind_strategy() -> impl Strategy<Value = QuestionKind> {
    prop_oneof![
        Just(QuestionKind::Single),
        Just(QuestionKind::MultipleAll),
        Just(QuestionKind::MultipleAny),
    ]
}

proptest! {
    #[test]
    fn score_never_exceeds_weight(
        kind in kind_strategy(),
        weight_cents in 1_i64..10_000,
        correct in prop::collection::vec(any::<bool>(), 1..8),
        picks in prop::collection::vec(0_usize..8, 0..8),
    ) {
        let weight = Decimal::new(weight_cents, 2);
        let built = question(kind, weight, &correct);
        prop_assert!(built.score(&ids(&built, &picks)) <= weight);
    }

    #[test]
    fn empty_answer_scores_zero(kind in kind_strategy(), correct in prop::collection::vec(any::<bool>(), 1..8)) {
        let built = question(kind, dec!(1), &correct);
        prop_assert_eq!(built.score(&[]), Decimal::ZERO);
    }

    #[test]
    fn selecting_all_in_any_mode_is_not_positive_when_wrong_exists(
        correct in prop::collection::vec(any::<bool>(), 2..8),
    ) {
        prop_assume!(correct.iter().any(|flag| !flag));
        let built = question(QuestionKind::MultipleAny, dec!(1), &correct);
        let all: Vec<usize> = (0..correct.len()).collect();
        prop_assert!(built.score(&ids(&built, &all)) <= Decimal::ZERO);
    }

    #[test]
    fn attempt_score_is_never_negative(scores in prop::collection::vec(-1000_i64..1000, 0..20)) {
        let decimals: Vec<Decimal> = scores.into_iter().map(|cents| Decimal::new(cents, 2)).collect();
        prop_assert!(attempt_score(&decimals) >= Decimal::ZERO);
    }
}

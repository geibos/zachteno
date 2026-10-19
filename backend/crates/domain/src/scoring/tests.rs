use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use super::{
    FinalScorePolicy, QuestionKind, ScoredOption, ScoredQuestion, attempt_score, final_percent,
    final_score, max_score, percent,
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
fn negative_sum_attempt_stays_negative() {
    assert_eq!(attempt_score(&[dec!(-1), dec!(-0.5)]), dec!(-1.5), "без нижней границы");
}

fn with_penalty(mut built: ScoredQuestion, position: usize, penalty: Decimal) -> ScoredQuestion {
    if let Some(option) = built.options.get_mut(position) {
        option.penalty = penalty;
    }
    built
}

const THREE_AND_THREE: [bool; 6] = [true, true, true, false, false, false];

#[test]
fn each_mode_gives_weight_per_picked_correct() {
    let multi = question(QuestionKind::MultipleEach, dec!(3), &THREE_AND_THREE);
    assert_eq!(multi.score(&ids(&multi, &[0, 1, 3])), dec!(6), "2 верных × 3, штраф 0");
}

#[test]
fn each_mode_subtracts_penalty_as_is() {
    let multi = with_penalty(
        question(QuestionKind::MultipleEach, dec!(1), &[true, true, true, false]),
        3,
        dec!(-1),
    );
    assert_eq!(multi.score(&ids(&multi, &[0, 1, 3])), dec!(1), "1 × 2 + (−1)");
}

#[test]
fn fraction_mode_without_penalty_gives_share_of_weight() {
    let multi = question(QuestionKind::MultipleFraction, dec!(3), &THREE_AND_THREE);
    assert_eq!(multi.score(&ids(&multi, &[0, 1, 3])), dec!(2), "2/3 × 3");
}

#[test]
fn fraction_mode_penalty_is_scaled_by_weight_and_wrong_count() {
    let base = question(QuestionKind::MultipleFraction, dec!(3), &THREE_AND_THREE);
    let mild = with_penalty(base.clone(), 3, dec!(-1));
    assert_eq!(mild.score(&ids(&mild, &[0, 1, 3])), dec!(1), "(2/3 − 1/3) × 3");
    let dangerous = with_penalty(base, 3, dec!(-100));
    assert_eq!(dangerous.score(&ids(&dangerous, &[0, 1, 3])), dec!(-98), "(2/3 − 100/3) × 3");
}

#[test]
fn fraction_mode_without_wrong_options_ignores_penalty_term() {
    let multi = question(QuestionKind::MultipleFraction, dec!(2), &[true, true]);
    assert_eq!(multi.score(&ids(&multi, &[0, 1])), dec!(2), "все верные — полный вес");
}

#[test]
fn question_score_is_rounded_to_two_places() {
    let multi = question(QuestionKind::MultipleFraction, dec!(1), &[true, true, true, false]);
    assert_eq!(multi.score(&ids(&multi, &[0])), dec!(0.33), "треть веса");
}

#[test]
fn each_mode_max_is_weight_times_correct_count() {
    let each = question(QuestionKind::MultipleEach, dec!(1), &[true, true, true, false]);
    assert_eq!(each.max_score(), dec!(3), "вес × число верных");
    let fraction = question(QuestionKind::MultipleFraction, dec!(2), &[true, true, false]);
    assert_eq!(fraction.max_score(), dec!(2), "доля верных — максимум равен весу");
    let questions = vec![question(QuestionKind::Single, dec!(1), &[true, false]), each];
    assert_eq!(max_score(&questions), dec!(4), "сумма максимумов вопросов");
}

#[test]
fn partial_modes_default_penalty_is_zero() {
    assert_eq!(QuestionKind::MultipleEach.default_penalty(dec!(2)), Decimal::ZERO, "по баллу");
    assert_eq!(QuestionKind::MultipleFraction.default_penalty(dec!(2)), Decimal::ZERO, "доля");
}

#[test]
fn every_kind_round_trips_through_its_name() {
    for kind in QuestionKind::ALL {
        assert_eq!(QuestionKind::parse(kind.as_str()), Some(kind), "{}", kind.as_str());
    }
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

#[test]
fn percent_of_full_score_is_hundred() {
    assert_eq!(percent(dec!(93), dec!(93)), Some(dec!(100)), "93 из 93");
}

#[test]
fn percent_is_rounded_to_one_place() {
    assert_eq!(percent(dec!(2), dec!(3)), Some(dec!(66.7)), "2 из 3");
    assert_eq!(percent(dec!(87), dec!(93)), Some(dec!(93.5)), "87 из 93");
}

#[test]
fn percent_can_be_negative_and_needs_positive_max() {
    assert_eq!(percent(dec!(-96), dec!(7)), Some(dec!(-1371.4)), "отрицательный балл");
    assert_eq!(percent(dec!(0), dec!(0)), None, "только свободные ответы");
}

#[test]
fn final_percent_follows_policy() {
    let attempts = [(dec!(1), dec!(2)), (dec!(3), dec!(4)), (dec!(2), dec!(4))];
    assert_eq!(final_percent(FinalScorePolicy::Best, &attempts), Some(dec!(75)), "лучший балл");
    assert_eq!(final_percent(FinalScorePolicy::Last, &attempts), Some(dec!(50)), "последняя");
    assert_eq!(
        final_percent(FinalScorePolicy::Average, &attempts),
        Some(dec!(58.3)),
        "среднее процентов"
    );
    assert_eq!(final_percent(FinalScorePolicy::Best, &[]), None, "без попыток");
}

fn kind_strategy() -> impl Strategy<Value = QuestionKind> {
    prop_oneof![
        Just(QuestionKind::Single),
        Just(QuestionKind::MultipleAll),
        Just(QuestionKind::MultipleAny),
        Just(QuestionKind::MultipleEach),
        Just(QuestionKind::MultipleFraction),
    ]
}

proptest! {
    #[test]
    fn score_never_exceeds_question_max(
        kind in kind_strategy(),
        weight_cents in 1_i64..10_000,
        correct in prop::collection::vec(any::<bool>(), 1..8),
        picks in prop::collection::vec(0_usize..8, 0..8),
    ) {
        let weight = Decimal::new(weight_cents, 2);
        let built = question(kind, weight, &correct);
        prop_assert!(built.score(&ids(&built, &picks)) <= built.max_score());
    }

    #[test]
    fn picking_exactly_the_correct_options_scores_the_max(
        kind in kind_strategy(),
        weight_cents in 1_i64..10_000,
        correct in prop::collection::vec(any::<bool>(), 1..8),
    ) {
        prop_assume!(correct.iter().any(|flag| *flag));
        prop_assume!(kind != QuestionKind::Single || correct.iter().filter(|flag| **flag).count() == 1);
        let built = question(kind, Decimal::new(weight_cents, 2), &correct);
        let right: Vec<usize> = correct.iter().enumerate().filter(|(_, flag)| **flag).map(|(pos, _)| pos).collect();
        prop_assert_eq!(built.score(&ids(&built, &right)), built.max_score());
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
    fn attempt_score_is_the_plain_sum(scores in prop::collection::vec(-1000_i64..1000, 0..20)) {
        let cents: i64 = scores.iter().sum();
        let decimals: Vec<Decimal> = scores.into_iter().map(|value| Decimal::new(value, 2)).collect();
        prop_assert_eq!(attempt_score(&decimals), Decimal::new(cents, 2));
    }
}

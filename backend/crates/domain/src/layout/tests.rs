use std::collections::HashSet;
use std::iter::repeat_with;

use proptest::prelude::*;
use uuid::Uuid;

use super::{Settings, SourceOption, SourceQuestion, build};

fn source(question_count: usize, option_pins: &[bool]) -> Vec<SourceQuestion> {
    repeat_with(|| SourceQuestion {
        question_id: Uuid::new_v4(),
        options: option_pins
            .iter()
            .map(|pinned| SourceOption { option_id: Uuid::new_v4(), pinned: *pinned })
            .collect(),
    })
    .take(question_count)
    .collect()
}

const fn shuffled(limit: Option<usize>) -> Settings {
    Settings { questions_per_attempt: limit, shuffle_questions: true, shuffle_options: true }
}

const fn ordered(limit: Option<usize>) -> Settings {
    Settings { questions_per_attempt: limit, shuffle_questions: false, shuffle_options: false }
}

fn find(pool: &[SourceQuestion], question_id: Uuid) -> Option<&SourceQuestion> {
    pool.iter().find(|candidate| candidate.question_id == question_id)
}

#[test]
fn random_sample_takes_distinct_questions_from_pool() {
    let pool = source(800, &[false, false, false]);
    let first = build(&pool, shuffled(Some(20)), 1);
    let second = build(&pool, shuffled(Some(20)), 2);
    let unique: HashSet<Uuid> = first.iter().map(|placed| placed.question_id).collect();
    assert_eq!(first.len(), 20, "20 вопросов в попытке");
    assert_eq!(unique.len(), 20, "вопросы не повторяются");
    assert_ne!(first, second, "разные попытки получают разные выборки");
}

#[test]
fn without_shuffle_order_follows_editor() {
    let pool = source(5, &[false, false]);
    let expected: Vec<Uuid> = pool.iter().map(|question| question.question_id).collect();
    let actual: Vec<Uuid> =
        build(&pool, ordered(None), 7).iter().map(|placed| placed.question_id).collect();
    assert_eq!(actual, expected, "порядок редактора");
}

#[test]
fn pinned_last_option_stays_last() {
    let pool = source(30, &[false, false, false, true]);
    let options_shuffled = Settings { shuffle_options: true, ..ordered(None) };
    for (question, placed) in pool.iter().zip(build(&pool, options_shuffled, 3)) {
        let pinned = question.options.last().map(|opt| opt.option_id);
        assert_eq!(placed.option_ids.last().copied(), pinned, "закреплён последним");
    }
}

proptest! {
    #[test]
    fn layout_is_deterministic_for_seed(seed in any::<u64>(), limit in 1_usize..15) {
        let pool = source(12, &[false, true, false, false]);
        prop_assert_eq!(build(&pool, shuffled(Some(limit)), seed), build(&pool, shuffled(Some(limit)), seed));
    }

    #[test]
    fn pinned_options_keep_their_index(seed in any::<u64>(), pins in prop::collection::vec(any::<bool>(), 1..8)) {
        let pool = source(3, &pins);
        for placed in build(&pool, shuffled(None), seed) {
            let original = find(&pool, placed.question_id);
            prop_assert!(original.is_some());
            for (pos, option) in original.iter().flat_map(|found| found.options.iter().enumerate()) {
                if option.pinned {
                    prop_assert_eq!(placed.option_ids.get(pos), Some(&option.option_id));
                }
            }
        }
    }

    #[test]
    fn sample_has_requested_size_of_distinct_pool_questions(seed in any::<u64>(), limit in 1_usize..40) {
        let pool = source(25, &[false, false]);
        let placed = build(&pool, shuffled(Some(limit)), seed);
        let pool_ids: HashSet<Uuid> = pool.iter().map(|question| question.question_id).collect();
        let picked: HashSet<Uuid> = placed.iter().map(|question| question.question_id).collect();
        prop_assert_eq!(placed.len(), limit.min(25));
        prop_assert_eq!(picked.len(), placed.len());
        prop_assert!(picked.is_subset(&pool_ids));
    }
}

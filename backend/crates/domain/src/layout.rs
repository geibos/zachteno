//! Состав и порядок вопросов и вариантов в попытке (spec `test-taking`).
//!
//! Раскладка строится один раз при начале попытки по сохранённому `seed`
//! и потом не меняется: перезагрузка и смена устройства её не трогают.

use rand::SeedableRng as _;
use rand::rngs::StdRng;
use rand::seq::{SliceRandom as _, index};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(test)]
mod tests;

/// Вариант ответа на входе раскладки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceOption {
    /// Идентификатор варианта.
    pub option_id: Uuid,
    /// Закреплена ли позиция варианта при перемешивании.
    pub pinned: bool,
}

/// Вопрос на входе раскладки, в порядке редактора.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceQuestion {
    /// Идентификатор вопроса.
    pub question_id: Uuid,
    /// Варианты ответа в порядке редактора.
    pub options: Vec<SourceOption>,
}

/// Настройки теста, влияющие на раскладку.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Сколько вопросов в попытке; `None` — все.
    pub questions_per_attempt: Option<usize>,
    /// Перемешивать ли вопросы.
    pub shuffle_questions: bool,
    /// Перемешивать ли варианты (кроме закреплённых).
    pub shuffle_options: bool,
}

/// Вопрос попытки с итоговым порядком вариантов.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedQuestion {
    /// Идентификатор вопроса.
    pub question_id: Uuid,
    /// Варианты в порядке показа.
    pub option_ids: Vec<Uuid>,
}

/// Строит раскладку попытки детерминированно по `seed`.
#[must_use]
pub fn build(source: &[SourceQuestion], settings: Settings, seed: u64) -> Vec<PlacedQuestion> {
    let mut rng = StdRng::seed_from_u64(seed);
    pick_questions(source, settings, &mut rng)
        .into_iter()
        .map(|question| PlacedQuestion {
            question_id: question.question_id,
            option_ids: order_options(&question.options, settings, &mut rng),
        })
        .collect()
}

fn pick_questions<'source>(
    source: &'source [SourceQuestion],
    settings: Settings,
    rng: &mut StdRng,
) -> Vec<&'source SourceQuestion> {
    let wanted =
        settings.questions_per_attempt.map_or(source.len(), |limit| limit.min(source.len()));
    let mut positions: Vec<usize> = if wanted < source.len() {
        index::sample(rng, source.len(), wanted).into_vec()
    } else {
        (0..source.len()).collect()
    };
    if settings.shuffle_questions {
        positions.shuffle(rng);
    } else {
        positions.sort_unstable();
    }
    positions.iter().filter_map(|pos| source.get(*pos)).collect()
}

fn order_options(options: &[SourceOption], settings: Settings, rng: &mut StdRng) -> Vec<Uuid> {
    if !settings.shuffle_options {
        return options.iter().map(|option| option.option_id).collect();
    }
    let mut movable: Vec<Uuid> =
        options.iter().filter(|option| !option.pinned).map(|option| option.option_id).collect();
    movable.shuffle(rng);
    let mut shuffled = movable.into_iter();
    options
        .iter()
        .filter_map(|option| if option.pinned { Some(option.option_id) } else { shuffled.next() })
        .collect()
}

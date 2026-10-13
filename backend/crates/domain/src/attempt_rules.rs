//! Срок попытки и правила навигации (spec `test-taking`, `streams-enrollment`).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use time::{Duration, OffsetDateTime};

#[cfg(test)]
mod tests;

/// Режим навигации по вопросам.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Navigation {
    /// Можно возвращаться и пропускать.
    Free,
    /// По одному вопросу, без возврата и пропуска.
    Sequential,
}

/// Сдана ли попытка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptStatus {
    /// Попытка идёт.
    InProgress,
    /// Попытка сдана.
    Submitted,
}

/// Есть ли ответ на текущий вопрос.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentAnswer {
    /// Ответ сохранён.
    Present,
    /// Ответа нет.
    Missing,
}

/// Время попытки: начало и лимит теста.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Начало попытки.
    pub started_at: OffsetDateTime,
    /// Лимит времени в минутах; `None` — без лимита.
    pub limit_minutes: Option<u32>,
}

impl Timing {
    /// Срок окончания: начало плюс лимит; без лимита срока нет.
    ///
    /// Закрытие окна прохождения срок не меняет.
    #[must_use]
    pub fn deadline(self) -> Option<OffsetDateTime> {
        let minutes = self.limit_minutes?;
        Some(self.started_at.saturating_add(Duration::minutes(i64::from(minutes))))
    }
}

/// Принимает ли сервер ответ в момент `now`.
#[must_use]
pub fn accepts_answer(
    now: OffsetDateTime,
    deadline: Option<OffsetDateTime>,
    status: AttemptStatus,
) -> bool {
    status == AttemptStatus::InProgress && deadline.is_none_or(|limit| now <= limit)
}

/// Положение студента в попытке.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// Режим навигации.
    pub navigation: Navigation,
    /// Текущий вопрос (для последовательного режима).
    pub current_index: usize,
}

impl Cursor {
    /// Можно ли сохранить ответ на вопрос `question_index`.
    #[must_use]
    pub const fn can_answer(self, question_index: usize) -> bool {
        match self.navigation {
            Navigation::Free => true,
            Navigation::Sequential => question_index == self.current_index,
        }
    }

    /// Можно ли перейти к вопросу `target_index` при ответе `current` на текущий.
    #[must_use]
    pub fn can_move_to(self, target_index: usize, current: CurrentAnswer) -> bool {
        match self.navigation {
            Navigation::Free => true,
            Navigation::Sequential => {
                current == CurrentAnswer::Present
                    && target_index == self.current_index.saturating_add(1)
            }
        }
    }
}

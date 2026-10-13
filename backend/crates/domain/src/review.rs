//! Политика показа результатов (spec `test-taking`, «Просмотр результатов»).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[cfg(test)]
mod tests;

/// Когда студенту доступен разбор попытки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Policy {
    /// Только факт сдачи.
    Never,
    /// Сразу после сдачи попытки.
    AfterSubmit,
    /// После закрытия окна и завершения всех попыток потока, но не раньше даты.
    AfterClose {
        /// Дата, раньше которой разбор не открывается.
        not_before: Option<OffsetDateTime>,
    },
}

/// Состояние, от которого зависит доступ к разбору.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    /// Текущий момент.
    pub now: OffsetDateTime,
    /// Сдана ли попытка.
    pub submitted: bool,
    /// Закрыто ли окно прохождения для потока.
    pub window_closed: bool,
    /// Остались ли в потоке незавершённые попытки по тесту.
    pub stream_has_active_attempts: bool,
}

/// Доступен ли разбор попытки сейчас.
#[must_use]
pub fn is_allowed(policy: Policy, context: Context) -> bool {
    if !context.submitted {
        return false;
    }
    match policy {
        Policy::Never => false,
        Policy::AfterSubmit => true,
        Policy::AfterClose { not_before } => {
            context.window_closed
                && !context.stream_has_active_attempts
                && not_before.is_none_or(|date| context.now >= date)
        }
    }
}

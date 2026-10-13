//! Часы приложения: системные в работе, управляемые в тестах расписаний и сроков.

use std::sync::{Arc, Mutex, PoisonError};

use time::{Duration, OffsetDateTime};

/// Источник текущего времени.
#[derive(Debug, Clone, Default)]
pub(crate) enum Clock {
    /// Системное время.
    #[default]
    System,
    /// Время, которое двигают тесты.
    Manual(Arc<Mutex<OffsetDateTime>>),
}

impl Clock {
    /// Текущий момент в UTC.
    pub(crate) fn now(&self) -> OffsetDateTime {
        match self {
            Self::System => OffsetDateTime::now_utc(),
            Self::Manual(current) => *current.lock().unwrap_or_else(PoisonError::into_inner),
        }
    }

    /// Управляемые часы, стартующие с `start`.
    #[cfg(test)]
    pub(crate) fn manual(start: OffsetDateTime) -> Self {
        Self::Manual(Arc::new(Mutex::new(start)))
    }

    /// Сдвигает управляемые часы; системные не трогает.
    #[cfg(test)]
    pub(crate) fn advance(&self, step: Duration) {
        if let Self::Manual(current) = self {
            let mut guard = current.lock().unwrap_or_else(PoisonError::into_inner);
            *guard = guard.saturating_add(step);
        }
    }
}

/// Московское время (UTC+3, без перехода на летнее время).
pub(crate) const MSK_OFFSET_HOURS: i8 = 3;

/// Длительность в сутках.
pub(crate) const fn days(count: i64) -> Duration {
    Duration::days(count)
}

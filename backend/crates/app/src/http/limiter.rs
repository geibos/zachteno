//! Ограничения частоты в памяти процесса (design D4).

use std::collections::HashMap;
use std::hash::Hash;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

/// Теоретическое время следующего прихода по каждому ключу.
type Arrivals<Key> = Mutex<HashMap<Key, Instant>>;

/// GCRA: `burst` запросов подряд, дальше один раз в `interval`.
#[derive(Debug)]
pub(crate) struct Limiter<Key = IpAddr> {
    interval: Duration,
    burst: u32,
    arrivals: Arrivals<Key>,
}

impl<Key: Hash + Eq + Clone> Limiter<Key> {
    pub(crate) fn new(interval: Duration, burst: u32) -> Self {
        Self { interval, burst, arrivals: Mutex::new(HashMap::new()) }
    }

    /// `Ok(())` — запрос пропущен; `Err(wait)` — через сколько можно повторить.
    pub(crate) fn check(&self, key: &Key, now: Instant) -> Result<(), Duration> {
        let tolerance = self.interval.saturating_mul(self.burst.saturating_sub(1));
        let mut arrivals = self.arrivals.lock().unwrap_or_else(PoisonError::into_inner);
        let theoretical = arrivals.get(key).copied().unwrap_or(now).max(now);
        let allowed_at = theoretical.checked_sub(tolerance).unwrap_or(now);
        if allowed_at > now {
            return Err(allowed_at.saturating_duration_since(now));
        }
        let next = theoretical.checked_add(self.interval).unwrap_or(theoretical);
        let _previous = arrivals.insert(key.clone(), next);
        drop(arrivals);
        Ok(())
    }

    /// Забывает ключи, чьё теоретическое время уже наступило: для `check` такая запись
    /// неотличима от отсутствующей, а без чистки карта растёт с каждым новым IP.
    pub(crate) fn prune(&self, now: Instant) {
        self.arrivals
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|_, theoretical| *theoretical > now);
    }
}

/// Счётчик неудачных входов по email — одинаково для существующих и несуществующих адресов,
/// чтобы пауза не выдавала, есть ли такая учётная запись.
#[derive(Debug, Default)]
pub(crate) struct LoginFailures {
    failures: Failures,
}

/// Число неудач подряд и время последней по каждому адресу.
type Failures = Mutex<HashMap<String, (u32, Instant)>>;

/// После стольких неудач подряд включается нарастающая пауза.
pub(crate) const FREE_FAILURES: u32 = 5;
/// Первая пауза после серии неудач; дальше удваивается.
const BASE_BACKOFF: Duration = Duration::from_secs(30);
/// Верхняя граница паузы.
const MAX_BACKOFF: Duration = Duration::from_mins(15);
/// Сколько помнить серию неудач после последней из них (заведомо дольше самой длинной паузы).
pub(crate) const FORGET_AFTER: Duration = Duration::from_hours(1);

impl LoginFailures {
    /// Атомарно резервирует попытку входа: при действующей паузе — `Err(wait)`,
    /// иначе попытка сразу считается неудачной и снимается только `reset` после успеха.
    /// Так параллельные запросы не обходят счётчик (проверка и учёт под одной блокировкой).
    pub(crate) fn try_begin(&self, email: &str, now: Instant) -> Result<(), Duration> {
        let mut failures = self.failures.lock().unwrap_or_else(PoisonError::into_inner);
        let slot = failures.entry(email.to_owned()).or_insert((0, now));
        if let Some(excess) = slot.0.checked_sub(FREE_FAILURES) {
            let pause =
                BASE_BACKOFF.saturating_mul(2_u32.saturating_pow(excess.min(10))).min(MAX_BACKOFF);
            let allowed_at = slot.1.checked_add(pause).unwrap_or(slot.1);
            if allowed_at > now {
                return Err(allowed_at.saturating_duration_since(now));
            }
        }
        *slot = (slot.0.saturating_add(1), now);
        drop(failures);
        Ok(())
    }

    /// Число неудач подряд для адреса.
    pub(crate) fn count(&self, email: &str) -> u32 {
        self.failures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(email)
            .map_or(0, |slot| slot.0)
    }

    /// Сбрасывает счётчик после успешного входа.
    pub(crate) fn reset(&self, email: &str) {
        let _removed = self.failures.lock().unwrap_or_else(PoisonError::into_inner).remove(email);
    }

    /// Забывает серии, где последняя неудача старше [`FORGET_AFTER`]: без этого карта росла бы
    /// с каждым новым адресом, включая несуществующие.
    pub(crate) fn prune(&self, now: Instant) {
        self.failures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|_, slot| now.saturating_duration_since(slot.1) < FORGET_AFTER);
    }
}

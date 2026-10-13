//! Ограничения частоты в памяти процесса (design D4).

use std::collections::HashMap;
use std::hash::Hash;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

/// GCRA: `burst` запросов подряд, дальше один раз в `interval`.
#[derive(Debug)]
pub(crate) struct Limiter<Key = IpAddr> {
    interval: Duration,
    burst: u32,
    arrivals: Mutex<HashMap<Key, Instant>>,
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
        let _previous = arrivals.insert(key.clone(), theoretical + self.interval);
        Ok(())
    }
}

/// Счётчик неудачных входов по email — одинаково для существующих и несуществующих адресов,
/// чтобы пауза не выдавала, есть ли такая учётная запись.
#[derive(Debug, Default)]
pub(crate) struct LoginFailures {
    failures: Mutex<HashMap<String, (u32, Instant)>>,
}

/// После стольких неудач подряд включается нарастающая пауза.
pub(crate) const FREE_FAILURES: u32 = 5;
/// Верхняя граница паузы.
const MAX_BACKOFF: Duration = Duration::from_secs(900);

impl LoginFailures {
    /// `Err(wait)`, если для адреса действует пауза.
    pub(crate) fn check(&self, email: &str, now: Instant) -> Result<(), Duration> {
        let failures = self.failures.lock().unwrap_or_else(PoisonError::into_inner);
        let Some((count, last)) = failures.get(email).copied() else { return Ok(()) };
        let Some(excess) = count.checked_sub(FREE_FAILURES) else { return Ok(()) };
        let pause = Duration::from_secs(2_u64.saturating_pow(excess.min(10))).min(MAX_BACKOFF);
        let allowed_at = last + pause;
        if allowed_at > now { Err(allowed_at.saturating_duration_since(now)) } else { Ok(()) }
    }

    /// Учитывает неудачу; возвращает число неудач подряд.
    pub(crate) fn record_failure(&self, email: &str, now: Instant) -> u32 {
        let mut failures = self.failures.lock().unwrap_or_else(PoisonError::into_inner);
        let slot = failures.entry(email.to_owned()).or_insert((0, now));
        *slot = (slot.0.saturating_add(1), now);
        slot.0
    }

    /// Сбрасывает счётчик после успешного входа.
    pub(crate) fn reset(&self, email: &str) {
        let _removed = self.failures.lock().unwrap_or_else(PoisonError::into_inner).remove(email);
    }
}

//! Ограничение частоты по ключу (GCRA, в памяти процесса — design D4).

use std::collections::HashMap;
use std::hash::Hash;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Ограничитель: `burst` запросов подряд, дальше один раз в `interval`.
#[derive(Debug)]
pub(crate) struct Limiter<Key = IpAddr> {
    interval: Duration,
    burst: u32,
    arrivals: Mutex<HashMap<Key, Instant>>,
}

impl<Key: Hash + Eq + Copy> Limiter<Key> {
    pub(crate) fn new(interval: Duration, burst: u32) -> Self {
        Self { interval, burst, arrivals: Mutex::new(HashMap::new()) }
    }

    /// `Ok(())` — запрос пропущен; `Err(wait)` — через сколько можно повторить.
    pub(crate) fn check(&self, key: Key, now: Instant) -> Result<(), Duration> {
        let tolerance = self.interval.saturating_mul(self.burst.saturating_sub(1));
        let mut arrivals = self.arrivals.lock().unwrap_or_else(PoisonError::into_inner);
        let theoretical = arrivals.get(&key).copied().unwrap_or(now).max(now);
        let allowed_at = theoretical.checked_sub(tolerance).unwrap_or(now);
        if allowed_at > now {
            return Err(allowed_at.saturating_duration_since(now));
        }
        let _previous = arrivals.insert(key, theoretical + self.interval);
        Ok(())
    }
}

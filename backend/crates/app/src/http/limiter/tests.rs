use std::time::{Duration, Instant};

use super::{FREE_FAILURES, Limiter, LoginFailures};

#[test]
fn limiter_allows_burst_then_waits() {
    let limiter: Limiter<u8> = Limiter::new(Duration::from_secs(10), 3);
    let now = Instant::now();
    for _ in 0..3 {
        assert!(limiter.check(&1, now).is_ok(), "в пределах burst");
    }
    assert!(limiter.check(&1, now).is_err(), "сверх burst — пауза");
    assert!(limiter.check(&2, now).is_ok(), "другой ключ независим");
}

#[test]
fn failures_trigger_backoff_after_five() {
    let failures = LoginFailures::default();
    let now = Instant::now();
    for _ in 0..FREE_FAILURES {
        let _count = failures.record_failure("a@b.c", now);
    }
    assert!(failures.check("a@b.c", now).is_err(), "шестая попытка ждёт");
    failures.reset("a@b.c");
    assert!(failures.check("a@b.c", now).is_ok(), "после успеха сброс");
}

use std::time::{Duration, Instant};

use super::{FORGET_AFTER, FREE_FAILURES, Limiter, LoginFailures};

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
        assert!(failures.try_begin("a@b.c", now).is_ok(), "первые пять");
    }
    assert!(failures.try_begin("a@b.c", now).is_err(), "шестая попытка ждёт");
    failures.reset("a@b.c");
    assert!(failures.try_begin("a@b.c", now).is_ok(), "после успеха сброс");
}

#[test]
fn stale_failures_are_forgotten_and_fresh_kept() {
    let failures = LoginFailures::default();
    let start = Instant::now();
    for _ in 0..FREE_FAILURES {
        assert!(failures.try_begin("old@b.c", start).is_ok(), "серия неудач");
    }
    let later = start.checked_add(FORGET_AFTER).unwrap();
    assert!(failures.try_begin("fresh@b.c", later).is_ok(), "свежая неудача");
    failures.prune(later);
    assert_eq!(failures.count("old@b.c"), 0, "устаревшая серия забыта");
    assert_eq!(failures.count("fresh@b.c"), 1, "свежая осталась");
    assert_eq!(failures.failures.lock().unwrap().len(), 1, "в карте только свежая запись");
}

#[test]
fn limiter_prune_drops_only_expired_keys() {
    let limiter: Limiter<u8> = Limiter::new(Duration::from_secs(10), 1);
    let now = Instant::now();
    assert!(limiter.check(&1, now).is_ok(), "первый ключ");
    let later = now.checked_add(Duration::from_secs(10)).unwrap();
    assert!(limiter.check(&2, later).is_ok(), "второй ключ");
    limiter.prune(later);
    assert_eq!(limiter.arrivals.lock().unwrap().len(), 1, "истёкший ключ забыт");
    assert!(limiter.check(&2, later).is_err(), "у живого ключа пауза сохранилась");
}

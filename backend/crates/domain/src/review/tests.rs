use time::macros::datetime;

use super::{Context, Policy, is_allowed};

const SETTLED: Context = Context {
    now: datetime!(2026-10-13 12:00 +3),
    submitted: true,
    window_closed: true,
    stream_has_active_attempts: false,
};

#[test]
fn never_policy_never_shows() {
    assert!(!is_allowed(Policy::Never, SETTLED), "никогда");
}

#[test]
fn after_submit_requires_submission_only() {
    let still_open = Context { window_closed: false, stream_has_active_attempts: true, ..SETTLED };
    assert!(is_allowed(Policy::AfterSubmit, still_open), "сдано");
    assert!(!is_allowed(Policy::AfterSubmit, Context { submitted: false, ..SETTLED }), "не сдано");
}

#[test]
fn after_close_waits_for_window_and_all_attempts() {
    let policy = Policy::AfterClose { not_before: None };
    assert!(is_allowed(policy, SETTLED), "закрыто и все сдали");
    assert!(!is_allowed(policy, Context { window_closed: false, ..SETTLED }), "окно открыто");
    assert!(
        !is_allowed(policy, Context { stream_has_active_attempts: true, ..SETTLED }),
        "кто-то дорабатывает"
    );
}

#[test]
fn after_close_respects_not_before_date() {
    let later = Policy::AfterClose { not_before: Some(datetime!(2026-10-14 9:00 +3)) };
    assert!(!is_allowed(later, SETTLED), "дата ещё не наступила");
    let earlier = Policy::AfterClose { not_before: Some(datetime!(2026-10-13 9:00 +3)) };
    assert!(is_allowed(earlier, SETTLED), "дата прошла");
}

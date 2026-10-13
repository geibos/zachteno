use time::macros::datetime;

use super::{AttemptStatus, CurrentAnswer, Cursor, Navigation, Timing, accepts_answer};

#[test]
fn timed_attempt_keeps_its_deadline_after_window_close() {
    let timing = Timing { started_at: datetime!(2026-10-13 10:30 +3), limit_minutes: Some(60) };
    assert_eq!(timing.deadline(), Some(datetime!(2026-10-13 11:30 +3)), "срок 11:30");
}

#[test]
fn untimed_attempt_has_no_deadline() {
    let timing = Timing { started_at: datetime!(2026-10-13 10:30 +3), limit_minutes: None };
    assert_eq!(timing.deadline(), None, "без лимита нет срока");
}

#[test]
fn answers_after_deadline_are_rejected() {
    let limit = Some(datetime!(2026-10-13 11:30 +3));
    let running = AttemptStatus::InProgress;
    assert!(accepts_answer(datetime!(2026-10-13 11:29 +3), limit, running), "до срока");
    assert!(!accepts_answer(datetime!(2026-10-13 11:31 +3), limit, running), "после срока");
}

#[test]
fn submitted_or_untimed_attempts_follow_status() {
    let limit = Some(datetime!(2026-10-13 11:30 +3));
    let submitted = AttemptStatus::Submitted;
    assert!(!accepts_answer(datetime!(2026-10-13 11:00 +3), limit, submitted), "сданная");
    let far_future = datetime!(2030-01-01 0:00 +3);
    assert!(accepts_answer(far_future, None, AttemptStatus::InProgress), "без срока");
}

#[test]
fn sequential_mode_forbids_going_back() {
    let cursor = Cursor { navigation: Navigation::Sequential, current_index: 3 };
    assert!(!cursor.can_answer(2), "к прошлому вопросу вернуться нельзя");
    assert!(cursor.can_answer(3), "текущий вопрос можно");
    assert!(!cursor.can_move_to(4, CurrentAnswer::Missing), "без ответа дальше нельзя");
    assert!(cursor.can_move_to(4, CurrentAnswer::Present), "с ответом — на следующий");
    assert!(!cursor.can_move_to(2, CurrentAnswer::Present), "назад нельзя");
}

#[test]
fn free_mode_allows_any_question() {
    let cursor = Cursor { navigation: Navigation::Free, current_index: 0 };
    assert!(cursor.can_answer(7), "любой вопрос");
    assert!(cursor.can_move_to(0, CurrentAnswer::Missing), "любой переход");
}

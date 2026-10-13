use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use time::Duration;

use crate::sweeper;
use crate::testing::{self, Classroom};

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn joined_student_sees_open_test(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 2, json!({})).await;
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    assert_eq!(dashboard["available"][0]["test_id"], room.test_id.as_str(), "тест в доступных: {dashboard}");
    assert_eq!(dashboard["available"][0]["attempts_limit"], 1, "лимит попыток виден");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn every_invite_use_is_logged_without_duplicate_membership(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let join = format!("/api/invites/{}/join", room.invite_token);
    let (_, again) = room.student.post(&join, json!({})).await;
    assert_eq!(again["already_member"], true, "повторный переход");
    let (mut second, _) = testing::register(&state, "second@example.com").await;
    let _joined = second.post(&join, json!({})).await;
    let (_, uses) = room.methodist.get(&format!("/api/streams/{}/invite-uses", room.stream_id)).await;
    assert_eq!(uses.as_array().map(Vec::len), Some(3), "три перехода в журнале: {uses}");
    let (_, stream) = room.methodist.get(&format!("/api/streams/{}", room.stream_id)).await;
    assert_eq!(stream["members"].as_array().map(Vec::len), Some(2), "двое участников");
    let (_, filtered) = room.methodist.get(&format!("/api/streams/{}/invite-uses?user_id={}", room.stream_id, room.student_id)).await;
    assert_eq!(filtered.as_array().map(Vec::len), Some(2), "фильтр по студенту");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn invite_page_is_public(db: PgPool) {
    let state = testing::state(db);
    let room = Classroom::new(&state, 1, json!({})).await;
    let mut visitor = room.student.anonymous();
    let (status, info) = visitor.get(&format!("/api/invites/{}", room.invite_token)).await;
    assert_eq!(status, StatusCode::OK, "без входа");
    assert_eq!(info["stream_name"], "ИВТ-21", "название потока");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn removed_student_loses_access_but_keeps_attempts(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let attempt = room.start().await;
    let path = format!("/api/streams/{}/members/{}", room.stream_id, room.student_id);
    assert_eq!(room.methodist.delete(&path).await.0, StatusCode::NO_CONTENT, "исключён");
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    assert_eq!(dashboard["available"].as_array().map(Vec::len), Some(0), "тестов нет");
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM attempts WHERE id = $1::uuid")
        .bind(attempt["id"].as_str().unwrap()).fetch_one(&state.db).await.unwrap();
    assert_eq!(kept, 1, "попытка сохранилась");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn scheduled_window_opens_on_time(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    room.window(json!({ "action": "close" })).await;
    let opens = state.clock.now() + Duration::hours(1);
    room.window(json!({ "action": "schedule", "open_at": opens.format(&time::format_description::well_known::Rfc3339).unwrap() })).await;
    assert!(sweeper::run_once(&state).await.unwrap(), "проход свипера");
    let path = format!("/api/tests/{}/attempts", room.test_id);
    assert_eq!(room.student.post(&path, json!({})).await.0, StatusCode::CONFLICT, "до 10:00 закрыто");
    state.clock.advance(Duration::hours(1));
    assert!(sweeper::run_once(&state).await.unwrap(), "проход свипера");
    assert_eq!(room.student.post(&path, json!({})).await.0, StatusCode::CREATED, "открылось по расписанию");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn manual_close_is_audited(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    room.window(json!({ "action": "close" })).await;
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'window.close'").fetch_one(&state.db).await.unwrap();
    assert_eq!(audited, 1, "закрытие в аудите");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn timed_attempt_keeps_running_after_window_close(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 2, json!({ "time_limit_minutes": 60, "attempts_limit": 2 })).await;
    let attempt = room.start().await;
    room.window(json!({ "action": "close" })).await;
    state.clock.advance(Duration::minutes(30));
    let (path, body) = testing::answer_body(&attempt, 0, 0, 1);
    assert_eq!(room.student.put(&path, body).await.0, StatusCode::OK, "дорабатывает свой срок");
    let restart = format!("/api/tests/{}/attempts", room.test_id);
    let _submitted = room.student.post(&format!("/api/attempts/{}/submit", attempt["id"].as_str().unwrap()), json!({})).await;
    assert_eq!(room.student.post(&restart, json!({})).await.0, StatusCode::CONFLICT, "новую начать нельзя");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn untimed_attempt_is_submitted_when_window_closes(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let attempt = room.start().await;
    room.window(json!({ "action": "close" })).await;
    let reason: Option<String> = sqlx::query_scalar("SELECT submit_reason FROM attempts WHERE id = $1::uuid")
        .bind(attempt["id"].as_str().unwrap()).fetch_one(&state.db).await.unwrap();
    assert_eq!(reason.as_deref(), Some("window_closed"), "сдана при закрытии окна");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn scheduled_close_submits_untimed_but_not_timed(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({ "time_limit_minutes": 90 })).await;
    let attempt = room.start().await;
    let closes = state.clock.now() + Duration::minutes(10);
    room.window(json!({ "action": "schedule", "close_at": closes.format(&time::format_description::well_known::Rfc3339).unwrap() })).await;
    state.clock.advance(Duration::minutes(11));
    assert!(sweeper::run_once(&state).await.unwrap(), "проход свипера");
    let submitted: bool = sqlx::query_scalar("SELECT submitted_at IS NOT NULL FROM attempts WHERE id = $1::uuid")
        .bind(attempt["id"].as_str().unwrap()).fetch_one(&state.db).await.unwrap();
    assert!(!submitted, "попытка с лимитом не сдаётся при закрытии по расписанию");
}

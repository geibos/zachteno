use axum::http::StatusCode;
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use time::Duration;
use time::format_description::well_known::Rfc3339;

use crate::state::AppState;
use crate::sweeper;
use crate::testing::{self, Classroom};

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn joined_student_sees_open_test(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 2, json!({})).await;
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    assert_eq!(
        dashboard["available"][0]["test_id"],
        room.test_id.as_str(),
        "тест в доступных: {dashboard}"
    );
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
    let (_, uses) =
        room.methodist.get(&format!("/api/streams/{}/invite-uses", room.stream_id)).await;
    assert_eq!(uses.as_array().map(Vec::len), Some(3), "три перехода в журнале: {uses}");
    let (_, stream) = room.methodist.get(&format!("/api/streams/{}", room.stream_id)).await;
    assert_eq!(stream["members"].as_array().map(Vec::len), Some(2), "двое участников");
    let (_, filtered) = room
        .methodist
        .get(&format!("/api/streams/{}/invite-uses?user_id={}", room.stream_id, room.student_id))
        .await;
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
        .bind(attempt["id"].as_str().unwrap())
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(kept, 1, "попытка сохранилась");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn scheduled_window_opens_on_time(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    room.window(json!({ "action": "close" })).await;
    let opens = state.clock.now().saturating_add(Duration::hours(1));
    room.window(json!({ "action": "schedule", "open_at": opens.format(&Rfc3339).unwrap() })).await;
    assert!(sweeper::run_once(&state).await.unwrap(), "проход свипера");
    let path = format!("/api/tests/{}/attempts", room.test_id);
    assert_eq!(
        room.student.post(&path, json!({})).await.0,
        StatusCode::CONFLICT,
        "до 10:00 закрыто"
    );
    state.clock.advance(Duration::hours(1));
    assert!(sweeper::run_once(&state).await.unwrap(), "проход свипера");
    assert_eq!(
        room.student.post(&path, json!({})).await.0,
        StatusCode::CREATED,
        "открылось по расписанию"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn manual_close_is_audited(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    room.window(json!({ "action": "close" })).await;
    let audited: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'window.close'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(audited, 1, "закрытие в аудите");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn timed_attempt_keeps_running_after_window_close(db: PgPool) {
    let state = testing::state(db);
    let mut room =
        Classroom::new(&state, 2, json!({ "time_limit_minutes": 60, "attempts_limit": 2 })).await;
    let attempt = room.start().await;
    room.window(json!({ "action": "close" })).await;
    state.clock.advance(Duration::minutes(30));
    let (path, body) = testing::answer_body(&attempt, 0, 0, 1);
    assert_eq!(room.student.put(&path, body).await.0, StatusCode::OK, "дорабатывает свой срок");
    let restart = format!("/api/tests/{}/attempts", room.test_id);
    let _submitted = room
        .student
        .post(&format!("/api/attempts/{}/submit", attempt["id"].as_str().unwrap()), json!({}))
        .await;
    assert_eq!(
        room.student.post(&restart, json!({})).await.0,
        StatusCode::CONFLICT,
        "новую начать нельзя"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn untimed_attempt_is_submitted_when_window_closes(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let attempt = room.start().await;
    room.window(json!({ "action": "close" })).await;
    let reason: Option<String> =
        sqlx::query_scalar("SELECT submit_reason FROM attempts WHERE id = $1::uuid")
            .bind(attempt["id"].as_str().unwrap())
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(reason.as_deref(), Some("window_closed"), "сдана при закрытии окна");
}

/// Сдана ли попытка и почему.
type Submission = (bool, Option<String>);

async fn submission(state: &AppState, attempt: &Json) -> Submission {
    sqlx::query_as(
        "SELECT submitted_at IS NOT NULL, submit_reason FROM attempts WHERE id = $1::uuid",
    )
    .bind(attempt["id"].as_str().unwrap())
    .fetch_one(&state.db)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn scheduled_close_submits_untimed_but_not_timed(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({ "time_limit_minutes": 90 })).await;
    let untimed_test = room.another_test(1, json!({})).await;
    let timed = room.start().await;
    let untimed = room.start_of(&untimed_test).await;
    let closes = state.clock.now().saturating_add(Duration::minutes(10));
    let schedule = json!({ "action": "schedule", "close_at": closes.format(&Rfc3339).unwrap() });
    room.window(schedule.clone()).await;
    room.window_of(&untimed_test, schedule).await;
    state.clock.advance(Duration::minutes(11));
    assert!(sweeper::run_once(&state).await.unwrap(), "проход свипера");
    assert_eq!(
        submission(&state, &untimed).await,
        (true, Some("window_closed".to_owned())),
        "попытка без лимита сдаётся при закрытии по расписанию"
    );
    assert_eq!(
        submission(&state, &timed).await,
        (false, None),
        "попытка с лимитом не сдаётся при закрытии по расписанию"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn course_lists_its_streams(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (_, streams) =
        room.methodist.get(&format!("/api/courses/{}/streams", room.course_id)).await;
    assert_eq!(streams[0]["name"], "ИВТ-21", "поток курса: {streams}");
    assert_eq!(streams[0]["members"], 1, "один студент");
}

/// Токен из ссылки приглашения.
fn token_of(invite: &Json) -> String {
    invite["url"].as_str().unwrap().rsplit('/').next().unwrap().to_owned()
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn course_teacher_assigns_stream_and_students_see_tests(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (mut teacher, _) = testing::register(&state, "teacher@example.com").await;
    let teachers = format!("/api/courses/{}/teachers", room.course_id);
    let _assigned = room.methodist.post(&teachers, json!({ "email": "teacher@example.com" })).await;
    let stream_id = testing::id_of(teacher.post("/api/streams", json!({ "name": "ИВТ-22" })).await);
    let assign = format!("/api/streams/{stream_id}/courses");
    let course = json!({ "course_id": room.course_id });
    assert_eq!(teacher.post(&assign, course.clone()).await.0, StatusCode::NO_CONTENT, "назначил");
    let (_, invite) = teacher.post(&format!("/api/streams/{stream_id}/invites"), json!({})).await;
    let (mut student, _) = testing::register(&state, "new@example.com").await;
    let _joined =
        student.post(&format!("/api/invites/{}/join", token_of(&invite)), json!({})).await;
    let window = format!("/api/tests/{}/windows/{stream_id}", room.test_id);
    let (opened, _) = teacher.put(&window, json!({ "action": "open" })).await;
    assert_eq!(opened, StatusCode::NO_CONTENT, "преподаватель открыл окно");
    let (_, dashboard) = student.get("/api/dashboard").await;
    assert_eq!(
        dashboard["available"][0]["test_id"],
        room.test_id.as_str(),
        "тест виден: {dashboard}"
    );
    let (mut outsider, outsider_id) = testing::register(&state, "other@example.com").await;
    testing::grant(&state, outsider_id, "methodist").await;
    let foreign = testing::id_of(outsider.post("/api/streams", json!({ "name": "Чужой" })).await);
    let foreign_assign = format!("/api/streams/{foreign}/courses");
    assert_eq!(
        outsider.post(&foreign_assign, course.clone()).await.0,
        StatusCode::FORBIDDEN,
        "чужой методист не назначает"
    );
    assert_eq!(
        room.student.post(&foreign_assign, course).await.0,
        StatusCode::NOT_FOUND,
        "студенту курс как будто не существует"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn invite_log_shows_name_email_time_and_filters_by_link(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let invites = format!("/api/streams/{}/invites", room.stream_id);
    let (_, second_link) = room.methodist.post(&invites, json!({ "label": "вторая" })).await;
    state.clock.advance(Duration::minutes(5));
    let (mut late, late_id) = testing::register(&state, "late@example.com").await;
    let _joined =
        late.post(&format!("/api/invites/{}/join", token_of(&second_link)), json!({})).await;
    let uses = format!("/api/streams/{}/invite-uses", room.stream_id);
    let (_, all) = room.methodist.get(&uses).await;
    assert_eq!(all.as_array().map(Vec::len), Some(2), "два перехода: {all}");
    let link_id = second_link["id"].as_str().unwrap();
    let (_, filtered) = room.methodist.get(&format!("{uses}?link_id={link_id}")).await;
    assert_eq!(filtered.as_array().map(Vec::len), Some(1), "фильтр по ссылке: {filtered}");
    let record = &filtered[0];
    assert_eq!(
        (record["user_id"].as_str(), record["email"].as_str(), record["display_name"].as_str()),
        (Some(late_id.to_string().as_str()), Some("late@example.com"), Some("late@example.com")),
        "кто перешёл"
    );
    let used_at = state.clock.now().format(&Rfc3339).unwrap();
    assert_eq!(record["used_at"].as_str(), Some(used_at.as_str()), "дата и время перехода");
    assert_eq!(record["link_label"], "вторая", "какая ссылка");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn removed_student_is_marked_in_results_table(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let attempt = room.start().await;
    let id = attempt["id"].as_str().unwrap();
    let _done = room.student.post(&format!("/api/attempts/{id}/submit"), json!({})).await;
    let results = format!("/api/tests/{}/results", room.test_id);
    assert_eq!(room.methodist.get(&results).await.1["rows"][0]["removed"], false, "пока в потоке");
    let member = format!("/api/streams/{}/members/{}", room.stream_id, room.student_id);
    assert_eq!(room.methodist.delete(&member).await.0, StatusCode::NO_CONTENT, "исключён");
    let (_, table) = room.methodist.get(&results).await;
    assert_eq!(table["rows"].as_array().map(Vec::len), Some(1), "попытка осталась: {table}");
    assert_eq!(table["rows"][0]["removed"], true, "с пометкой «исключён»");
}

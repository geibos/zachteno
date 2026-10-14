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
    let (mut teacher, _) = testing::register_verified(&state, "teacher@example.com").await;
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

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn own_course_does_not_let_methodist_capture_foreign_stream(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (mut intruder, intruder_id) = testing::register(&state, "intruder@example.com").await;
    testing::grant(&state, intruder_id, "methodist").await;
    let own = testing::id_of(intruder.post("/api/courses", json!({ "title": "Свой" })).await);
    let assign = format!("/api/streams/{}/courses", room.stream_id);
    let (status, _) = intruder.post(&assign, json!({ "course_id": own })).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "чужой поток на свой курс не назначить");
    let stream = format!("/api/streams/{}", room.stream_id);
    assert_eq!(intruder.get(&stream).await.0, StatusCode::FORBIDDEN, "менеджером не стал");
    let (_, detail) = room.methodist.get(&stream).await;
    assert_eq!(
        detail["courses"].as_array().map(Vec::len),
        Some(1),
        "курсов у потока не прибавилось"
    );
    let (mut teacher, _) = testing::register_verified(&state, "teacher@example.com").await;
    let teachers = format!("/api/courses/{own}/teachers");
    let _assigned = intruder.post(&teachers, json!({ "email": "teacher@example.com" })).await;
    let (hidden, _) = teacher.post(&assign, json!({ "course_id": own })).await;
    assert_eq!(hidden, StatusCode::NOT_FOUND, "преподавателю своего курса чужой поток не виден");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn removed_student_cannot_rejoin_by_link(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let member = format!("/api/streams/{}/members/{}", room.stream_id, room.student_id);
    assert_eq!(room.methodist.delete(&member).await.0, StatusCode::NO_CONTENT, "исключён");
    let join = format!("/api/invites/{}/join", room.invite_token);
    let (status, error) = room.student.post(&join, json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT, "ссылка не возвращает: {error}");
    assert_eq!(
        error["message"], "Вы исключены из этого потока, обратитесь к преподавателю",
        "понятная причина"
    );
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    assert_eq!(dashboard["available"].as_array().map(Vec::len), Some(0), "тестов по-прежнему нет");
    let (_, stream) = room.methodist.get(&format!("/api/streams/{}", room.stream_id)).await;
    assert!(!stream["members"][0]["removed_at"].is_null(), "всё ещё исключён: {stream}");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn removed_student_cannot_continue_started_attempt(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let attempt = room.start().await;
    let member = format!("/api/streams/{}/members/{}", room.stream_id, room.student_id);
    assert_eq!(room.methodist.delete(&member).await.0, StatusCode::NO_CONTENT, "исключён");
    let (path, body) = testing::answer_body(&attempt, 0, 0, 1);
    assert_eq!(
        room.student.put(&path, body).await.0,
        StatusCode::NOT_FOUND,
        "ответ не сохраняется"
    );
    let id = attempt["id"].as_str().unwrap();
    let submit = format!("/api/attempts/{id}/submit");
    assert_eq!(
        room.student.post(&submit, json!({})).await.0,
        StatusCode::NOT_FOUND,
        "сдать нельзя"
    );
    let (answers, submitted): (i64, bool) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM answers WHERE attempt_id = $1::uuid),
                (SELECT submitted_at IS NOT NULL FROM attempts WHERE id = $1::uuid)",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!((answers, submitted), (0, false), "попытка не изменилась");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn manager_restores_removed_student_and_stranger_cannot(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let member = format!("/api/streams/{}/members/{}", room.stream_id, room.student_id);
    assert_eq!(room.methodist.delete(&member).await.0, StatusCode::NO_CONTENT, "исключён");
    let restore = format!("{member}/restore");
    let (mut stranger, _id) = testing::register(&state, "stranger@example.com").await;
    assert_eq!(
        stranger.post(&restore, json!({})).await.0,
        StatusCode::NOT_FOUND,
        "посторонний не возвращает"
    );
    assert_eq!(
        room.methodist.post(&restore, json!({})).await.0,
        StatusCode::NO_CONTENT,
        "возвращён"
    );
    assert_eq!(
        room.methodist.post(&restore, json!({})).await.0,
        StatusCode::NOT_FOUND,
        "уже не исключён"
    );
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    assert_eq!(dashboard["available"].as_array().map(Vec::len), Some(1), "тесты снова видны");
    let audited: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'stream.restore_member'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(audited, 1, "возврат в аудите");
}

/// Итог добавления по адресу из ответа `POST /api/streams/{id}/members`.
fn outcome<'body>(body: &'body Json, email: &str) -> &'body Json {
    body["results"]
        .as_array()
        .and_then(|list| list.iter().find(|item| item["email"] == email))
        .unwrap_or_else(|| panic!("нет итога для {email}: {body}"))
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn manager_adds_registered_students_by_email(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (mut newcomer, _newcomer_id) = testing::register_verified(&state, "new@example.com").await;
    let (_unverified, _fresh_id) = testing::register(&state, "fresh@example.com").await;
    let path = format!("/api/streams/{}/members", room.stream_id);
    let emails = json!({ "emails": [
        "New@Example.com", "fresh@example.com", "student@example.com",
        "ghost@example.com", "not-an-email", "new@example.com"
    ] });
    let (status, body) = room.methodist.post(&path, emails).await;
    assert_eq!(status, StatusCode::OK, "добавление: {body}");
    assert_eq!(body["results"].as_array().map(Vec::len), Some(5), "дубли схлопнуты: {body}");
    assert_eq!(outcome(&body, "new@example.com")["status"], "added");
    assert_eq!(outcome(&body, "new@example.com")["email_verified"], true);
    assert_eq!(outcome(&body, "fresh@example.com")["status"], "added");
    assert_eq!(
        outcome(&body, "fresh@example.com")["email_verified"],
        false,
        "неподтверждённый отмечен"
    );
    assert_eq!(outcome(&body, "student@example.com")["status"], "already_member");
    assert_eq!(outcome(&body, "ghost@example.com")["status"], "not_found");
    assert_eq!(outcome(&body, "not-an-email")["status"], "invalid");
    let (_, dashboard) = newcomer.get("/api/dashboard").await;
    assert_eq!(dashboard["available"].as_array().map(Vec::len), Some(1), "тест виден: {dashboard}");
    let created: i64 =
        sqlx::query_scalar("SELECT count(*) FROM users WHERE email_lower = 'ghost@example.com'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(created, 0, "аккаунт не создан");
    let audited: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'stream.add_members'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(audited, 1, "добавление в аудите");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn manual_add_restores_removed_student(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let member = format!("/api/streams/{}/members/{}", room.stream_id, room.student_id);
    assert_eq!(room.methodist.delete(&member).await.0, StatusCode::NO_CONTENT, "исключён");
    let path = format!("/api/streams/{}/members", room.stream_id);
    let (_, body) = room.methodist.post(&path, json!({ "emails": ["student@example.com"] })).await;
    assert_eq!(outcome(&body, "student@example.com")["status"], "restored", "{body}");
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    assert_eq!(dashboard["available"].as_array().map(Vec::len), Some(1), "тесты снова видны");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn stranger_cannot_add_members_and_list_is_limited(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (mut stranger, _id) = testing::register(&state, "stranger@example.com").await;
    let path = format!("/api/streams/{}/members", room.stream_id);
    let (status, _) = stranger.post(&path, json!({ "emails": ["stranger@example.com"] })).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "посторонний не добавляет");
    let members: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM stream_members WHERE stream_id = $1::uuid AND removed_at IS NULL",
    )
    .bind(&room.stream_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(members, 1, "состав не изменился");
    let many: Vec<String> = (0..501).map(|index| format!("s{index}@example.com")).collect();
    let (too_many, _) = room.methodist.post(&path, json!({ "emails": many })).await;
    assert_eq!(too_many, StatusCode::BAD_REQUEST, "больше 500 адресов");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn closed_test_is_listed_and_streams_are_named(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    room.window(json!({ "action": "close" })).await;
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    assert_eq!(dashboard["available"].as_array().map(Vec::len), Some(0), "начать нельзя");
    assert_eq!(
        dashboard["closed"][0]["test_id"],
        room.test_id.as_str(),
        "тест в закрытых: {dashboard}"
    );
    assert_eq!(dashboard["streams"], json!(["ИВТ-21"]), "поток назван");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn student_in_stream_without_course_sees_stream_name(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let stream_id =
        testing::id_of(room.methodist.post("/api/streams", json!({ "name": "Пустой" })).await);
    let (_, invite) = room
        .methodist
        .post(&format!("/api/streams/{stream_id}/invites"), json!({ "label": "" }))
        .await;
    let token = invite["url"].as_str().unwrap().rsplit('/').next().unwrap().to_owned();
    let (mut lonely, _id) = testing::register(&state, "lonely@example.com").await;
    let _joined = lonely.post(&format!("/api/invites/{token}/join"), json!({})).await;
    let (_, dashboard) = lonely.get("/api/dashboard").await;
    assert_eq!(dashboard["streams"], json!(["Пустой"]), "{dashboard}");
    assert_eq!(dashboard["closed"].as_array().map(Vec::len), Some(0), "тестов нет");
}

/// Строка выдачи поиска по email.
fn candidate<'body>(body: &'body Json, email: &str) -> Option<&'body Json> {
    body["users"].as_array()?.iter().find(|user| user["email"] == email)
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn manager_searches_registered_users_with_membership(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (_ivan, ivan_id) = testing::register(&state, "ivan@example.com").await;
    let (_blocked, blocked_id) = testing::register(&state, "blocked@example.com").await;
    for (sql, id) in [
        ("UPDATE users SET display_name = 'Иван Петров' WHERE id = $1", ivan_id),
        ("UPDATE users SET blocked_at = now() WHERE id = $1", blocked_id),
    ] {
        let _done = sqlx::query(sql).bind(id).execute(&state.db).await.unwrap();
    }
    let member = format!("/api/streams/{}/members/{}", room.stream_id, room.student_id);
    assert_eq!(room.methodist.delete(&member).await.0, StatusCode::NO_CONTENT, "исключён");
    let search = format!("/api/streams/{}/candidates", room.stream_id);

    let (status, by_name) =
        room.methodist.get(&format!("{search}?search=%D0%BF%D0%B5%D1%82%D1%80%D0%BE%D0%B2")).await;
    assert_eq!(status, StatusCode::OK, "{by_name}");
    assert_eq!(
        by_name["users"].as_array().map(Vec::len),
        Some(1),
        "по имени без учёта регистра: {by_name}"
    );
    assert_eq!(candidate(&by_name, "ivan@example.com").unwrap()["membership"], "none");

    let (_, by_email) = room.methodist.get(&format!("{search}?search=EXAMPLE.COM")).await;
    assert_eq!(candidate(&by_email, "student@example.com").unwrap()["membership"], "removed");
    assert!(
        candidate(&by_email, "blocked@example.com").is_none(),
        "заблокированных нет: {by_email}"
    );
    assert_eq!(by_email["more"], false);

    let (_, wildcard) = room.methodist.get(&format!("{search}?search=%25")).await;
    assert_eq!(
        wildcard["users"].as_array().map(Vec::len),
        Some(0),
        "% — обычный символ: {wildcard}"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn candidate_search_is_limited_and_private(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let _bulk = sqlx::query(
        "INSERT INTO users (email, display_name) SELECT 'bulk' || n || '@example.com', 'Bulk ' || n FROM generate_series(1, 51) n",
    )
    .execute(&state.db)
    .await
    .unwrap();
    let search = format!("/api/streams/{}/candidates", room.stream_id);
    let (_, page) = room.methodist.get(&format!("{search}?search=bulk")).await;
    assert_eq!(page["users"].as_array().map(Vec::len), Some(50), "не больше 50");
    assert_eq!(page["more"], true, "есть ещё");
    let (mut stranger, _id) = testing::register(&state, "stranger@example.com").await;
    assert_eq!(
        stranger.get(&format!("{search}?search=bulk")).await.0,
        StatusCode::NOT_FOUND,
        "посторонний"
    );
}

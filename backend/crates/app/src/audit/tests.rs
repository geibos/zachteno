use axum::http::{Method, StatusCode};
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use time::Duration;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::antifraud::drain;
use crate::state::AppState;
use crate::sweeper;
use crate::testing::{self, Classroom, Client, PASSWORD};

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn methodist_sees_only_own_course_log(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (_, own) = room.methodist.get(&format!("/api/audit?course_id={}", room.course_id)).await;
    let actions: Vec<&str> =
        own.as_array().unwrap().iter().filter_map(|record| record["action"].as_str()).collect();
    assert!(actions.contains(&"test.publish"), "публикация в журнале курса: {actions:?}");
    assert!(
        own.as_array().unwrap().iter().all(|record| record["course_id"] == room.course_id.as_str()),
        "только свой курс"
    );
    assert_eq!(
        room.methodist.get("/api/audit").await.0,
        StatusCode::FORBIDDEN,
        "весь журнал — только суперпользователю"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn audit_log_is_append_only_for_app_role(db: PgPool) {
    let state = testing::state(db);
    let _user = testing::register(&state, "a@example.com").await;
    let mut tx = state.db.begin().await.unwrap();
    let _role = sqlx::query("SET LOCAL ROLE zachteno_app").execute(&mut *tx).await.unwrap();
    let denied = sqlx::query("UPDATE audit_log SET action = 'x'").execute(&mut *tx).await;
    assert!(
        denied.is_err_and(|error| error.to_string().contains("permission denied")),
        "UPDATE запрещён роли приложения"
    );
}

/// Ожидаемая запись аудит-лога: действие, исполнитель (`None` — аноним или система),
/// есть ли объект и IP-адрес.
struct Expected {
    action: &'static str,
    actor: Option<Uuid>,
    object: bool,
    ip: bool,
}

impl Expected {
    /// Действие пользователя над объектом с его IP.
    const fn by(action: &'static str, actor: Uuid) -> Self {
        Self { action, actor: Some(actor), object: true, ip: true }
    }

    /// Действие системы (свипер): без исполнителя и IP.
    const fn system(action: &'static str) -> Self {
        Self { action, actor: None, object: true, ip: false }
    }
}

async fn user_id(state: &AppState, email: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(&state.db)
        .await
        .unwrap()
}

/// Есть ли запись с действием, исполнителем, объектом и IP, как ожидается.
async fn logged(state: &AppState, expected: &Expected) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM audit_log WHERE action = $1 AND actor_id IS NOT DISTINCT FROM $2
            AND (object_id IS NOT NULL) = $3 AND (ip IS NOT NULL) = $4)",
    )
    .bind(expected.action)
    .bind(expected.actor)
    .bind(expected.object)
    .bind(expected.ip)
    .fetch_one(&state.db)
    .await
    .unwrap()
}

/// Запрос: метод, путь, тело и ожидаемый статус.
type Call = (Method, String, Option<Json>, StatusCode);

fn post(path: impl Into<String>, body: Json, expected: StatusCode) -> Call {
    (Method::POST, path.into(), Some(body), expected)
}

/// Выполняет запросы по очереди и проверяет статусы.
async fn run(client: &mut Client, calls: Vec<Call>) {
    for (method, path, body, expected) in calls {
        let (status, answer) = client.send(method.clone(), &path, body).await;
        assert_eq!(status, expected, "{method} {path}: {answer}");
    }
}

/// Вход, неудачный вход, смена и сброс пароля студента.
async fn security_actions(state: &AppState) {
    let mut visitor = Client::new(state);
    let login = |password: &str| json!({ "email": "student@example.com", "password": password });
    let change = json!({ "current_password": PASSWORD, "new_password": "другой длинный пароль 9" });
    let forgot = json!({ "email": "student@example.com" });
    let calls = vec![
        post("/api/auth/login", login("не тот пароль 12"), StatusCode::UNAUTHORIZED),
        post("/api/auth/login", login(PASSWORD), StatusCode::OK),
        post("/api/profile/password", change, StatusCode::NO_CONTENT),
        post("/api/auth/password/forgot", forgot, StatusCode::ACCEPTED),
    ];
    run(&mut visitor, calls).await;
    let token = testing::last_mail_token(state, "reset_password").await;
    let reset = json!({ "token": token, "password": "третий длинный пароль 9" });
    run(&mut visitor, vec![post("/api/auth/password/reset", reset, StatusCode::NO_CONTENT)]).await;
}

/// Роли и блокировка; возвращает id суперпользователя.
async fn admin_actions(state: &AppState) -> Uuid {
    let (mut admin, admin_id) = testing::register(state, "admin@example.com").await;
    testing::grant(state, admin_id, "superuser").await;
    let (_, target_id) = testing::register(state, "target@example.com").await;
    let roles = format!("/api/admin/users/{target_id}/roles");
    let role = |granted: bool| json!({ "role": "methodist", "granted": granted });
    let calls = vec![
        post(&roles, role(true), StatusCode::NO_CONTENT),
        post(&roles, role(false), StatusCode::NO_CONTENT),
        post(
            format!("/api/admin/users/{target_id}/block"),
            json!({ "blocked": true }),
            StatusCode::NO_CONTENT,
        ),
    ];
    run(&mut admin, calls).await;
    trusted_registration(state, &mut admin).await;
    admin_id
}

/// Ссылка доверенной регистрации и регистрация по ней.
async fn trusted_registration(state: &AppState, admin: &mut Client) {
    let (_, link) = admin.post("/api/admin/trusted-links", json!({ "label": "методисты" })).await;
    let token = link["url"].as_str().unwrap().split("trusted=").nth(1).unwrap().to_owned();
    let body = json!({ "email": "trusted@example.com", "display_name": "Д", "password": PASSWORD, "trusted_token": token });
    run(&mut Client::new(state), vec![post("/api/auth/register", body, StatusCode::CREATED)]).await;
}

/// Преподаватель, клоны курса и теста, выгрузка CSV.
async fn course_actions(state: &AppState, room: &mut Classroom) {
    let _teacher = testing::register(state, "teacher@example.com").await;
    let csv = room.methodist.get_bytes(&format!("/api/tests/{}/results.csv", room.test_id)).await;
    assert_eq!(csv.0, StatusCode::OK, "выгрузка");
    let (course, test) = (room.course_id.clone(), room.test_id.clone());
    let calls = vec![
        post(
            format!("/api/courses/{course}/teachers"),
            json!({ "email": "teacher@example.com" }),
            StatusCode::NO_CONTENT,
        ),
        post(format!("/api/courses/{course}/clone"), json!({}), StatusCode::CREATED),
        post(
            format!("/api/tests/{test}/clone"),
            json!({ "course_id": course }),
            StatusCode::CREATED,
        ),
    ];
    run(&mut room.methodist, calls).await;
}

/// Окна вручную и по расписанию, исключение студента, архив теста и курса.
async fn closing_actions(state: &AppState, room: &mut Classroom) {
    let (course, test) = (room.course_id.clone(), room.test_id.clone());
    room.window(json!({ "action": "close" })).await;
    scheduled_window(state, room).await;
    let member = format!("/api/streams/{}/members/{}", room.stream_id, room.student_id);
    let archive = json!({ "archived": true });
    let closing = vec![
        (Method::DELETE, member, None, StatusCode::NO_CONTENT),
        post(format!("/api/tests/{test}/archive"), json!({}), StatusCode::NO_CONTENT),
        (Method::PATCH, format!("/api/courses/{course}"), Some(archive), StatusCode::NO_CONTENT),
    ];
    run(&mut room.methodist, closing).await;
}

/// Окно открывается и закрывается по расписанию свипером.
async fn scheduled_window(state: &AppState, room: &mut Classroom) {
    let opens = state.clock.now().saturating_add(Duration::minutes(5));
    let closes = opens.saturating_add(Duration::minutes(5));
    room.window(json!({ "action": "schedule", "open_at": opens.format(&Rfc3339).unwrap(), "close_at": closes.format(&Rfc3339).unwrap() }))
        .await;
    for _ in 0..2 {
        state.clock.advance(Duration::minutes(6));
        assert!(sweeper::run_once(state).await.unwrap(), "проход свипера");
    }
}

/// Связь студента с двойником, её просмотр и решение по ней.
async fn antifraud_actions(state: &AppState, room: &mut Classroom) {
    let (mut twin, twin_id) = testing::register(state, "twin@example.com").await;
    let _joined = twin.post(&format!("/api/invites/{}/join", room.invite_token), json!({})).await;
    for client in [&mut room.student, &mut twin] {
        let body = json!({ "event": "login", "marker": "same", "device": { "gpu": "g" } });
        assert_eq!(client.post("/api/signals", body).await.0, StatusCode::NO_CONTENT, "сигналы");
    }
    drain(state).await;
    let links = format!("/api/courses/{}/antifraud", room.course_id);
    assert_eq!(room.methodist.get(&links).await.0, StatusCode::OK, "просмотр связей");
    let decision = json!({ "course_id": room.course_id, "user_a": room.student_id, "user_b": twin_id, "decision": "confirmed" });
    assert_eq!(
        room.methodist.post("/api/antifraud/decisions", decision).await.0,
        StatusCode::NO_CONTENT,
        "решение"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn every_spec_action_is_audited(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let methodist = user_id(&state, "metod@example.com").await;
    let student = room.student_id;
    antifraud_actions(&state, &mut room).await;
    course_actions(&state, &mut room).await;
    closing_actions(&state, &mut room).await;
    let admin = admin_actions(&state).await;
    security_actions(&state).await;
    let trusted = user_id(&state, "trusted@example.com").await;
    let expected = [
        Expected::by("auth.login", student),
        Expected { action: "auth.login_failed", actor: None, object: false, ip: true },
        Expected::by("auth.password_change", student),
        Expected::by("auth.password_reset", student),
        Expected::by("user.block", admin),
        Expected::by("role.grant", admin),
        Expected::by("role.revoke", admin),
        Expected::by("course.teacher_assign", methodist),
        Expected::by("trusted_link.create", admin),
        Expected::by("trusted_link.use", trusted),
        Expected::by("course.create", methodist),
        Expected::by("test.create", methodist),
        Expected::by("test.publish", methodist),
        Expected::by("course.clone", methodist),
        Expected::by("test.clone", methodist),
        Expected::by("test.archive", methodist),
        Expected::by("course.archive", methodist),
        Expected::by("window.open", methodist),
        Expected::by("window.close", methodist),
        Expected::system("window.open"),
        Expected::system("window.close"),
        Expected::by("invite.create", methodist),
        Expected::by("stream.remove_member", methodist),
        Expected::by("export.csv", methodist),
        Expected::by("antifraud.view", methodist),
        // Объект решения — пара аккаунтов, она в `details` (user_a, user_b), а не в `object_id`.
        Expected { action: "antifraud.decision", actor: Some(methodist), object: false, ip: true },
    ];
    for entry in &expected {
        assert!(
            logged(&state, entry).await,
            "в аудит-логе нет «{}» от {:?} (объект: {}, IP: {})",
            entry.action,
            entry.actor,
            entry.object,
            entry.ip
        );
    }
}

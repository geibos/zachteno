use std::collections::BTreeMap;

use axum::http::{Method, StatusCode};
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::{CourseRole, Membership, require, resolve};
use crate::auth::session::{CurrentUser, Roles};
use crate::error::AppError;
use crate::state::AppState;
use crate::testing::{self, Classroom, Client};

const METHODIST: Roles = Roles { superuser: false, methodist: true };
const NO_ROLES: Roles = Roles { superuser: false, methodist: false };

fn user(roles: Roles) -> CurrentUser {
    CurrentUser {
        id: Uuid::new_v4(),
        email: String::new(),
        display_name: String::new(),
        roles,
        email_verified: true,
        trusted_registration: false,
        has_password: true,
        session_id: Uuid::nil(),
    }
}

const NOBODY: Membership = Membership { is_owner: false, is_teacher: false, is_student: false };

#[test]
fn methodist_of_other_course_only_observes() {
    let role = resolve(&user(METHODIST), &NOBODY);
    assert_eq!(role, Some(CourseRole::Observer), "чужой курс — наблюдатель");
    assert!(
        matches!(require(role, CourseRole::can_edit), Err(AppError::Forbidden)),
        "править нельзя"
    );
    assert!(require(role, CourseRole::can_view).is_ok(), "смотреть можно");
}

#[test]
fn combined_roles_take_the_strongest_on_course() {
    let teacher_and_student = Membership { is_owner: false, is_teacher: true, is_student: true };
    assert_eq!(
        resolve(&user(METHODIST), &teacher_and_student),
        Some(CourseRole::Teacher),
        "преподаватель"
    );
}

#[test]
fn student_gets_not_found_for_results() {
    let student = Membership { is_owner: false, is_teacher: false, is_student: true };
    let role = resolve(&user(NO_ROLES), &student);
    assert!(
        matches!(require(role, CourseRole::can_view), Err(AppError::NotFound)),
        "как будто нет"
    );
}

#[test]
fn stranger_has_no_role() {
    assert_eq!(resolve(&user(NO_ROLES), &NOBODY), None, "посторонний");
}

/// Участники матрицы прав, в порядке столбцов ожиданий.
const ACTORS: [&str; 7] = [
    "аноним",
    "посторонний",
    "студент",
    "преподаватель",
    "чужой методист",
    "владелец",
    "суперпользователь",
];

/// Идентификаторы объектов стенда по имени подстановки: курс A (`course`, свой для владельца)
/// и курс B (`foreign_*`, свой для чужого методиста).
type Ids = BTreeMap<&'static str, String>;

/// Строка матрицы: метод, путь, тело (`""` — без тела, `single` — вопрос) и ожидания по
/// столбцам [`ACTORS`]: `U` — 401, `H` — 404 («не найдено»), `D` — 403, `A` — доступ есть
/// (любой другой статус, в том числе 400 и 409). В пути и теле `{имя}` заменяется id из [`Ids`].
type Template = (&'static str, &'static str, &'static str, &'static str);

/// Роль × эндпоинт × свой или чужой объект.
const MATRIX: [Template; 44] = [
    // Просмотр курса A: все, кто на курсе не студент.
    ("GET", "/api/courses/{course}", "", "UHHAAAA"),
    ("GET", "/api/tests/{test}", "", "UHHAAAA"),
    ("GET", "/api/tests/{test}/results", "", "UHHAAAA"),
    ("GET", "/api/tests/{test}/results.csv", "", "UHHAAAA"),
    ("GET", "/api/tests/{test}/results.xlsx", "", "UHHAAAA"),
    ("GET", "/api/tests/{test}/stats", "", "UHHAAAA"),
    ("GET", "/api/courses/{course}/summary", "", "UHHAAAA"),
    ("GET", "/api/courses/{course}/summary.csv", "", "UHHAAAA"),
    ("GET", "/api/courses/{course}/summary.xlsx", "", "UHHAAAA"),
    ("GET", "/api/courses/{course}/streams", "", "UHHAAAA"),
    ("GET", "/api/courses/{course}/teachers", "", "UHHAAAA"),
    ("GET", "/api/tests/{test}/windows", "", "UHHAAAA"),
    ("GET", "/api/attempts/{attempt}/details", "", "UHHAAAA"),
    ("GET", "/api/courses/{course}/antifraud", "", "UHHAAAA"),
    ("POST", "/api/tests/{test}/preview", "{}", "UHHAAAA"),
    // Потоки и окна курса A: преподаватель и владелец; чужой методист — «нет доступа».
    ("PUT", "/api/tests/{test}/windows/{stream}", r#"{"action":"open"}"#, "UHHADAA"),
    ("POST", "/api/streams/{stream}/courses", r#"{"course_id":"{course}"}"#, "UHHADAA"),
    ("GET", "/api/streams/{stream}", "", "UHHADAA"),
    ("GET", "/api/streams/{stream}/invites", "", "UHHADAA"),
    ("GET", "/api/streams/{stream}/invite-uses", "", "UHHADAA"),
    ("POST", "/api/streams/{stream}/invites", r#"{"label":"x"}"#, "UHHADAA"),
    ("DELETE", "/api/streams/{stream}/members/{stranger}", "", "UHHADAA"),
    // Правка курса A и его тестов: только владелец и суперпользователь.
    ("PATCH", "/api/courses/{course}", r#"{"title":"ПДД"}"#, "UHHDDAA"),
    ("POST", "/api/courses/{course}/tests", r#"{"title":"Новый"}"#, "UHHDDAA"),
    ("POST", "/api/courses/{course}/teachers", r#"{"email":"teacher@example.com"}"#, "UHHDDAA"),
    ("DELETE", "/api/courses/{course}/teachers/{stranger}", "", "UHHDDAA"),
    ("PATCH", "/api/tests/{draft}", r#"{"title":"Черновик"}"#, "UHHDDAA"),
    ("POST", "/api/tests/{draft}/questions", "single", "UHHDDAA"),
    ("PUT", "/api/questions/{question}", "single", "UHHDDAA"),
    ("POST", "/api/tests/{test}/clone", r#"{"course_id":"{course}"}"#, "UHHDDAA"),
    ("POST", "/api/courses/{course}/clone", "{}", "UDDDAAA"),
    ("GET", "/api/audit?course_id={course}", "", "UHHDDAA"),
    // Своя попытка студента: остальным — как несуществующая.
    ("GET", "/api/attempts/{attempt}", "", "UHAHHHH"),
    ("GET", "/api/attempts/{attempt}/review", "", "UHAHHHH"),
    ("POST", "/api/attempts/{attempt}/events", r#"{"events":[]}"#, "UHAHHHH"),
    ("POST", "/api/tests/{test}/attempts", "{}", "UHAAAAA"),
    // Приглашение публично, администрирование — только суперпользователю.
    ("GET", "/api/invites/{invite}", "", "AAAAAAA"),
    ("GET", "/api/admin/users", "", "UDDDDDA"),
    ("GET", "/api/admin/trusted-links", "", "UDDDDDA"),
    // Курс B: владельцу курса A — только просмотр, преподавателю и студенту A — не найдено.
    ("GET", "/api/courses/{foreign_course}", "", "UHHHAAA"),
    ("PATCH", "/api/courses/{foreign_course}", r#"{"title":"Чужой"}"#, "UHHHADA"),
    ("GET", "/api/tests/{foreign_test}/results", "", "UHHHAAA"),
    (
        "PUT",
        "/api/tests/{foreign_test}/windows/{foreign_stream}",
        r#"{"action":"close"}"#,
        "UHHHADA",
    ),
    ("GET", "/api/streams/{foreign_stream}", "", "UHHHADA"),
];

/// Стенд: клиенты всех участников (в порядке [`ACTORS`]) и объекты двух курсов.
async fn world(state: &AppState) -> (Vec<Client>, Ids) {
    let mut room = Classroom::new(state, 1, json!({})).await;
    let mut ids = Ids::from([
        ("course", room.course_id.clone()),
        ("test", room.test_id.clone()),
        ("stream", room.stream_id.clone()),
        ("invite", room.invite_token.clone()),
        ("attempt", submitted_attempt(&mut room).await),
    ]);
    let teacher = course_teacher(state, &mut room).await;
    ids.extend(draft_with_question(&mut room).await);
    let (other, foreign) = foreign_course(state).await;
    ids.extend(foreign);
    let (stranger, stranger_id) = testing::register(state, "stranger@example.com").await;
    let _stranger = ids.insert("stranger", stranger_id.to_string());
    let (admin, admin_id) = testing::register(state, "root@example.com").await;
    testing::grant(state, admin_id, "superuser").await;
    let anonymous = room.student.anonymous();
    (vec![anonymous, stranger, room.student, teacher, other, room.methodist, admin], ids)
}

/// Сданная попытка студента.
async fn submitted_attempt(room: &mut Classroom) -> String {
    let attempt = room.start().await;
    let attempt_id = attempt["id"].as_str().unwrap().to_owned();
    let _done = room.student.post(&format!("/api/attempts/{attempt_id}/submit"), json!({})).await;
    attempt_id
}

/// Преподаватель курса A.
async fn course_teacher(state: &AppState, room: &mut Classroom) -> Client {
    let (teacher, _) = testing::register_verified(state, "teacher@example.com").await;
    let teachers = format!("/api/courses/{}/teachers", room.course_id);
    let _assigned = room.methodist.post(&teachers, json!({ "email": "teacher@example.com" })).await;
    teacher
}

/// Черновик в курсе A и его вопрос.
async fn draft_with_question(room: &mut Classroom) -> [(&'static str, String); 2] {
    let tests = format!("/api/courses/{}/tests", room.course_id);
    let draft = testing::id_of(room.methodist.post(&tests, json!({ "title": "Черновик" })).await);
    let questions = format!("/api/tests/{draft}/questions");
    let question =
        testing::id_of(room.methodist.post(&questions, testing::single_question(0)).await);
    [("draft", draft), ("question", question)]
}

/// Курс B другого методиста с тестом и потоком.
async fn foreign_course(state: &AppState) -> (Client, [(&'static str, String); 3]) {
    let (mut other, other_id) = testing::register(state, "other@example.com").await;
    testing::grant(state, other_id, "methodist").await;
    let course = testing::id_of(other.post("/api/courses", json!({ "title": "Чужой" })).await);
    let tests = format!("/api/courses/{course}/tests");
    let test = testing::id_of(other.post(&tests, json!({ "title": "Т" })).await);
    let stream = testing::id_of(other.post("/api/streams", json!({ "name": "Чужой поток" })).await);
    let assign = format!("/api/streams/{stream}/courses");
    let (assigned, _) = other.post(&assign, json!({ "course_id": course })).await;
    assert_eq!(assigned, StatusCode::NO_CONTENT, "чужой поток на чужой курс");
    (other, [("foreign_course", course), ("foreign_test", test), ("foreign_stream", stream)])
}

/// Подставляет id в шаблон пути или тела.
fn expand(template: &str, ids: &Ids) -> String {
    ids.iter()
        .fold(template.to_owned(), |text, (name, id)| text.replace(&format!("{{{name}}}"), id))
}

/// Тело запроса из шаблона.
fn body_of(template: &str, ids: &Ids) -> Option<Json> {
    match template {
        "" => None,
        "single" => Some(testing::single_question(1)),
        other => Some(serde_json::from_str(&expand(other, ids)).unwrap()),
    }
}

/// Те же id объектов, но несуществующие (пользователь и приглашение — прежние).
fn phantom(ids: &Ids) -> Ids {
    let missing = Uuid::new_v4().to_string();
    ids.iter()
        .map(|(name, id)| {
            let kept = matches!(*name, "stranger" | "invite");
            (*name, if kept { id.clone() } else { missing.clone() })
        })
        .collect()
}

fn matches(expected: char, status: StatusCode) -> bool {
    match expected {
        'U' => status == StatusCode::UNAUTHORIZED,
        'H' => status == StatusCode::NOT_FOUND,
        'D' => status == StatusCode::FORBIDDEN,
        _ => ![StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN, StatusCode::NOT_FOUND]
            .contains(&status),
    }
}

/// Запрос по шаблону к объектам `ids`.
async fn call(
    client: &mut Client,
    (verb, path, body, _): &Template,
    ids: &Ids,
) -> (StatusCode, Json) {
    let method = Method::from_bytes(verb.as_bytes()).unwrap();
    client.send(method, &expand(path, ids), body_of(body, ids)).await
}

/// Проверяет строку для всех участников; расхождения складывает в `failures`.
/// Где ждём «не найдено», ответ сверяется с ответом про несуществующие объекты.
async fn check(
    actors: &mut [Client],
    (ids, ghosts): (&Ids, &Ids),
    row: &Template,
    failures: &mut Vec<String>,
) {
    for ((client, actor), mark) in actors.iter_mut().zip(ACTORS).zip(row.3.chars()) {
        let answer = call(client, row, ids).await;
        if !matches(mark, answer.0) {
            failures.push(format!("{actor}: {} {} → {}, ждали {mark}", row.0, row.1, answer.0));
        }
        if mark == 'H' && call(client, row, ghosts).await != answer {
            failures.push(format!("{actor}: {} {} отличим от несуществующего", row.0, row.1));
        }
    }
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn role_endpoint_object_matrix(db: PgPool) {
    let state = testing::state(db);
    let (mut actors, ids) = world(&state).await;
    let ghosts = phantom(&ids);
    let mut failures = Vec::new();
    for row in &MATRIX {
        check(&mut actors, (&ids, &ghosts), row, &mut failures).await;
    }
    assert!(failures.is_empty(), "расхождения с матрицей прав:\n{}", failures.join("\n"));
}

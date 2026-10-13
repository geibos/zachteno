use axum::http::StatusCode;
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use time::Duration;
use uuid::Uuid;

use crate::antifraud::drain;
use crate::state::AppState;
use crate::sweeper;
use crate::testing::{self, Classroom, Client};

fn device(gpu: &str) -> Json {
    json!({ "gpu": gpu, "screen": "1920x1080", "cores": "8", "fonts": "a,b,c", "timezone": "Europe/Moscow", "os": "macOS 15" })
}

async fn signal(client: &mut Client, marker: &str, device_fields: Json) {
    let body = json!({ "event": "login", "marker": marker, "browser": { "ua": "x" }, "device": device_fields, "timezone": "Europe/Moscow" });
    let (status, error) = client.post("/api/signals", body).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "сигналы: {error}");
}

async fn level(state: &AppState, one: Uuid, two: Uuid) -> Option<String> {
    drain(state).await;
    let (user_a, user_b) = if one < two { (one, two) } else { (two, one) };
    sqlx::query_scalar("SELECT level FROM identity_links WHERE user_a = $1 AND user_b = $2")
        .bind(user_a)
        .bind(user_b)
        .fetch_optional(&state.db)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn shared_browser_marker_is_high(db: PgPool) {
    let state = testing::state(db);
    let (mut first, first_id) = testing::register(&state, "a@example.com").await;
    let (mut second, second_id) = testing::register(&state, "b@example.com").await;
    signal(&mut first, "marker-1", device("gpu-a")).await;
    signal(&mut second, "marker-1", device("gpu-b")).await;
    assert_eq!(
        level(&state, first_id, second_id).await.as_deref(),
        Some("high"),
        "один браузер — высокий"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn same_device_in_other_browser_is_linked(db: PgPool) {
    let state = testing::state(db);
    let (mut first, first_id) = testing::register(&state, "a@example.com").await;
    let (mut second, second_id) = testing::register(&state, "b@example.com").await;
    signal(&mut first, "marker-chrome", device("apple-m3")).await;
    signal(&mut second, "marker-firefox", device("apple-m3")).await;
    assert_eq!(
        level(&state, first_id, second_id).await.as_deref(),
        Some("medium"),
        "редкое общее устройство"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn common_device_fingerprint_is_low(db: PgPool) {
    let state = testing::state(db);
    let mut ids = Vec::new();
    for index in 0..5 {
        let (mut client, user_id) =
            testing::register(&state, &format!("class{index}@example.com")).await;
        signal(&mut client, &format!("marker-{index}"), device("lab-gpu")).await;
        ids.push(user_id);
    }
    assert_eq!(
        level(&state, ids[0], ids[1]).await.as_deref(),
        Some("low"),
        "компьютерный класс — низкий"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn identical_free_text_links_different_devices(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 0, json!({})).await;
    let (mut other, other_id) = testing::register(&state, "twin@example.com").await;
    let _joined = other.post(&format!("/api/invites/{}/join", room.invite_token), json!({})).await;
    signal(&mut room.student, "marker-1", device("gpu-1")).await;
    signal(&mut other, "marker-2", device("gpu-2")).await;
    let _answers = sqlx::query(
        "WITH q AS (INSERT INTO questions (test_id, position, kind, body) VALUES ($1::uuid, 99, 'free_text', '?') RETURNING id),
              a1 AS (INSERT INTO attempts (test_id, user_id, layout, submitted_at) SELECT $1::uuid, $2, '[]', now() RETURNING id),
              a2 AS (INSERT INTO attempts (test_id, user_id, layout, submitted_at) SELECT $1::uuid, $3, '[]', now() RETURNING id)
         INSERT INTO answers (attempt_id, question_id, text_answer)
         SELECT a1.id, q.id, 'Пропустить пешехода и только потом поворачивать' FROM a1, q
         UNION ALL SELECT a2.id, q.id, '  пропустить   пешехода и только потом поворачивать ' FROM a2, q",
    )
    .bind(&room.test_id).bind(room.student_id).bind(other_id).execute(&state.db).await.unwrap();
    assert_eq!(
        level(&state, room.student_id, other_id).await.as_deref(),
        Some("medium"),
        "по поведенческим сигналам"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn high_link_blocks_nothing(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (mut twin, twin_id) = testing::register(&state, "twin@example.com").await;
    signal(&mut room.student, "same", device("gpu")).await;
    signal(&mut twin, "same", device("gpu")).await;
    assert_eq!(
        level(&state, room.student_id, twin_id).await.as_deref(),
        Some("high"),
        "связь есть"
    );
    let attempt = room.start().await;
    let (path, body) = testing::answer_body(&attempt, 0, 0, 1);
    assert_eq!(room.student.put(&path, body).await.0, StatusCode::OK, "ответ сохраняется");
    let (_, review) = {
        let id = attempt["id"].as_str().unwrap();
        let _done = room.student.post(&format!("/api/attempts/{id}/submit"), json!({})).await;
        room.student.get(&format!("/api/attempts/{id}/review")).await
    };
    assert_eq!(review["score"], "1", "балл не изменился");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn reviewer_rejects_link_and_flag_disappears(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (mut twin, twin_id) = testing::register(&state, "twin@example.com").await;
    let _joined = twin.post(&format!("/api/invites/{}/join", room.invite_token), json!({})).await;
    signal(&mut room.student, "same", device("gpu")).await;
    signal(&mut twin, "same", device("gpu")).await;
    let _level = level(&state, room.student_id, twin_id).await;
    let (_, links) =
        room.methodist.get(&format!("/api/courses/{}/antifraud", room.course_id)).await;
    assert_eq!(links["links"].as_array().map(Vec::len), Some(1), "связь в разделе курса");
    assert!(
        !links.to_string().contains("marker_hash") && !links.to_string().contains("same"),
        "сырых значений нет"
    );
    let decision = json!({ "course_id": room.course_id, "user_a": room.student_id, "user_b": twin_id, "decision": "rejected", "comment": "соседи по комнате" });
    assert_eq!(
        room.methodist.post("/api/antifraud/decisions", decision).await.0,
        StatusCode::NO_CONTENT,
        "решение"
    );
    let (_, after) =
        room.methodist.get(&format!("/api/courses/{}/antifraud", room.course_id)).await;
    assert_eq!(after["links"][0]["decision"], "rejected", "решение сохранено");
    assert_eq!(
        after["clusters"].as_array().map(Vec::len),
        Some(0),
        "отклонённая связь не в кластерах"
    );
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action IN ('antifraud.view', 'antifraud.decision')",
    )
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(audited, 3, "просмотры и решение в аудите");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn expired_signals_are_removed_but_links_remain(db: PgPool) {
    let state = testing::state(db);
    let (mut first, first_id) = testing::register(&state, "a@example.com").await;
    let (mut second, second_id) = testing::register(&state, "b@example.com").await;
    signal(&mut first, "m", device("g")).await;
    signal(&mut second, "m", device("g")).await;
    let _level = level(&state, first_id, second_id).await;
    state.clock.advance(Duration::days(366));
    assert!(sweeper::run_once(&state).await.unwrap(), "проход свипера");
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM signals").fetch_one(&state.db).await.unwrap();
    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM identity_links")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!((left, links), (0, 1), "сигналы удалены, связь осталась");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn raw_component_values_are_not_stored(db: PgPool) {
    let state = testing::state(db);
    let (mut client, _id) = testing::register(&state, "a@example.com").await;
    signal(&mut client, "very-unique-marker", device("NVIDIA RTX 4090")).await;
    let dump: String = sqlx::query_scalar("SELECT row_to_json(s)::text FROM signals s")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert!(!dump.contains("RTX") && !dump.contains("very-unique-marker"), "только хеши: {dump}");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn teacher_cannot_decide_on_strangers_links(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (mut first, first_id) = testing::register(&state, "x@example.com").await;
    let (mut second, second_id) = testing::register(&state, "y@example.com").await;
    signal(&mut first, "m", device("g")).await;
    signal(&mut second, "m", device("g")).await;
    let _level = level(&state, first_id, second_id).await;
    let decision = json!({ "course_id": room.course_id, "user_a": first_id, "user_b": second_id, "decision": "rejected" });
    assert_eq!(
        room.methodist.post("/api/antifraud/decisions", decision).await.0,
        StatusCode::NOT_FOUND,
        "связь не из этого курса"
    );
}

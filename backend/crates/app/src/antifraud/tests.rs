use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use time::Duration;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::antifraud::asn::AsnTable;
use crate::antifraud::drain;
use crate::state::AppState;
use crate::sweeper;
use crate::testing::{self, Classroom, Client};

/// Отпечаток устройства; разные `gpu` — разные компьютеры (у них отличается и экран).
fn device(gpu: &str) -> Json {
    json!({ "gpu": gpu, "screen": format!("1920x1080 {gpu}"), "cores": "8", "fonts": "a,b,c", "timezone": "Europe/Moscow", "os": "macOS" })
}

/// Тот же компьютер, каким его видит другой браузер: строка GPU и число шрифтов другие,
/// аппаратное ядро (экран, процессор, ОС, часовой пояс) — то же.
fn same_machine(browser: &str) -> Json {
    json!({ "webgl_renderer": format!("gpu as seen by {browser}"), "fonts": browser, "fonts_count": browser.len().to_string(),
            "screen": "2560x1600", "color_depth": "30", "pixel_ratio": "2", "cores": "10", "touch_points": "0",
            "timezone": "Europe/Moscow", "os": "macOS" })
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
    state.clock.advance(Duration::hours(3));
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
        state.clock.advance(Duration::hours(1));
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
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (mut other, other_id) = testing::register(&state, "twin@example.com").await;
    let _joined = other.post(&format!("/api/invites/{}/join", room.invite_token), json!({})).await;
    signal(&mut room.student, "marker-1", device("gpu-1")).await;
    state.clock.advance(Duration::hours(3));
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

/// Основания связи пары (без пересчёта).
async fn evidence(state: &AppState, one: Uuid, two: Uuid) -> Json {
    let (user_a, user_b) = if one < two { (one, two) } else { (two, one) };
    sqlx::query_scalar("SELECT evidence FROM identity_links WHERE user_a = $1 AND user_b = $2")
        .bind(user_a)
        .bind(user_b)
        .fetch_one(&state.db)
        .await
        .unwrap()
}

/// Виды оснований связи.
fn kinds(found: &Json) -> Vec<&str> {
    found.as_array().unwrap().iter().filter_map(|piece| piece["kind"].as_str()).collect()
}

/// Домашний провайдер и VPN-хостинг из примера iptoasn.
const NETWORKS: &str = "198.51.100.0\t198.51.100.255\t64500\tRU\tExample Home ISP\n\
                        192.0.2.0\t192.0.2.255\t64501\tNL\tExample VPN Hosting\n";

/// Пользователь с отдельным устройством, браузером и адресом `ip`.
async fn on_network(state: &AppState, email: &str, ip: &str) -> (Client, Uuid) {
    let (mut client, user_id) = testing::register(state, email).await;
    client.set_ip(ip);
    signal(&mut client, &format!("marker-{email}"), device(&format!("gpu-{email}"))).await;
    (client, user_id)
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn vpn_keeps_marker_link_and_flags_network(db: PgPool) {
    let mut state = testing::state(db);
    state.asn = Arc::new(AsnTable::from_lines(NETWORKS));
    let (mut first, first_id) = testing::register(&state, "a@example.com").await;
    let (mut second, second_id) = testing::register(&state, "b@example.com").await;
    first.set_ip("198.51.100.10");
    second.set_ip("198.51.100.20");
    signal(&mut first, "browser-1", device("gpu-a")).await;
    signal(&mut second, "browser-1", device("gpu-b")).await;
    state.clock.advance(Duration::hours(2));
    first.set_ip("192.0.2.77");
    signal(&mut first, "browser-1", device("gpu-a")).await;
    assert_eq!(level(&state, first_id, second_id).await.as_deref(), Some("high"), "по маркеру");
    assert!(
        kinds(&evidence(&state, first_id, second_id).await).contains(&"shared_marker"),
        "основание — общий маркер"
    );
    let hosting: bool = sqlx::query_scalar(
        "SELECT hosting FROM signals WHERE user_id = $1 ORDER BY at DESC LIMIT 1",
    )
    .bind(first_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert!(hosting, "у сетевого сигнала признак VPN");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn shared_phone_links_accounts_and_is_named(db: PgPool) {
    let state = testing::state(db);
    let (mut owner, owner_id) = testing::register(&state, "a@example.com").await;
    let (mut guest, guest_id) = testing::register(&state, "b@example.com").await;
    signal(&mut owner, "a-laptop", device("laptop-gpu")).await;
    let laptop_at = state.clock.now();
    state.clock.advance(Duration::hours(2));
    signal(&mut owner, "a-phone", device("phone-gpu")).await;
    state.clock.advance(Duration::hours(2));
    signal(&mut guest, "b-phone", device("phone-gpu")).await;
    assert_eq!(level(&state, owner_id, guest_id).await.as_deref(), Some("medium"), "общий телефон");
    let found = evidence(&state, owner_id, guest_id).await;
    let devices: Vec<&Json> = found
        .as_array()
        .unwrap()
        .iter()
        .filter(|piece| piece["kind"] == "shared_rare_device")
        .collect();
    assert_eq!(devices.len(), 1, "в объяснении одно общее устройство: {found}");
    let laptop = laptop_at.format(&Rfc3339).unwrap();
    assert_ne!(devices[0]["at"].as_str(), Some(laptop.as_str()), "это телефон, а не ноутбук");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn bridge_device_links_accounts_transitively(db: PgPool) {
    let state = testing::state(db);
    let (mut bridge, bridge_id) = testing::register(&state, "a@example.com").await;
    let (mut laptop_user, laptop_id) = testing::register(&state, "b@example.com").await;
    let (mut phone_user, phone_id) = testing::register(&state, "c@example.com").await;
    signal(&mut bridge, "a-laptop", device("laptop-gpu")).await;
    signal(&mut bridge, "a-phone", device("phone-gpu")).await;
    state.clock.advance(Duration::hours(2));
    signal(&mut laptop_user, "b-laptop", device("laptop-gpu")).await;
    state.clock.advance(Duration::hours(2));
    signal(&mut phone_user, "c-phone", device("phone-gpu")).await;
    assert_eq!(level(&state, bridge_id, laptop_id).await.as_deref(), Some("medium"), "ноутбук");
    assert_eq!(
        level(&state, laptop_id, phone_id).await.as_deref(),
        Some("medium"),
        "транзитивно через аккаунт с ноутбуком и телефоном — уровень слабейшего ребра"
    );
    assert_eq!(
        kinds(&evidence(&state, laptop_id, phone_id).await),
        ["transitive_device"],
        "объяснение называет транзитивную связь"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn different_devices_same_home_network_on_several_days(db: PgPool) {
    let state = testing::state(db);
    let (mut first, first_id) = on_network(&state, "a@example.com", "198.51.100.7").await;
    state.clock.advance(Duration::hours(3));
    let (mut second, second_id) = on_network(&state, "b@example.com", "198.51.100.7").await;
    state.clock.advance(Duration::days(1));
    signal(&mut first, "marker-a@example.com", device("gpu-a@example.com")).await;
    state.clock.advance(Duration::hours(3));
    signal(&mut second, "marker-b@example.com", device("gpu-b@example.com")).await;
    assert_eq!(
        level(&state, first_id, second_id).await.as_deref(),
        Some("medium"),
        "домашняя сеть"
    );
    let found = evidence(&state, first_id, second_id).await;
    assert_eq!(kinds(&found), ["home_network"], "только сеть: {found}");
    let detail = found[0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("2026-10-13") && detail.contains("2026-10-14"),
        "в объяснении даты: {detail}"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn dorm_nat_shared_by_dozens_is_low(db: PgPool) {
    let state = testing::state(db);
    let mut ids = Vec::new();
    for index in 0..12 {
        let (_, user_id) =
            on_network(&state, &format!("dorm{index}@example.com"), "198.51.100.99").await;
        state.clock.advance(Duration::minutes(20));
        ids.push(user_id);
    }
    assert_eq!(level(&state, ids[0], ids[1]).await.as_deref(), Some("low"), "общий NAT — низкий");
    assert_eq!(
        kinds(&evidence(&state, ids[0], ids[1]).await),
        ["shared_public_network"],
        "только IP"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn ip_of_six_accounts_is_public_not_home(db: PgPool) {
    let state = testing::state(db);
    let mut ids = Vec::new();
    for index in 0..6 {
        let (_, user_id) =
            on_network(&state, &format!("u{index}@example.com"), "198.51.100.42").await;
        state.clock.advance(Duration::days(1));
        ids.push(user_id);
    }
    assert_eq!(
        level(&state, ids[0], ids[5]).await.as_deref(),
        Some("low"),
        "общий IP не связывает"
    );
    assert_eq!(
        kinds(&evidence(&state, ids[0], ids[5]).await),
        ["shared_public_network"],
        "сеть общего пользования, а не домашняя, хотя дни разные"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn rejected_link_removes_flag_from_results_table(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (mut twin, twin_id) = testing::register(&state, "twin@example.com").await;
    let _joined = twin.post(&format!("/api/invites/{}/join", room.invite_token), json!({})).await;
    signal(&mut room.student, "same", device("gpu")).await;
    signal(&mut twin, "same", device("gpu")).await;
    let _level = level(&state, room.student_id, twin_id).await;
    let attempt = room.start().await;
    let id = attempt["id"].as_str().unwrap();
    let _done = room.student.post(&format!("/api/attempts/{id}/submit"), json!({})).await;
    let results = format!("/api/tests/{}/results", room.test_id);
    assert_eq!(room.methodist.get(&results).await.1["rows"][0]["antifraud"], true, "значок есть");
    let decision = json!({ "course_id": room.course_id, "user_a": room.student_id, "user_b": twin_id, "decision": "rejected", "comment": "соседи по комнате" });
    let _decided = room.methodist.post("/api/antifraud/decisions", decision).await;
    assert_eq!(
        room.methodist.get(&results).await.1["rows"][0]["antifraud"],
        false,
        "значок пропал"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn other_browser_on_same_machine_is_linked_by_hardware_core(db: PgPool) {
    let state = testing::state(db);
    let (mut first, first_id) = testing::register(&state, "chrome@example.com").await;
    let (mut second, second_id) = testing::register(&state, "firefox@example.com").await;
    signal(&mut first, "marker-chrome", same_machine("chromium")).await;
    state.clock.advance(Duration::hours(3));
    signal(&mut second, "marker-firefox", same_machine("firefox")).await;
    assert_eq!(
        level(&state, first_id, second_id).await.as_deref(),
        Some("medium"),
        "то же устройство"
    );
    let found = evidence(&state, first_id, second_id).await;
    assert_eq!(kinds(&found), ["shared_rare_device"], "основание — общее устройство");
    assert!(found.to_string().contains("в разных браузерах"), "объяснение: {found}");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn exact_and_hardware_core_match_count_as_one_device(db: PgPool) {
    let state = testing::state(db);
    let (mut first, first_id) = testing::register(&state, "a@example.com").await;
    let (mut second, second_id) = testing::register(&state, "b@example.com").await;
    signal(&mut first, "marker-a", same_machine("chromium")).await;
    state.clock.advance(Duration::hours(3));
    signal(&mut second, "marker-b", same_machine("chromium")).await;
    signal(&mut second, "marker-b", same_machine("firefox")).await;
    assert_eq!(level(&state, first_id, second_id).await.as_deref(), Some("medium"), "не высокий");
    assert_eq!(
        kinds(&evidence(&state, first_id, second_id).await),
        ["shared_rare_device"],
        "одно основание"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn hardware_core_of_a_computer_class_is_low(db: PgPool) {
    let state = testing::state(db);
    let mut ids = Vec::new();
    for index in 0..5 {
        let (mut client, user_id) =
            testing::register(&state, &format!("lab{index}@example.com")).await;
        state.clock.advance(Duration::hours(1));
        signal(&mut client, &format!("marker-{index}"), same_machine(&format!("browser-{index}")))
            .await;
        ids.push(user_id);
    }
    assert_eq!(
        level(&state, ids[0], ids[1]).await.as_deref(),
        Some("low"),
        "одинаковые компьютеры класса"
    );
}

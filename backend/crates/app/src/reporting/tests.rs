use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use time::Duration;
use time::format_description::well_known::Rfc3339;

use crate::testing::{self, Classroom};

/// Номер вопроса и выбранного варианта.
type Pick = (usize, usize);

async fn submitted(room: &mut Classroom, picks: &[Pick]) -> String {
    let attempt = room.start().await;
    for (seq, (index, option)) in (1_i64..).zip(picks) {
        let (path, body) = testing::answer_body(&attempt, *index, *option, seq);
        let _saved = room.student.put(&path, body).await;
    }
    let id = attempt["id"].as_str().unwrap().to_owned();
    let _done = room.student.post(&format!("/api/attempts/{id}/submit"), json!({})).await;
    id
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn results_table_filters_by_stream_and_shows_cells(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 2, json!({})).await;
    let _attempt = submitted(&mut room, &[(0, 0), (1, 1)]).await;
    let (_, table) = room
        .methodist
        .get(&format!("/api/tests/{}/results?stream_id={}", room.test_id, room.stream_id))
        .await;
    assert_eq!(table["rows"].as_array().map(Vec::len), Some(1), "одна строка: {table}");
    assert_eq!(table["rows"][0]["score"], "1", "балл");
    assert_eq!(table["rows"][0]["cells"][1]["score"], "0", "неверный ответ");
    let other = uuid::Uuid::new_v4();
    let (_, empty) =
        room.methodist.get(&format!("/api/tests/{}/results?stream_id={other}", room.test_id)).await;
    assert_eq!(empty["rows"].as_array().map(Vec::len), Some(0), "фильтр по чужому потоку");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn student_cannot_read_results(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    assert_eq!(
        room.student.get(&format!("/api/tests/{}/results", room.test_id)).await.0,
        StatusCode::NOT_FOUND,
        "как будто нет"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn csv_export_has_bom_and_is_audited(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let _attempt = submitted(&mut room, &[(0, 0)]).await;
    let (status, bytes) =
        room.methodist.get_bytes(&format!("/api/tests/{}/results.csv", room.test_id)).await;
    assert_eq!(status, StatusCode::OK, "выгрузка");
    assert_eq!(&bytes[..3], b"\xEF\xBB\xBF", "BOM в начале");
    assert!(String::from_utf8_lossy(&bytes).contains("Студент;Email"), "заголовок с ;");
    let audited: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'export.csv'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(audited, 1, "выгрузка в аудите");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn course_summary_marks_missing_attempts(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (_, summary) =
        room.methodist.get(&format!("/api/courses/{}/summary", room.course_id)).await;
    assert_eq!(summary["rows"][0]["cells"][0]["attempts"], 0, "нет попыток: {summary}");
    assert!(summary["rows"][0]["cells"][0]["final_score"].is_null(), "пустая ячейка");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn question_stats_show_option_distribution(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let _attempt = submitted(&mut room, &[(0, 2)]).await;
    let (_, stats) = room.methodist.get(&format!("/api/tests/{}/stats", room.test_id)).await;
    assert_eq!(stats[0]["sample"], 1, "размер выборки");
    assert_eq!(stats[0]["options"][2]["chosen"], 1, "неверный вариант выбирали");
    assert_eq!(stats[0]["options"][0]["chosen"], 0, "верный — нет");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn attempt_details_show_event_log(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 2, json!({})).await;
    let attempt_id = submitted(&mut room, &[(1, 0)]).await;
    let (_, details) = room.methodist.get(&format!("/api/attempts/{attempt_id}/details")).await;
    let kinds: Vec<&str> = details["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["kind"].as_str())
        .collect();
    assert_eq!(kinds, ["start", "answer_saved", "submit"], "хронология: {details}");
}

/// Опубликованный тест «выбор + свободный ответ» с открытым окном; возвращает его id.
async fn free_text_test(room: &mut Classroom) -> String {
    let created = room
        .methodist
        .post(&format!("/api/courses/{}/tests", room.course_id), json!({ "title": "Эссе" }))
        .await;
    let test_id = testing::id_of(created);
    let questions = format!("/api/tests/{test_id}/questions");
    let free = json!({ "kind": "free_text", "body": "Опишите манёвр", "options": [] });
    for question in [testing::single_question(0), free] {
        let (added, body) = room.methodist.post(&questions, question).await;
        assert_eq!(added, StatusCode::CREATED, "вопрос: {body}");
    }
    let settings = json!({ "settings": testing::settings(&json!({})) });
    let _configured = room.methodist.patch(&format!("/api/tests/{test_id}"), settings).await;
    let (published, body) =
        room.methodist.post(&format!("/api/tests/{test_id}/publish"), json!({})).await;
    assert_eq!(published, StatusCode::NO_CONTENT, "публикация: {body}");
    room.window_of(&test_id, json!({ "action": "open" })).await;
    test_id
}

/// Студент выбирает верный вариант и пишет свободный ответ `text`; возвращает id попытки.
async fn answered_with_text(room: &mut Classroom, test_id: &str, text: &str) -> String {
    let attempt = room.start_of(test_id).await;
    let (path, body) = testing::answer_body(&attempt, 0, 0, 1);
    let _saved = room.student.put(&path, body).await;
    let free_id = attempt["questions"][1]["id"].as_str().unwrap();
    let id = attempt["id"].as_str().unwrap().to_owned();
    let free = format!("/api/attempts/{id}/answers/{free_id}");
    let (saved, error) = room.student.put(&free, json!({ "text": text, "client_seq": 2 })).await;
    assert_eq!(saved, StatusCode::OK, "свободный ответ: {error}");
    let _done = room.student.post(&format!("/api/attempts/{id}/submit"), json!({})).await;
    id
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn free_text_answer_is_shown_whole_and_not_scored(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let test_id = free_text_test(&mut room).await;
    let text = "Сначала пропустить пешехода, затем повернуть. ".repeat(20);
    let _attempt = answered_with_text(&mut room, &test_id, &text).await;
    let (_, table) = room.methodist.get(&format!("/api/tests/{test_id}/results")).await;
    assert_eq!(table["columns"][1]["free_text"], true, "столбец свободного ответа: {table}");
    let cell = &table["rows"][0]["cells"][1];
    assert_eq!(cell["text_answer"].as_str(), Some(text.as_str()), "текст целиком");
    assert!(cell["score"].is_null() && cell["free_text"] == true, "пометка «не оценивается»");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn attempt_details_show_answers_by_question(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let test_id = free_text_test(&mut room).await;
    let attempt_id = answered_with_text(&mut room, &test_id, "свой ответ").await;
    let (_, details) = room.methodist.get(&format!("/api/attempts/{attempt_id}/details")).await;
    let cells = &details["cells"];
    assert_eq!(cells[0]["selected"], json!(["вариант 0"]), "выбранный вариант: {details}");
    assert_eq!(cells[0]["score"], "1", "балл за вопрос");
    assert_eq!(cells[1]["text_answer"], "свой ответ", "свободный ответ");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn event_log_shows_when_question_seven_was_saved(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 8, json!({})).await;
    let attempt = room.start().await;
    let id = attempt["id"].as_str().unwrap().to_owned();
    state.clock.advance(Duration::minutes(7));
    let (path, body) = testing::answer_body(&attempt, 6, 0, 1);
    assert_eq!(room.student.put(&path, body).await.0, StatusCode::OK, "ответ на вопрос 7");
    let saved_at = state.clock.now().format(&Rfc3339).unwrap();
    state.clock.advance(Duration::minutes(1));
    let batch = json!({ "events": ["tab_hidden", "tab_visible", "offline", "online"] });
    let (accepted, _) = room.student.post(&format!("/api/attempts/{id}/events"), batch).await;
    assert_eq!(accepted, StatusCode::NO_CONTENT, "события клиента приняты");
    let _done = room.student.post(&format!("/api/attempts/{id}/submit"), json!({})).await;
    let (_, details) = room.methodist.get(&format!("/api/attempts/{id}/details")).await;
    let events = details["events"].as_array().unwrap();
    let saved = events.iter().find(|event| event["kind"] == "answer_saved").unwrap();
    assert_eq!(saved["question_id"], attempt["questions"][6]["id"], "какой вопрос: {details}");
    assert_eq!(saved["at"].as_str(), Some(saved_at.as_str()), "когда сохранён (серверное время)");
    assert!(saved["details"]["selected"].is_array(), "новый ответ в событии");
    let client: Vec<&str> = events.iter().filter_map(|event| event["kind"].as_str()).collect();
    assert_eq!(
        client,
        ["start", "answer_saved", "tab_hidden", "tab_visible", "offline", "online", "submit"],
        "вкладка и связь в журнале"
    );
}

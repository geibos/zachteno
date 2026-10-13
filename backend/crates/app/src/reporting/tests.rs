use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

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

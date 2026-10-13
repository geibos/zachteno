use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use time::Duration;

use crate::sweeper;
use crate::testing::{self, Classroom};

/// Причина сдачи и балл попытки.
type Outcome = (Option<String>, Option<rust_decimal::Decimal>);

fn attempt_path(attempt: &serde_json::Value, tail: &str) -> String {
    format!("/api/attempts/{}{tail}", attempt["id"].as_str().unwrap())
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn draft_test_cannot_be_started(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let draft = testing::id_of(
        room.methodist
            .post(&format!("/api/courses/{}/tests", room.course_id), json!({ "title": "Черновик" }))
            .await,
    );
    assert_eq!(
        room.student.post(&format!("/api/tests/{draft}/attempts"), json!({})).await.0,
        StatusCode::NOT_FOUND,
        "черновик не виден"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn attempts_limit_is_enforced(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({ "attempts_limit": 1 })).await;
    let attempt = room.start().await;
    let _done = room.student.post(&attempt_path(&attempt, "/submit"), json!({})).await;
    let (status, error) =
        room.student.post(&format!("/api/tests/{}/attempts", room.test_id), json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT, "попытки исчерпаны: {error}");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn concurrent_starts_create_one_attempt(db: PgPool) {
    let state = testing::state(db);
    let room = Classroom::new(&state, 1, json!({ "attempts_limit": 3 })).await;
    let path = format!("/api/tests/{}/attempts", room.test_id);
    let mut first = room.student.anonymous_with_same_cookie();
    let mut second = room.student.anonymous_with_same_cookie();
    let (one, two) = tokio::join!(first.post(&path, json!({})), second.post(&path, json!({})));
    assert_eq!(one.1["attempt_id"], two.1["attempt_id"], "одна и та же попытка");
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM attempts").fetch_one(&state.db).await.unwrap();
    assert_eq!(count, 1, "в базе одна попытка");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn reopening_resumes_same_attempt_with_same_order(db: PgPool) {
    let state = testing::state(db);
    let mut room =
        Classroom::new(&state, 6, json!({ "shuffle_questions": true, "shuffle_options": true }))
            .await;
    let first = room.start().await;
    let second = room.start().await;
    assert_eq!(first["id"], second["id"], "та же попытка на другом устройстве");
    assert_eq!(first["questions"], second["questions"], "порядок не меняется");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn random_sample_takes_requested_number_of_questions(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 30, json!({ "questions_per_attempt": 20 })).await;
    let attempt = room.start().await;
    assert_eq!(attempt["questions"].as_array().map(Vec::len), Some(20), "20 из 30");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn attempt_payload_has_no_correct_answers(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 3, json!({})).await;
    let attempt = room.start().await;
    let text = attempt.to_string();
    assert!(
        !text.contains("is_correct") && !text.contains("penalty") && !text.contains("weight"),
        "нет признаков правильности: {text}"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn answers_after_deadline_are_rejected_and_attempt_autosubmits(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 2, json!({ "time_limit_minutes": 20 })).await;
    let attempt = room.start().await;
    let (path, body) = testing::answer_body(&attempt, 0, 0, 1);
    assert_eq!(room.student.put(&path, body).await.0, StatusCode::OK, "до срока");
    state.clock.advance(Duration::minutes(21));
    let (late_path, late) = testing::answer_body(&attempt, 1, 0, 2);
    assert_eq!(
        room.student.put(&late_path, late).await.0,
        StatusCode::CONFLICT,
        "после срока — отказ"
    );
    assert!(sweeper::run_once(&state).await.unwrap(), "проход свипера");
    let (reason, score): Outcome = sqlx::query_as("SELECT submit_reason, score FROM attempts")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(reason.as_deref(), Some("timeout"), "сдана по времени");
    assert_eq!(
        score.map(|points| points.normalize().to_string()),
        Some("1".to_owned()),
        "засчитан ответ до срока"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn sequential_mode_forbids_going_back_even_via_api(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 3, json!({ "free_navigation": false })).await;
    let attempt = room.start().await;
    let navigate = attempt_path(&attempt, "/navigate");
    assert_eq!(
        room.student.post(&navigate, json!({ "to_index": 1 })).await.0,
        StatusCode::CONFLICT,
        "без ответа дальше нельзя"
    );
    let (path, body) = testing::answer_body(&attempt, 0, 1, 1);
    let _saved = room.student.put(&path, body).await;
    assert_eq!(
        room.student.post(&navigate, json!({ "to_index": 1 })).await.0,
        StatusCode::NO_CONTENT,
        "с ответом — дальше"
    );
    let (back_path, change) = testing::answer_body(&attempt, 0, 0, 2);
    assert_eq!(
        room.student.put(&back_path, change).await.0,
        StatusCode::CONFLICT,
        "вернуться и поменять нельзя"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn stale_client_seq_is_ignored(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let attempt = room.start().await;
    let (path, newer) = testing::answer_body(&attempt, 0, 2, 5);
    let _saved = room.student.put(&path, newer).await;
    let (_, older) = testing::answer_body(&attempt, 0, 0, 3);
    let _ignored = room.student.put(&path, older).await;
    let (_, view) = room.student.get(&attempt_path(&attempt, "")).await;
    assert_eq!(view["answers"][0]["client_seq"], 5, "старый запрос не перетёр новый");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn first_answer_locks_test_but_start_and_preview_do_not(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 2, json!({})).await;
    let preview =
        room.methodist.post(&format!("/api/tests/{}/preview", room.test_id), json!({})).await;
    let preview_id = preview.1["attempt_id"].as_str().unwrap().to_owned();
    let (_, preview_view) = room.methodist.get(&format!("/api/attempts/{preview_id}")).await;
    let (path, body) = testing::answer_body(&preview_view, 0, 0, 1);
    let _saved = room.methodist.put(&path, body).await;
    let attempt = room.start().await;
    let locked = || {
        sqlx::query_scalar::<_, bool>("SELECT locked_at IS NOT NULL FROM tests WHERE id = $1::uuid")
            .bind(room.test_id.clone())
    };
    assert!(!locked().fetch_one(&state.db).await.unwrap(), "предпросмотр и старт не блокируют");
    let (student_path, student_body) = testing::answer_body(&attempt, 0, 0, 1);
    let _student_saved = room.student.put(&student_path, student_body).await;
    assert!(locked().fetch_one(&state.db).await.unwrap(), "первый ответ блокирует");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn submit_scores_and_review_shows_after_submit(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 3, json!({})).await;
    let attempt = room.start().await;
    for (index, option) in [(0, 0), (1, 2)] {
        let (path, body) = testing::answer_body(&attempt, index, option, 1);
        let _saved = room.student.put(&path, body).await;
    }
    let (_, submitted) = room.student.post(&attempt_path(&attempt, "/submit"), json!({})).await;
    assert_eq!(submitted["submit_reason"], "manual", "сдана вручную");
    let (_, review) = room.student.get(&attempt_path(&attempt, "/review")).await;
    assert_eq!(review["score"], "1", "один верный из трёх: {review}");
    assert_eq!(review["max_score"], "3", "максимум 3");
    assert_eq!(review["questions"][1]["options"][0]["is_correct"], true, "правильный ответ виден");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn never_policy_hides_everything_but_fact(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({ "review_policy": "never" })).await;
    let attempt = room.start().await;
    let _done = room.student.post(&attempt_path(&attempt, "/submit"), json!({})).await;
    let (_, review) = room.student.get(&attempt_path(&attempt, "/review")).await;
    assert!(
        review["score"].is_null() && review["questions"].as_array().is_some_and(Vec::is_empty),
        "только факт сдачи: {review}"
    );
    assert!(review["submitted_at"].is_string(), "время сдачи есть");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn after_close_waits_for_whole_stream(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(
        &state,
        1,
        json!({ "review_policy": "after_close", "time_limit_minutes": 60 }),
    )
    .await;
    let mine = room.start().await;
    let (mut other, _) = testing::register(&state, "other@example.com").await;
    let _joined = other.post(&format!("/api/invites/{}/join", room.invite_token), json!({})).await;
    let _theirs = other.post(&format!("/api/tests/{}/attempts", room.test_id), json!({})).await;
    let _done = room.student.post(&attempt_path(&mine, "/submit"), json!({})).await;
    room.window(json!({ "action": "close" })).await;
    let (_, review) = room.student.get(&attempt_path(&mine, "/review")).await;
    assert_eq!(review["availability"]["available"], false, "кто-то ещё дорабатывает");
    assert_eq!(review["availability"]["waiting_for_stream"], true, "ждём поток");
    state.clock.advance(Duration::minutes(61));
    assert!(sweeper::run_once(&state).await.unwrap(), "проход свипера");
    let (_, after) = room.student.get(&attempt_path(&mine, "/review")).await;
    assert_eq!(after["availability"]["available"], true, "все попытки завершены — разбор открыт");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn dashboard_reports_results_later(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({ "review_policy": "after_close" })).await;
    let attempt = room.start().await;
    let _done = room.student.post(&attempt_path(&attempt, "/submit"), json!({})).await;
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    let completed = &dashboard["completed"][0];
    assert_eq!(completed["review"]["available"], false, "результаты позже: {dashboard}");
    assert!(completed["final_score"].is_null(), "балл скрыт до показа");
}

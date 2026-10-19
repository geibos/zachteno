use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use time::Duration;
use time::format_description::well_known::Rfc3339;

use crate::sweeper;
use crate::testing::{self, Classroom, Client};

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
    assert_eq!(attempt["question_count"], 20, "в свободном режиме видны все вопросы попытки");
    assert_eq!(attempt["forbid_copy"], false, "копирование по умолчанию не запрещено");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn sequential_mode_sends_only_current_question(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 3, json!({ "free_navigation": false })).await;
    let attempt = room.start().await;
    assert_eq!(attempt["questions"].as_array().map(Vec::len), Some(1), "один вопрос: {attempt}");
    assert_eq!(attempt["questions"][0]["index"], 0, "первый");
    assert_eq!(attempt["question_count"], 3, "число вопросов известно");
    let (path, body) = testing::answer_body(&attempt, 0, 1, 1);
    let _saved = room.student.put(&path, body).await;
    let (status, moved) =
        room.student.post(&attempt_path(&attempt, "/navigate"), json!({ "to_index": 1 })).await;
    assert_eq!(status, StatusCode::OK, "переход отдаёт вид попытки: {moved}");
    assert_eq!(moved["questions"].as_array().map(Vec::len), Some(1), "снова один вопрос");
    assert_eq!(moved["questions"][0]["index"], 1, "следующий");
    assert_ne!(moved["questions"][0]["id"], attempt["questions"][0]["id"], "другой вопрос");
    assert_eq!(moved["answers"], json!([]), "ответов на пройденный вопрос нет");
    let (_, reopened) = room.student.get(&attempt_path(&attempt, "")).await;
    assert_eq!(reopened["questions"], moved["questions"], "после перезагрузки тот же вопрос");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn forbid_copy_reaches_attempt_and_review(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({ "copy_policy": "forbidden" })).await;
    let attempt = room.start().await;
    assert_eq!(attempt["forbid_copy"], true, "в попытке: {attempt}");
    let _done = room.student.post(&attempt_path(&attempt, "/submit"), json!({})).await;
    let (_, review) = room.student.get(&attempt_path(&attempt, "/review")).await;
    assert_eq!(review["forbid_copy"], true, "в разборе: {review}");
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
        StatusCode::OK,
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
    assert_eq!(review["percent"], "33.3", "1 из 3");
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
        review["score"].is_null()
            && review["percent"].is_null()
            && review["questions"].as_array().is_some_and(Vec::is_empty),
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
    assert!(completed["final_percent"].is_null(), "процент скрыт до показа");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn second_active_attempt_violates_unique_index(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({ "attempts_limit": 3 })).await;
    let _first = room.start().await;
    let second =
        sqlx::query("INSERT INTO attempts (test_id, user_id, layout) VALUES ($1::uuid, $2, '[]')")
            .bind(&room.test_id)
            .bind(room.student_id)
            .execute(&state.db)
            .await;
    let violated = second.as_ref().err().and_then(|error| error.as_database_error());
    assert_eq!(
        violated.and_then(|error| error.constraint()),
        Some("attempts_one_active_idx"),
        "вторая активная попытка нарушает индекс: {second:?}"
    );
    let preview = sqlx::query(
        "INSERT INTO attempts (test_id, user_id, layout, is_preview) VALUES ($1::uuid, $2, '[]', true)",
    )
    .bind(&room.test_id)
    .bind(room.student_id)
    .execute(&state.db)
    .await;
    assert!(preview.is_ok(), "предпросмотр индекс не трогает: {preview:?}");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn dashboard_waits_for_all_stream_attempts_after_close(db: PgPool) {
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
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    let review = &dashboard["completed"][0]["review"];
    assert_eq!(review["mode"], "after_close", "статус «после закрытия»: {dashboard}");
    assert_eq!(review["available"], false, "ещё рано");
    assert_eq!(review["waiting_for_stream"], true, "ждём завершения всех попыток потока");
    state.clock.advance(Duration::minutes(61));
    assert!(sweeper::run_once(&state).await.unwrap(), "проход свипера");
    let (_, after) = room.student.get("/api/dashboard").await;
    assert_eq!(after["completed"][0]["review"]["available"], true, "все сдали — результаты есть");
    assert_eq!(after["completed"][0]["final_score"], "0", "балл виден");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn dashboard_shows_review_date_and_scheduled_tests(db: PgPool) {
    let state = testing::state(db);
    let shows = state.clock.now().saturating_add(Duration::days(2)).format(&Rfc3339).unwrap();
    let policy = json!({ "review_policy": "after_close", "review_not_before": shows });
    let mut room = Classroom::new(&state, 1, policy).await;
    let attempt = room.start().await;
    let _done = room.student.post(&attempt_path(&attempt, "/submit"), json!({})).await;
    let planned = room.another_test(1, json!({})).await;
    room.window_of(&planned, json!({ "action": "close" })).await;
    let opens = state.clock.now().saturating_add(Duration::hours(3)).format(&Rfc3339).unwrap();
    room.window_of(&planned, json!({ "action": "schedule", "open_at": opens })).await;
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    let review = &dashboard["completed"][0]["review"];
    assert_eq!(review["available"], false, "результаты позже: {dashboard}");
    assert_eq!(review["mode"], "after_close", "после закрытия");
    assert_eq!(review["not_before"].as_str(), Some(shows.as_str()), "указана дата показа");
    assert_eq!(dashboard["scheduled"][0]["test_id"], planned.as_str(), "запланированный тест");
    assert_eq!(
        dashboard["scheduled"][0]["opens_at"].as_str(),
        Some(opens.as_str()),
        "время открытия"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn other_device_continues_same_attempt_with_answers_and_timer(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 2, json!({ "time_limit_minutes": 30 })).await;
    let laptop = room.start().await;
    let (path, body) = testing::answer_body(&laptop, 0, 1, 1);
    assert_eq!(room.student.put(&path, body).await.0, StatusCode::OK, "ответ с ноутбука");
    state.clock.advance(Duration::minutes(5));
    let mut phone = Client::new(&state);
    let login = json!({ "email": "student@example.com", "password": testing::PASSWORD });
    assert_eq!(phone.post("/api/auth/login", login).await.0, StatusCode::OK, "вход с телефона");
    let (_, started) =
        phone.post(&format!("/api/tests/{}/attempts", room.test_id), json!({})).await;
    assert_eq!(started["attempt_id"], laptop["id"], "та же попытка: {started}");
    assert_eq!(started["resumed"], true, "продолжение");
    let (_, on_phone) = phone.get(&attempt_path(&laptop, "")).await;
    assert_eq!(on_phone["deadline_at"], laptop["deadline_at"], "таймер тот же");
    assert_eq!(
        on_phone["answers"],
        room.student.get(&attempt_path(&laptop, "")).await.1["answers"],
        "ответы те же"
    );
    assert_eq!(on_phone["answers"].as_array().map(Vec::len), Some(1), "сохранённый ответ на месте");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn partial_credit_scores_and_review_shows_question_max(db: PgPool) {
    let state = testing::state(db);
    let each = json!({ "kind": "multiple_each", "body": "?", "options": [
        { "body": "а", "is_correct": true }, { "body": "б", "is_correct": true },
        { "body": "в", "is_correct": true }, { "body": "г", "is_correct": false }] });
    let fraction = json!({ "kind": "multiple_fraction", "body": "?", "weight": "3", "options": [
        { "body": "а", "is_correct": true }, { "body": "б", "is_correct": true },
        { "body": "в", "is_correct": true }, { "body": "опасно", "is_correct": false, "penalty": "-100" },
        { "body": "д", "is_correct": false }, { "body": "е", "is_correct": false }] });
    let mut room = Classroom::with_questions(
        &state,
        &[each, fraction, testing::single_question(0)],
        json!({}),
    )
    .await;
    let attempt = room.start().await;
    let flags: Vec<Option<bool>> =
        (0..3).map(|index| attempt["questions"][index]["partial_credit"].as_bool()).collect();
    assert_eq!(flags, [Some(true), Some(true), Some(false)], "подсказка о частичном начислении");
    assert!(
        attempt["questions"][0]["options"][0].get("is_correct").is_none(),
        "правильность не передаётся: {attempt}"
    );
    for (seq, (index, options)) in (1_i64..).zip([(0, vec![0, 1]), (1, vec![0, 1, 3])]) {
        let (path, body) = testing::answer_many(&attempt, index, &options, seq);
        let (saved, error) = room.student.put(&path, body).await;
        assert!(saved.is_success(), "ответ: {error}");
    }
    let _done = room.student.post(&attempt_path(&attempt, "/submit"), json!({})).await;
    let (_, review) = room.student.get(&attempt_path(&attempt, "/review")).await;
    assert_eq!(review["max_score"], "7", "3 + 3 + 1: {review}");
    assert_eq!(review["score"], "-96", "2 + (−98) + 0, без нижней границы");
    assert_eq!(review["percent"], "-1371.4", "процент тоже отрицательный");
    let (_, dashboard) = room.student.get("/api/dashboard").await;
    assert_eq!(
        dashboard["completed"][0]["final_percent"], "-1371.4",
        "итоговый процент: {dashboard}"
    );
    assert_eq!(review["questions"][0]["score"], "2", "два верных из трёх");
    assert_eq!(review["questions"][0]["max_score"], "3", "максимум вопроса — вес × число верных");
    assert_eq!(review["questions"][1]["score"], "-98", "(2/3 − 100/3) × 3");
    assert_eq!(review["questions"][1]["max_score"], "3", "максимум «доли верных» — вес");
}

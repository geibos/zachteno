use std::io::Cursor;
use std::time::{Duration as StdDuration, Instant};

use axum::http::StatusCode;
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use tokio::time::sleep;
use uuid::Uuid;

use crate::auth::session::{CurrentUser, Roles};
use crate::authoring::quizzes::lock_for_edit;
use crate::state::AppState;
use crate::testing::{self, Classroom, Client};

async fn methodist(state: &AppState, email: &str) -> (Client, Uuid) {
    let (client, user_id) = testing::register(state, email).await;
    testing::grant(state, user_id, "methodist").await;
    (client, user_id)
}

async fn course(client: &mut Client) -> String {
    let (status, created) = client.post("/api/courses", json!({ "title": "ПДД" })).await;
    assert_eq!(status, StatusCode::CREATED, "курс: {created}");
    created["id"].as_str().unwrap().to_owned()
}

async fn test_in(client: &mut Client, course_id: &str) -> String {
    let (status, created) = client
        .post(&format!("/api/courses/{course_id}/tests"), json!({ "title": "Экзамен" }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "тест: {created}");
    created["id"].as_str().unwrap().to_owned()
}

fn single(correct: usize) -> Json {
    let options: Vec<Json> = (0..3)
        .map(|index| json!({ "body": format!("вариант {index}"), "is_correct": index == correct }))
        .collect();
    json!({ "kind": "single", "body": "Вопрос", "options": options })
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn methodist_creates_course_and_owns_it(db: PgPool) {
    let state = testing::state(db);
    let (mut owner, owner_id) = methodist(&state, "m@example.com").await;
    let course_id = course(&mut owner).await;
    let (_, detail) = owner.get(&format!("/api/courses/{course_id}")).await;
    assert_eq!(detail["owner_id"], owner_id.to_string(), "создатель — владелец");
    assert_eq!(detail["can_edit"], true, "может править");
    let (mut student, _) = testing::register(&state, "s@example.com").await;
    assert_eq!(
        student.post("/api/courses", json!({ "title": "x" })).await.0,
        StatusCode::FORBIDDEN,
        "студент не создаёт курсы"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn other_methodist_views_but_cannot_edit(db: PgPool) {
    let state = testing::state(db);
    let (mut owner, _) = methodist(&state, "m@example.com").await;
    let course_id = course(&mut owner).await;
    let test_id = test_in(&mut owner, &course_id).await;
    let (mut other, _) = methodist(&state, "other@example.com").await;
    assert_eq!(
        other.get(&format!("/api/tests/{test_id}")).await.0,
        StatusCode::OK,
        "просмотр чужого курса"
    );
    let patch = json!({ "title": "чужое" });
    assert_eq!(
        other.patch(&format!("/api/tests/{test_id}"), patch).await.0,
        StatusCode::FORBIDDEN,
        "правка запрещена"
    );
    let teacher = json!({ "email": "other@example.com" });
    assert_eq!(
        other.post(&format!("/api/courses/{course_id}/teachers"), teacher).await.0,
        StatusCode::FORBIDDEN,
        "назначать нельзя"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn owner_assigns_teacher_who_sees_but_cannot_edit(db: PgPool) {
    let state = testing::state(db);
    let (mut owner, _) = methodist(&state, "m@example.com").await;
    let course_id = course(&mut owner).await;
    let test_id = test_in(&mut owner, &course_id).await;
    let (mut teacher, _) = testing::register(&state, "t@example.com").await;
    let assign = json!({ "email": "t@example.com" });
    assert_eq!(
        owner.post(&format!("/api/courses/{course_id}/teachers"), assign).await.0,
        StatusCode::NO_CONTENT,
        "назначен"
    );
    let (_, list) = teacher.get("/api/courses").await;
    assert_eq!(list.as_array().map(Vec::len), Some(1), "курс в списке преподавателя");
    assert_eq!(teacher.get(&format!("/api/tests/{test_id}")).await.0, StatusCode::OK, "видит тест");
    assert_eq!(
        teacher.post(&format!("/api/tests/{test_id}/questions"), single(0)).await.0,
        StatusCode::FORBIDDEN,
        "не правит"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn new_test_has_spec_defaults(db: PgPool) {
    let state = testing::state(db);
    let (mut owner, _) = methodist(&state, "m@example.com").await;
    let course_id = course(&mut owner).await;
    let test_id = test_in(&mut owner, &course_id).await;
    let (_, test) = owner.get(&format!("/api/tests/{test_id}")).await;
    let settings = &test["settings"];
    assert_eq!(settings["attempts_limit"], 1, "одна попытка");
    assert!(settings["time_limit_minutes"].is_null(), "без лимита времени");
    assert_eq!(settings["free_navigation"], false, "без свободной навигации");
    assert_eq!(settings["review_policy"], "after_submit", "результаты сразу");
    assert_eq!(settings["final_policy"], "best", "лучшая попытка");
    assert_eq!(test["status"], "draft", "черновик");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn defaults_for_weight_and_any_mode_penalty(db: PgPool) {
    let state = testing::state(db);
    let (mut owner, _) = methodist(&state, "m@example.com").await;
    let course_id = course(&mut owner).await;
    let test_id = test_in(&mut owner, &course_id).await;
    let any = json!({ "kind": "multiple_any", "body": "?", "weight": "2", "options": [
        { "body": "да", "is_correct": true }, { "body": "нет", "is_correct": false }] });
    assert_eq!(
        owner.post(&format!("/api/tests/{test_id}/questions"), any).await.0,
        StatusCode::CREATED,
        "вопрос"
    );
    assert_eq!(
        owner.post(&format!("/api/tests/{test_id}/questions"), single(1)).await.0,
        StatusCode::CREATED,
        "вопрос"
    );
    let (_, test) = owner.get(&format!("/api/tests/{test_id}")).await;
    assert_eq!(test["questions"][0]["options"][1]["penalty"], "-2", "штраф −W в «хотя бы один»");
    assert_eq!(test["questions"][1]["weight"], "1", "вес по умолчанию 1");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn publish_names_broken_question(db: PgPool) {
    let state = testing::state(db);
    let (mut owner, _) = methodist(&state, "m@example.com").await;
    let course_id = course(&mut owner).await;
    let test_id = test_in(&mut owner, &course_id).await;
    let no_correct = json!({ "kind": "multiple_all", "body": "?", "options": [{ "body": "а" }, { "body": "б" }] });
    let _added = owner.post(&format!("/api/tests/{test_id}/questions"), no_correct).await;
    let (status, error) = owner.post(&format!("/api/tests/{test_id}/publish"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "публикация отклонена");
    assert!(error["message"].as_str().unwrap().starts_with("Вопрос 1"), "указан вопрос: {error}");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn locked_test_allows_only_title_and_review_changes(db: PgPool) {
    let state = testing::state(db);
    let (mut owner, _) = methodist(&state, "m@example.com").await;
    let course_id = course(&mut owner).await;
    let test_id = test_in(&mut owner, &course_id).await;
    let _added = owner.post(&format!("/api/tests/{test_id}/questions"), single(0)).await;
    let _locked = sqlx::query("UPDATE tests SET locked_at = now() WHERE id = $1::uuid")
        .bind(&test_id)
        .execute(&state.db)
        .await
        .unwrap();
    let (_, test) = owner.get(&format!("/api/tests/{test_id}")).await;
    let question_id = test["questions"][0]["id"].as_str().unwrap();
    let (status, error) = owner.put(&format!("/api/questions/{question_id}"), single(1)).await;
    assert_eq!(status, StatusCode::CONFLICT, "вопрос не меняется");
    assert!(error["message"].as_str().unwrap().contains("копию"), "предлагает копию: {error}");
    let mut settings = test["settings"].clone();
    settings["review_policy"] = json!("never");
    let patch = json!({ "title": "Новое имя", "settings": settings });
    assert_eq!(
        owner.patch(&format!("/api/tests/{test_id}"), patch).await.0,
        StatusCode::NO_CONTENT,
        "имя и показ можно"
    );
    settings["attempts_limit"] = json!(5);
    let forbidden = json!({ "settings": settings });
    assert_eq!(
        owner.patch(&format!("/api/tests/{test_id}"), forbidden).await.0,
        StatusCode::CONFLICT,
        "лимит попыток нельзя"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn clone_of_locked_test_is_editable_draft(db: PgPool) {
    let state = testing::state(db);
    let (mut owner, _) = methodist(&state, "m@example.com").await;
    let course_id = course(&mut owner).await;
    let test_id = test_in(&mut owner, &course_id).await;
    let _added = owner.post(&format!("/api/tests/{test_id}/questions"), single(2)).await;
    let _locked =
        sqlx::query("UPDATE tests SET locked_at = now(), status = 'published' WHERE id = $1::uuid")
            .bind(&test_id)
            .execute(&state.db)
            .await
            .unwrap();
    let (status, created) =
        owner.post(&format!("/api/tests/{test_id}/clone"), json!({ "course_id": course_id })).await;
    assert_eq!(status, StatusCode::CREATED, "клон: {created}");
    let (_, copy) = owner.get(&format!("/api/tests/{}", created["id"].as_str().unwrap())).await;
    assert_eq!(copy["status"], "draft", "черновик");
    assert!(copy["locked_at"].is_null(), "не заблокирован");
    assert_eq!(copy["questions"][0]["options"][2]["is_correct"], true, "содержимое скопировано");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn image_upload_and_access_checks(db: PgPool) {
    let state = testing::state(db);
    let (owner, _) = methodist(&state, "m@example.com").await;
    let png = {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::new(3, 2))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    };
    let (status, uploaded) = owner.multipart("/api/attachments", ("file", "sign.png"), png).await;
    assert_eq!(status, StatusCode::CREATED, "загрузка: {uploaded}");
    let url = uploaded["url"].as_str().unwrap().to_owned();
    assert_eq!(owner.get_raw(&url).await, StatusCode::OK, "автор видит");
    let (stranger, _) = testing::register(&state, "x@example.com").await;
    assert_eq!(stranger.get_raw(&url).await, StatusCode::NOT_FOUND, "посторонний — как будто нет");
    let (rejected, _) =
        owner.multipart("/api/attachments", ("file", "doc.pdf"), b"%PDF-1.7".to_vec()).await;
    assert_eq!(rejected, StatusCode::BAD_REQUEST, "не картинка");
}

/// Методист с черновиком теста из одного вопроса (верен вариант 1).
async fn draft(state: &AppState) -> (Client, String) {
    let (mut owner, _) = methodist(state, "m@example.com").await;
    let course_id = course(&mut owner).await;
    let test_id = test_in(&mut owner, &course_id).await;
    let (added, body) = owner.post(&format!("/api/tests/{test_id}/questions"), single(1)).await;
    assert_eq!(added, StatusCode::CREATED, "вопрос: {body}");
    (owner, test_id)
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn draft_preview_shows_score_and_review_but_leaves_no_results(db: PgPool) {
    let state = testing::state(db);
    let (mut owner, test_id) = draft(&state).await;
    let (status, started) = owner.post(&format!("/api/tests/{test_id}/preview"), json!({})).await;
    assert_eq!(status, StatusCode::CREATED, "предпросмотр черновика: {started}");
    let attempt_id = started["attempt_id"].as_str().unwrap();
    let (_, attempt) = owner.get(&format!("/api/attempts/{attempt_id}")).await;
    let (path, body) = testing::answer_body(&attempt, 0, 1, 1);
    assert_eq!(owner.put(&path, body).await.0, StatusCode::OK, "ответ в предпросмотре");
    let _done = owner.post(&format!("/api/attempts/{attempt_id}/submit"), json!({})).await;
    let (_, review) = owner.get(&format!("/api/attempts/{attempt_id}/review")).await;
    assert_eq!(review["score"], "1", "балл виден: {review}");
    assert_eq!(review["questions"][0]["options"][1]["is_correct"], true, "разбор виден");
    let (_, test) = owner.get(&format!("/api/tests/{test_id}")).await;
    assert!(test["locked_at"].is_null(), "тест не заблокирован");
    assert_eq!(test["status"], "draft", "остался черновиком");
    let (_, results) = owner.get(&format!("/api/tests/{test_id}/results")).await;
    assert_eq!(results["rows"].as_array().map(Vec::len), Some(0), "в результатах пусто: {results}");
}

/// Методист как `CurrentUser` — для правки в своей транзакции.
fn as_methodist(user_id: Uuid) -> CurrentUser {
    CurrentUser {
        id: user_id,
        email: String::new(),
        display_name: String::new(),
        roles: Roles { superuser: false, methodist: true },
        email_verified: true,
        trusted_registration: false,
        has_password: true,
        session_id: Uuid::nil(),
    }
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn edit_waiting_for_first_answer_sees_lock(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    // Транзакция первого ответа уже поставила locked_at, но ещё не завершилась.
    let mut answer = state.db.begin().await.unwrap();
    let _locked = sqlx::query("UPDATE tests SET locked_at = now() WHERE id = $1::uuid")
        .bind(&room.test_id)
        .execute(&mut *answer)
        .await
        .unwrap();
    let questions = format!("/api/tests/{}/questions", room.test_id);
    let edit = room.methodist.post(&questions, single(0));
    let commit = async {
        sleep(StdDuration::from_millis(300)).await;
        answer.commit().await.unwrap();
    };
    let ((status, body), ()) = tokio::join!(edit, commit);
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "правка дождалась ответа и увидела блокировку: {body}"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn first_answer_waits_for_edit_in_progress(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let attempt = room.start().await;
    let methodist_id: Uuid = sqlx::query_scalar("SELECT owner_id FROM courses WHERE id = $1::uuid")
        .bind(&room.course_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    let test_id: Uuid = room.test_id.parse().unwrap();
    // Правка методиста держит строку теста (FOR UPDATE) и видит его незаблокированным.
    let mut edit = state.db.begin().await.unwrap();
    let test = lock_for_edit(&mut edit, &as_methodist(methodist_id), test_id).await.unwrap();
    assert!(test.locked_at.is_none(), "правка начата до первого ответа");
    let (path, body) = testing::answer_body(&attempt, 0, 0, 1);
    let started = Instant::now();
    let answer = room.student.put(&path, body);
    let finish_edit = async {
        sleep(StdDuration::from_millis(300)).await;
        let _renamed = sqlx::query("UPDATE tests SET title = 'Правка до ответа' WHERE id = $1")
            .bind(test_id)
            .execute(&mut *edit)
            .await
            .unwrap();
        edit.commit().await.unwrap();
    };
    let ((status, _), ()) = tokio::join!(answer, finish_edit);
    assert_eq!(status, StatusCode::OK, "ответ сохранён после правки");
    assert!(started.elapsed() >= StdDuration::from_millis(300), "ответ ждал конца правки");
    let (title, locked): (String, bool) =
        sqlx::query_as("SELECT title, locked_at IS NOT NULL FROM tests WHERE id = $1")
            .bind(test_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!((title.as_str(), locked), ("Правка до ответа", true), "правка, затем блокировка");
}

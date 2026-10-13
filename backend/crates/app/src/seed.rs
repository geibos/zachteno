//! Демонстрационные данные: курс «ПДД» из набора `etspring/pdd_russia` (task 13.1).
//!
//! Набор в репозиторий не входит (лицензии нет): команда читает его из каталога.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::Deserialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::authoring::model::TestSettings;
use crate::blobs;
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Название курса — по нему повторный запуск находит уже созданное.
pub(crate) const COURSE_TITLE: &str = "ПДД: экзаменационные билеты (A, B)";

#[derive(Debug, Deserialize)]
struct Question {
    question: String,
    image: String,
    answers: Vec<Answer>,
    #[serde(default)]
    answer_tip: String,
}

#[derive(Debug, Deserialize)]
struct Answer {
    answer_text: String,
    is_correct: bool,
}

/// Итог загрузки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seeded {
    pub course_id: Uuid,
    pub questions: usize,
    pub images: usize,
    pub created: bool,
}

/// Загружает билеты категорий A и B; владелец курса — `owner_id`.
///
/// cancel-safe: NO — картинки пишутся в хранилище до коммита транзакции; повторный запуск
/// после отмены создаст курс заново (осиротевшие картинки безвредны).
pub(crate) async fn seed_pdd(state: &AppState, dataset: &Path, owner_id: Uuid) -> anyhow::Result<Seeded> {
    if let Some(course_id) = existing(state, owner_id).await? {
        let questions: i64 = sqlx::query_scalar("SELECT count(*) FROM questions q JOIN tests t ON t.id = q.test_id WHERE t.course_id = $1 AND t.title = 'Вся база ПДД'")
            .bind(course_id)
            .fetch_one(&state.db)
            .await?;
        return Ok(Seeded { course_id, questions: usize::try_from(questions).unwrap_or(0), images: 0, created: false });
    }
    let questions = read_tickets(&dataset.join("questions/A_B/tickets"))?;
    let images = upload_images(state, dataset, &questions, owner_id).await?;
    let mut tx = state.db.begin().await?;
    let course_id: Uuid = sqlx::query_scalar("INSERT INTO courses (title, description, owner_id) VALUES ($1, $2, $3) RETURNING id")
        .bind(COURSE_TITLE)
        .bind("Демонстрационный курс: официальные экзаменационные билеты ГИБДД, категории A и B.")
        .bind(owner_id)
        .fetch_one(&mut *tx)
        .await?;
    let full = create_test(&mut tx, course_id, ("Вся база ПДД", 0), &full_settings()).await?;
    insert_questions(&mut tx, full, &questions, &images).await?;
    let exam = create_test(&mut tx, course_id, ("Экзамен ПДД", 1), &exam_settings()).await?;
    copy_questions(&mut tx, full, exam).await?;
    tx.commit().await?;
    Ok(Seeded { course_id, questions: questions.len(), images: images.len(), created: true })
}

async fn existing(state: &AppState, owner_id: Uuid) -> anyhow::Result<Option<Uuid>> {
    Ok(sqlx::query_scalar("SELECT id FROM courses WHERE title = $1 AND owner_id = $2 AND archived_at IS NULL")
        .bind(COURSE_TITLE)
        .bind(owner_id)
        .fetch_optional(&state.db)
        .await?)
}

/// Билеты в порядке номеров: «Билет 1» … «Билет 40».
fn read_tickets(directory: &Path) -> anyhow::Result<Vec<Question>> {
    let mut files: Vec<(u32, PathBuf)> = std::fs::read_dir(directory)
        .with_context(|| format!("каталог билетов {}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| ticket_number(&path).map(|number| (number, path)))
        .collect();
    files.sort_unstable();
    let mut questions = Vec::new();
    for (_, path) in files {
        let text = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        let ticket: Vec<Question> = serde_json::from_str(&text).with_context(|| path.display().to_string())?;
        questions.extend(ticket);
    }
    Ok(questions)
}

fn ticket_number(path: &Path) -> Option<u32> {
    path.file_stem()?.to_str()?.strip_prefix("Билет ")?.parse().ok()
}

fn has_image(question: &Question) -> bool {
    !question.image.ends_with("no_image.jpg") && !question.image.is_empty()
}

async fn upload_images(state: &AppState, dataset: &Path, questions: &[Question], owner_id: Uuid) -> anyhow::Result<HashMap<String, Uuid>> {
    let mut uploaded = HashMap::new();
    for question in questions.iter().filter(|question| has_image(question)) {
        if uploaded.contains_key(&question.image) {
            continue;
        }
        let path = dataset.join(question.image.trim_start_matches("./"));
        let bytes = tokio::fs::read(&path).await.with_context(|| path.display().to_string())?;
        let processed = tokio::task::spawn_blocking(move || blobs::process(&bytes)).await?.map_err(|rejected| anyhow::anyhow!("{}: {rejected}", path.display()))?;
        let key = format!("img/{}", Uuid::new_v4());
        let size = i64::try_from(processed.bytes.len()).unwrap_or(i64::MAX);
        let (width, height) = (i32::try_from(processed.width).unwrap_or(0), i32::try_from(processed.height).unwrap_or(0));
        blobs::put(&state.blobs, &key, processed.bytes).await?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO attachments (storage_key, content_type, byte_size, width, height, uploaded_by) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(&key)
        .bind(processed.content_type)
        .bind(size)
        .bind(width)
        .bind(height)
        .bind(owner_id)
        .fetch_one(&state.db)
        .await?;
        let _previous = uploaded.insert(question.image.clone(), id);
    }
    Ok(uploaded)
}

fn full_settings() -> TestSettings {
    TestSettings { attempts_limit: None, free_navigation: true, shuffle_options: true, ..TestSettings::default() }
}

fn exam_settings() -> TestSettings {
    TestSettings {
        attempts_limit: None,
        time_limit_minutes: Some(20),
        questions_per_attempt: Some(20),
        free_navigation: true,
        shuffle_questions: true,
        shuffle_options: true,
        ..TestSettings::default()
    }
}

async fn create_test(tx: &mut PgConnection, course_id: Uuid, (title, position): (&str, i32), settings: &TestSettings) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO tests (course_id, title, status, position, attempts_limit, time_limit_minutes, questions_per_attempt,
                            free_navigation, shuffle_questions, shuffle_options, review_policy, final_policy)
         VALUES ($1, $2, 'published', $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
    )
    .bind(course_id)
    .bind(title)
    .bind(position)
    .bind(settings.attempts_limit)
    .bind(settings.time_limit_minutes)
    .bind(settings.questions_per_attempt)
    .bind(settings.free_navigation)
    .bind(settings.shuffle_questions)
    .bind(settings.shuffle_options)
    .bind(settings.review_policy.as_str())
    .bind(settings.final_policy.as_str())
    .fetch_one(&mut *tx)
    .await?)
}

async fn insert_questions(tx: &mut PgConnection, test_id: Uuid, questions: &[Question], images: &HashMap<String, Uuid>) -> anyhow::Result<()> {
    for (position, question) in (0_i32..).zip(questions) {
        let question_id: Uuid = sqlx::query_scalar(
            "INSERT INTO questions (test_id, position, kind, body, image_id, image_alt, hint) VALUES ($1, $2, 'single', $3, $4, $5, $6) RETURNING id",
        )
        .bind(test_id)
        .bind(position)
        .bind(&question.question)
        .bind(images.get(&question.image).copied())
        .bind(if has_image(question) { "Дорожная ситуация к вопросу" } else { "" })
        .bind(&question.answer_tip)
        .fetch_one(&mut *tx)
        .await?;
        for (option_position, answer) in (0_i32..).zip(&question.answers) {
            let _inserted = sqlx::query("INSERT INTO options (question_id, position, body, is_correct) VALUES ($1, $2, $3, $4)")
                .bind(question_id)
                .bind(option_position)
                .bind(&answer.answer_text)
                .bind(answer.is_correct)
                .execute(&mut *tx)
                .await?;
        }
    }
    Ok(())
}

async fn copy_questions(tx: &mut PgConnection, from: Uuid, to: Uuid) -> anyhow::Result<()> {
    let _copied = sqlx::query(
        "WITH source AS (SELECT q.*, gen_random_uuid() AS new_id FROM questions q WHERE q.test_id = $1),
              inserted AS (INSERT INTO questions (id, test_id, position, kind, body, weight, image_id, image_alt, hint)
                           SELECT new_id, $2, position, kind, body, weight, image_id, image_alt, hint FROM source RETURNING id)
         INSERT INTO options (question_id, position, body, image_id, image_alt, is_correct, penalty, pinned)
         SELECT s.new_id, o.position, o.body, o.image_id, o.image_alt, o.is_correct, o.penalty, o.pinned
         FROM options o JOIN source s ON s.id = o.question_id",
    )
    .bind(from)
    .bind(to)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

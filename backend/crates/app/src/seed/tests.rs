use std::fs::{create_dir_all, write};
use std::io::Cursor;
use std::path::Path;

use serde_json::json;
use sqlx::PgPool;

use super::{COURSE_TITLE, seed_pdd};
use crate::testing;

/// Название теста, число вопросов и вопросов на попытку.
type TestShape = (String, i64, Option<i32>);

fn fragment() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let tickets = root.path().join("questions/A_B/tickets");
    create_dir_all(&tickets).unwrap();
    create_dir_all(root.path().join("images/A_B")).unwrap();
    write(root.path().join("images/A_B/sign.jpg"), sample_jpeg()).unwrap();
    for ticket in 1..=2 {
        write_ticket(&tickets, ticket);
    }
    root
}

fn sample_jpeg() -> Vec<u8> {
    let mut jpeg = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::new(8, 4))
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    jpeg.into_inner()
}

fn write_ticket(tickets: &Path, ticket: u32) {
    let questions: Vec<_> = (0..20).map(|index| json!({
        "question": format!("Билет {ticket}, вопрос {index}"),
        "image": if index == 0 { "./images/A_B/sign.jpg" } else { "./images/no_image.jpg" },
        "answers": [{ "answer_text": "да", "is_correct": true }, { "answer_text": "нет", "is_correct": false }],
        "answer_tip": "пояснение"
    })).collect();
    write(tickets.join(format!("Билет {ticket}.json")), serde_json::to_string(&questions).unwrap())
        .unwrap();
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn seed_creates_course_with_full_base_and_exam(db: PgPool) {
    let state = testing::state(db);
    let (_client, owner) = testing::register(&state, "metod@example.com").await;
    let dataset = fragment();
    let seeded = seed_pdd(&state, dataset.path(), owner).await.unwrap();
    assert_eq!(
        (seeded.questions, seeded.images, seeded.created),
        (40, 1, true),
        "40 вопросов, одна картинка"
    );
    let tests: Vec<TestShape> = sqlx::query_as(
        "SELECT t.title, (SELECT count(*) FROM questions q WHERE q.test_id = t.id), t.questions_per_attempt FROM tests t WHERE course_id = $1 ORDER BY position",
    )
    .bind(seeded.course_id).fetch_all(&state.db).await.unwrap();
    assert_eq!(
        tests,
        vec![("Вся база ПДД".to_owned(), 40, None), ("Экзамен ПДД".to_owned(), 40, Some(20))],
        "два теста"
    );
    let again = seed_pdd(&state, dataset.path(), owner).await.unwrap();
    assert!(!again.created, "повторный запуск не создаёт дублей");
    let courses: i64 = sqlx::query_scalar("SELECT count(*) FROM courses WHERE title = $1")
        .bind(COURSE_TITLE)
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(courses, 1, "один курс");
}

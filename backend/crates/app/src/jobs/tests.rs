use serde_json::json;
use sqlx::PgPool;
use time::Duration;

use super::{JobKind, MAX_ATTEMPTS, claim, enqueue, fail};
use crate::state::AppState;
use crate::sweeper;
use crate::testing;

/// Ошибка и время следующего запуска задачи.
type Retry = (Option<String>, time::OffsetDateTime);

async fn queued(state: &AppState) {
    enqueue(&state.db, JobKind::Email, &json!({ "template": "verify_email" }), state.clock.now())
        .await
        .unwrap();
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn failed_job_is_retried_after_pause_with_last_error(db: PgPool) {
    let state = testing::state(db);
    queued(&state).await;
    let job = claim(&state, Some(JobKind::Email)).await.unwrap().expect("задача взята");
    fail(&state, &job, "SMTP недоступен").await.unwrap();
    let (error, run_at): Retry =
        sqlx::query_as("SELECT last_error, run_at FROM jobs WHERE id = $1")
            .bind(job.id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(error.as_deref(), Some("SMTP недоступен"), "ошибка сохранена");
    assert_eq!(run_at, state.clock.now().saturating_add(Duration::seconds(2)), "пауза 2 с");
    assert!(claim(&state, None).await.unwrap().is_none(), "до паузы не повторяется");
    state.clock.advance(Duration::seconds(3));
    let retried = claim(&state, None).await.unwrap().expect("повтор после паузы");
    assert_eq!((retried.id, retried.attempts), (job.id, 2), "та же задача, вторая попытка");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn two_workers_do_not_take_the_same_job(db: PgPool) {
    let state = testing::state(db);
    queued(&state).await;
    let (first, second) = tokio::join!(claim(&state, None), claim(&state, None));
    let taken: Vec<i64> =
        [first.unwrap(), second.unwrap()].into_iter().flatten().map(|job| job.id).collect();
    assert_eq!(taken.len(), 1, "задачу взял ровно один воркер: {taken:?}");
    assert!(claim(&state, None).await.unwrap().is_none(), "пока идёт аренда, никто другой");
}

/// Полезная нагрузка задачи.
async fn payload_of(state: &AppState, job_id: i64) -> serde_json::Value {
    sqlx::query_scalar("SELECT payload FROM jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(&state.db)
        .await
        .unwrap()
}

/// Письмо со ссылкой и сырым токеном в очереди.
async fn letter_with_token(state: &AppState) {
    let letter =
        json!({ "template": "verify_email", "to": "a@b.ru", "name": "А", "token": "сырой-токен" });
    enqueue(&state.db, JobKind::Email, &letter, state.clock.now()).await.unwrap();
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn letter_that_exhausted_attempts_loses_its_token(db: PgPool) {
    let state = testing::state(db);
    letter_with_token(&state).await;
    let job = claim(&state, Some(JobKind::Email)).await.unwrap().expect("задача взята");
    fail(&state, &job, "SMTP недоступен").await.unwrap();
    assert_eq!(payload_of(&state, job.id).await["token"], "сырой-токен", "для повтора токен нужен");
    let _exhausted = sqlx::query("UPDATE jobs SET attempts = $2 - 1, run_at = $3 WHERE id = $1")
        .bind(job.id)
        .bind(MAX_ATTEMPTS)
        .bind(state.clock.now())
        .execute(&state.db)
        .await
        .unwrap();
    let last = claim(&state, Some(JobKind::Email)).await.unwrap().expect("последняя попытка");
    assert_eq!(last.attempts, MAX_ATTEMPTS, "попытки исчерпаны");
    fail(&state, &last, "SMTP недоступен").await.unwrap();
    assert_eq!(
        payload_of(&state, job.id).await,
        json!({}),
        "токен стёрт после окончательного провала"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn sweeper_scrubs_unsent_letters_older_than_token_lifetime(db: PgPool) {
    let state = testing::state(db);
    letter_with_token(&state).await;
    letter_with_token(&state).await;
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM jobs ORDER BY id").fetch_all(&state.db).await.unwrap();
    let _aged = sqlx::query("UPDATE jobs SET created_at = $2 WHERE id = $1")
        .bind(ids[0])
        .bind(state.clock.now().saturating_sub(Duration::hours(25)))
        .execute(&state.db)
        .await
        .unwrap();
    assert!(sweeper::run_once(&state).await.unwrap(), "свипер прошёл");
    assert_eq!(
        payload_of(&state, ids[0]).await,
        json!({}),
        "старое неотправленное письмо без токена"
    );
    assert_eq!(payload_of(&state, ids[1]).await["token"], "сырой-токен", "свежее не тронуто");
}

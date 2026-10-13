use serde_json::json;
use sqlx::PgPool;
use time::Duration;

use super::{JobKind, claim, enqueue, fail};
use crate::state::AppState;
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

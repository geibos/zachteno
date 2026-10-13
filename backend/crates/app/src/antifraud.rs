//! Сигналы и связи аккаунтов (spec `antifraud-signals`). Только сигнал: ничего не блокирует.

pub(crate) mod asn;
pub(crate) mod ingest;
pub(crate) mod links;
mod review;

#[cfg(test)]
mod tests;

use tokio::time::{sleep, timeout};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::jobs::{self, JobKind};
use crate::state::AppState;

/// Маршруты антифрода.
pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(ingest::submit))
        .routes(routes!(review::course_links))
        .routes(routes!(review::decide))
}

/// Воркер пересчёта связей.
///
/// cancel-safe: yes — каждая задача идемпотентна; при отмене повторится по истечении аренды.
pub(crate) async fn run_worker(state: AppState) -> ! {
    loop {
        step(&state).await;
    }
}

/// Одна итерация воркера: задача, ожидание новой или пауза после ошибки очереди.
///
/// cancel-safe: yes — см. `run_worker`.
async fn step(state: &AppState) {
    match jobs::claim(state, Some(JobKind::AntifraudRecompute)).await {
        Ok(Some(job)) => process(state, &job).await,
        Ok(None) => {
            let _woken = timeout(jobs::IDLE_POLL, state.jobs_wake.notified()).await;
        }
        Err(error) => queue_unavailable(&error).await,
    }
}

/// Пишет ошибку очереди и выжидает перед следующей попыткой.
///
/// cancel-safe: yes — только пауза.
async fn queue_unavailable(error: &sqlx::Error) {
    tracing::error!(%error, "очередь антифрода недоступна");
    sleep(jobs::IDLE_POLL).await;
}

#[derive(serde::Deserialize)]
struct RecomputePayload {
    user_id: uuid::Uuid,
}

/// cancel-safe: yes — пересчёт идемпотентен, отметка о задаче — один UPDATE.
async fn process(state: &AppState, job: &jobs::Claimed) {
    let recorded = match recompute(state, job).await {
        Ok(()) => jobs::complete(state, job.id).await,
        Err(error) => jobs::fail(state, job, &error).await,
    };
    if let Err(error) = recorded {
        report_unrecorded(job, &error);
    }
}

fn report_unrecorded(job: &jobs::Claimed, error: &sqlx::Error) {
    tracing::error!(job = job.id, %error, "не удалось обновить задачу антифрода");
}

/// cancel-safe: yes — пересчёт идемпотентен.
async fn recompute(state: &AppState, job: &jobs::Claimed) -> Result<(), String> {
    let payload = job.payload::<RecomputePayload>().map_err(|error| error.to_string())?;
    links::recompute(state, payload.user_id).await.map_err(|error| error.to_string())
}

/// Пересчитывает все ожидающие задачи сразу (для тестов и CLI).
#[cfg(test)]
pub(crate) async fn drain(state: &AppState) {
    while let Ok(Some(job)) = jobs::claim(state, Some(JobKind::AntifraudRecompute)).await {
        process(state, &job).await;
    }
    let failed: Option<String> =
        sqlx::query_scalar("SELECT last_error FROM jobs WHERE last_error IS NOT NULL LIMIT 1")
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    assert!(failed.is_none(), "задача антифрода упала: {failed:?}");
}

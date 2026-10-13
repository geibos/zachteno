//! Сигналы и связи аккаунтов (spec `antifraud-signals`). Только сигнал: ничего не блокирует.

pub(crate) mod asn;
pub(crate) mod ingest;
pub(crate) mod links;
mod review;

#[cfg(test)]
mod tests;

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
pub(crate) async fn run_worker(state: AppState) {
    loop {
        match jobs::claim(&state, Some(JobKind::AntifraudRecompute)).await {
            Ok(Some(job)) => process(&state, &job).await,
            Ok(None) => {
                let _woken = tokio::time::timeout(jobs::IDLE_POLL, state.jobs_wake.notified()).await;
            }
            Err(error) => {
                tracing::error!(%error, "очередь антифрода недоступна");
                tokio::time::sleep(jobs::IDLE_POLL).await;
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct RecomputePayload {
    user_id: uuid::Uuid,
}

async fn process(state: &AppState, job: &jobs::Claimed) {
    let outcome = match job.payload::<RecomputePayload>() {
        Ok(payload) => links::recompute(state, payload.user_id).await.map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    };
    let recorded = match outcome {
        Ok(()) => jobs::complete(state, job.id).await,
        Err(error) => jobs::fail(state, job, &error).await,
    };
    if let Err(error) = recorded {
        tracing::error!(job = job.id, %error, "не удалось обновить задачу антифрода");
    }
}

/// Пересчитывает все ожидающие задачи сразу (для тестов и CLI).
#[cfg(test)]
pub(crate) async fn drain(state: &AppState) {
    while let Ok(Some(job)) = jobs::claim(state, Some(JobKind::AntifraudRecompute)).await {
        process(state, &job).await;
    }
}

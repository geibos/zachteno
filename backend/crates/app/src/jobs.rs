//! Очередь фоновых задач в PostgreSQL (`SKIP LOCKED`, повторы — design D7).

use std::time::Duration as StdDuration;

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value as Json;
use sqlx::{FromRow, PgExecutor};
use time::{Duration, OffsetDateTime};

use crate::state::AppState;

/// Виды задач.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobKind {
    /// Отправка письма.
    Email,
    /// Пересчёт связей антифрода для пользователя.
    AntifraudRecompute,
}

impl JobKind {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::AntifraudRecompute => "antifraud_recompute",
        }
    }
}

/// Ставит задачу в очередь (в транзакции вызывающего).
///
/// cancel-safe: yes — один INSERT.
pub(crate) async fn enqueue<'conn>(
    executor: impl PgExecutor<'conn>,
    kind: JobKind,
    payload: &(impl Serialize + Sync),
    run_at: OffsetDateTime,
) -> Result<(), sqlx::Error> {
    let _inserted = sqlx::query("INSERT INTO jobs (kind, payload, run_at) VALUES ($1, $2, $3)")
        .bind(kind.name())
        .bind(serde_json::to_value(payload).map_err(|error| sqlx::Error::Encode(error.into()))?)
        .bind(run_at)
        .execute(executor)
        .await?;
    Ok(())
}

/// Взятая в работу задача.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct Claimed {
    pub id: i64,
    pub payload: Json,
    pub attempts: i32,
}

impl Claimed {
    /// Разбирает полезную нагрузку.
    pub(crate) fn payload<T: DeserializeOwned>(&self) -> anyhow::Result<T> {
        Ok(serde_json::from_value(self.payload.clone())?)
    }
}

/// Сколько держать задачу за собой, прежде чем её сможет взять другой воркер.
const LEASE_SECONDS: i64 = 300;
/// После стольких неудач задача больше не повторяется.
pub(crate) const MAX_ATTEMPTS: i32 = 8;

const CLAIM: &str = "UPDATE jobs SET locked_until = $1, attempts = attempts + 1
         WHERE id = (
             SELECT id FROM jobs
             WHERE done_at IS NULL AND run_at <= $2 AND attempts < $4
               AND (locked_until IS NULL OR locked_until < $2)
               AND ($3::text IS NULL OR kind = $3)
             ORDER BY run_at, id
             FOR UPDATE SKIP LOCKED
             LIMIT 1)
         RETURNING id, payload, attempts";

/// Берёт одну готовую задачу вида `kind` (или любого вида при `None`).
///
/// cancel-safe: yes — UPDATE атомарен; при отмене после него задача вернётся по истечении аренды.
pub(crate) async fn claim(
    state: &AppState,
    kind: Option<JobKind>,
) -> Result<Option<Claimed>, sqlx::Error> {
    let now = state.clock.now();
    sqlx::query_as(CLAIM)
        .bind(now.saturating_add(Duration::seconds(LEASE_SECONDS)))
        .bind(now)
        .bind(kind.map(JobKind::name))
        .bind(MAX_ATTEMPTS)
        .fetch_optional(&state.db)
        .await
}

/// Отмечает задачу выполненной и стирает полезную нагрузку (в ней могут быть токены ссылок).
///
/// cancel-safe: yes — один UPDATE.
pub(crate) async fn complete(state: &AppState, job_id: i64) -> Result<(), sqlx::Error> {
    let _done = sqlx::query(
        "UPDATE jobs SET done_at = $2, payload = '{}', locked_until = NULL WHERE id = $1",
    )
    .bind(job_id)
    .bind(state.clock.now())
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Отмечает неудачу: повтор с экспоненциальной паузой.
///
/// cancel-safe: yes — один UPDATE.
pub(crate) async fn fail(state: &AppState, job: &Claimed, error: &str) -> Result<(), sqlx::Error> {
    let exponent = u32::try_from(job.attempts.clamp(0, 12)).unwrap_or(12);
    let pause = Duration::seconds(i64::from(2_u32.saturating_pow(exponent)).min(3600));
    let _failed = sqlx::query(
        "UPDATE jobs SET last_error = $2, run_at = $3, locked_until = NULL WHERE id = $1",
    )
    .bind(job.id)
    .bind(error)
    .bind(state.clock.now().saturating_add(pause))
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Пауза воркера, когда задач нет (или пока не разбудят).
pub(crate) const IDLE_POLL: StdDuration = StdDuration::from_secs(10);

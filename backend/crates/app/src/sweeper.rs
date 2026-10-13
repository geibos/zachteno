//! Периодические задачи: автосдача по сроку, расписание окон, очистка (design D7).

use std::time::Duration as StdDuration;

use sqlx::PgConnection;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::attempts::lifecycle::{self, SubmitReason};
use crate::error::ApiResult;
use crate::state::AppState;

/// Интервал между проходами.
const INTERVAL: StdDuration = StdDuration::from_secs(15);
/// Ключ advisory lock: при нескольких экземплярах свипер работает один.
const LOCK_KEY: i64 = 7_420_011;

/// Бесконечный цикл свипера.
///
/// cancel-safe: yes — каждый проход в своих транзакциях; отмена между ними безопасна.
pub(crate) async fn run(state: AppState) {
    loop {
        if let Err(error) = run_once(&state).await {
            tracing::error!(%error, "проход свипера не удался");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

/// Один проход; `false`, если блокировку держит другой экземпляр.
///
/// cancel-safe: yes — см. `run`.
pub(crate) async fn run_once(state: &AppState) -> ApiResult<bool> {
    let mut connection = state.db.acquire().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(LOCK_KEY).fetch_one(&mut *connection).await?;
    if !locked {
        return Ok(false);
    }
    let outcome = sweep(state, &mut connection).await;
    let _released = sqlx::query("SELECT pg_advisory_unlock($1)").bind(LOCK_KEY).execute(&mut *connection).await?;
    outcome.map(|()| true)
}

async fn sweep(state: &AppState, connection: &mut PgConnection) -> ApiResult<()> {
    let now = state.clock.now();
    submit_expired(state, now).await?;
    apply_schedules(state, now).await?;
    cleanup(connection, now).await?;
    let retention = Duration::days(i64::from(state.config.signal_retention_days));
    let _expired = sqlx::query("DELETE FROM signals WHERE at < $1").bind(now - retention).execute(&mut *connection).await?;
    Ok(())
}

async fn submit_expired(state: &AppState, now: OffsetDateTime) -> ApiResult<()> {
    let expired: Vec<(Uuid, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, deadline_at FROM attempts WHERE submitted_at IS NULL AND deadline_at IS NOT NULL AND deadline_at < $1",
    )
    .bind(now)
    .fetch_all(&state.db)
    .await?;
    for (attempt_id, deadline) in expired {
        let mut tx = state.db.begin().await?;
        let _submitted = lifecycle::submit(&mut tx, attempt_id, SubmitReason::Timeout, deadline).await?;
        tx.commit().await?;
    }
    Ok(())
}

async fn apply_schedules(state: &AppState, now: OffsetDateTime) -> ApiResult<()> {
    let _opened = sqlx::query(
        "UPDATE test_windows SET is_open = true, open_at = NULL, closed_at = NULL, updated_at = $1
         WHERE NOT is_open AND open_at IS NOT NULL AND open_at <= $1 AND (close_at IS NULL OR close_at > $1)",
    )
    .bind(now)
    .execute(&state.db)
    .await?;
    let closing: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "UPDATE test_windows SET is_open = false, close_at = NULL, closed_at = $1, updated_at = $1
         WHERE close_at IS NOT NULL AND close_at <= $1 AND (is_open OR open_at IS NOT NULL)
         RETURNING test_id, stream_id",
    )
    .bind(now)
    .fetch_all(&state.db)
    .await?;
    for window in closing {
        let mut tx = state.db.begin().await?;
        lifecycle::close_untimed(&mut tx, window, now).await?;
        tx.commit().await?;
    }
    Ok(())
}

async fn cleanup(connection: &mut PgConnection, now: OffsetDateTime) -> ApiResult<()> {
    let statements = [
        ("DELETE FROM sessions WHERE last_seen_at < $1", now - Duration::days(30)),
        ("DELETE FROM oauth_states WHERE created_at < $1", now - Duration::hours(1)),
        ("DELETE FROM auth_tokens WHERE expires_at < $1", now - Duration::days(7)),
        ("DELETE FROM jobs WHERE done_at < $1", now - Duration::days(30)),
    ];
    for (statement, cutoff) in statements {
        let _removed = sqlx::query(statement).bind(cutoff).execute(&mut *connection).await?;
    }
    Ok(())
}

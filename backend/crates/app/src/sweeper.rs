//! Периодические задачи: автосдача по сроку, расписание окон, очистка (design D7).

use std::time::{Duration as StdDuration, Instant};

use sqlx::PgConnection;
use time::{Duration, OffsetDateTime};
use tokio::time::sleep;
use uuid::Uuid;

use crate::attempts::lifecycle::{self, SubmitReason};
use crate::audit::AuditEntry;
use crate::auth::onetime::LONGEST_LIFETIME;
use crate::error::ApiResult;
use crate::jobs::JobKind;
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Интервал между проходами.
const INTERVAL: StdDuration = StdDuration::from_secs(15);
/// Ключ advisory lock: при нескольких экземплярах свипер работает один.
const LOCK_KEY: i64 = 7_420_011;

/// Бесконечный цикл свипера.
///
/// cancel-safe: yes — каждый проход в своих транзакциях; отмена между ними безопасна.
pub(crate) async fn run(state: AppState) -> ! {
    loop {
        forget_stale_limits(&state);
        if let Err(error) = run_once(&state).await {
            tracing::error!(%error, "проход свипера не удался");
        }
        sleep(INTERVAL).await;
    }
}

/// Чистит лимиты в памяти процесса. Они у каждого экземпляра свои, поэтому — вне advisory lock.
fn forget_stale_limits(state: &AppState) {
    let now = Instant::now();
    state.login_failures.prune(now);
    for limiter in [&state.limits.login, &state.limits.register, &state.limits.mail] {
        limiter.prune(now);
    }
    state.uploads.prune(now);
    state.signals.prune(now);
}

/// Один проход; `false`, если блокировку держит другой экземпляр.
///
/// cancel-safe: yes — см. `run`.
pub(crate) async fn run_once(state: &AppState) -> ApiResult<bool> {
    let mut connection = state.db.acquire().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(LOCK_KEY)
        .fetch_one(&mut *connection)
        .await?;
    if !locked {
        return Ok(false);
    }
    let outcome = sweep(state, &mut connection).await;
    let _released = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(LOCK_KEY)
        .execute(&mut *connection)
        .await?;
    outcome.map(|()| true)
}

async fn sweep(state: &AppState, connection: &mut PgConnection) -> ApiResult<()> {
    let now = state.clock.now();
    submit_expired(state, now).await?;
    apply_schedules(state, now).await?;
    cleanup(connection, now).await?;
    let retention = Duration::days(i64::from(state.config.signal_retention_days));
    let _expired = sqlx::query("DELETE FROM signals WHERE at < $1")
        .bind(now.saturating_sub(retention))
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// Попытка и её срок.
type Deadline = (Uuid, OffsetDateTime);

/// Окно, изменённое по расписанию: тест, поток и курс теста.
type ScheduledWindow = (Uuid, Uuid, Uuid);

async fn submit_expired(state: &AppState, now: OffsetDateTime) -> ApiResult<()> {
    let expired: Vec<Deadline> = sqlx::query_as(
        "SELECT id, deadline_at FROM attempts WHERE submitted_at IS NULL AND deadline_at IS NOT NULL AND deadline_at < $1",
    )
    .bind(now)
    .fetch_all(&state.db)
    .await?;
    for (attempt_id, deadline) in expired {
        let mut tx = state.db.begin().await?;
        let _submitted =
            lifecycle::submit(&mut tx, attempt_id, SubmitReason::Timeout, deadline).await?;
        tx.commit().await?;
    }
    Ok(())
}

async fn apply_schedules(state: &AppState, now: OffsetDateTime) -> ApiResult<()> {
    open_scheduled(state, now).await?;
    close_scheduled(state, now).await
}

/// Открывает окна, у которых наступило время открытия, и пишет это в аудит-лог.
///
/// cancel-safe: yes — одна транзакция.
async fn open_scheduled(state: &AppState, now: OffsetDateTime) -> ApiResult<()> {
    let mut tx = state.db.begin().await?;
    let opened: Vec<ScheduledWindow> = sqlx::query_as(
        "UPDATE test_windows w SET is_open = true, open_at = NULL, closed_at = NULL, updated_at = $1
         FROM tests t
         WHERE t.id = w.test_id AND NOT w.is_open AND w.open_at IS NOT NULL AND w.open_at <= $1
           AND (w.close_at IS NULL OR w.close_at > $1)
         RETURNING w.test_id, w.stream_id, t.course_id",
    )
    .bind(now)
    .fetch_all(&mut *tx)
    .await?;
    for window in opened {
        audit_scheduled(&mut tx, "window.open", window).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Закрывает окна по расписанию и сдаёт в них попытки без лимита времени.
///
/// cancel-safe: yes — каждое окно в своей транзакции.
async fn close_scheduled(state: &AppState, now: OffsetDateTime) -> ApiResult<()> {
    let closing: Vec<ScheduledWindow> = sqlx::query_as(
        "UPDATE test_windows w SET is_open = false, close_at = NULL, closed_at = $1, updated_at = $1
         FROM tests t
         WHERE t.id = w.test_id AND w.close_at IS NOT NULL AND w.close_at <= $1
           AND (w.is_open OR w.open_at IS NOT NULL)
         RETURNING w.test_id, w.stream_id, t.course_id",
    )
    .bind(now)
    .fetch_all(&state.db)
    .await?;
    for window in closing {
        let (test_id, stream_id, _course_id) = window;
        let mut tx = state.db.begin().await?;
        lifecycle::close_untimed(&mut tx, (test_id, stream_id), now).await?;
        audit_scheduled(&mut tx, "window.close", window).await?;
        tx.commit().await?;
    }
    Ok(())
}

/// Запись аудит-лога об открытии или закрытии окна по расписанию (исполнитель — система).
///
/// cancel-safe: yes — в транзакции вызывающего.
async fn audit_scheduled(
    tx: &mut PgConnection,
    action: &'static str,
    (test_id, stream_id, course_id): ScheduledWindow,
) -> ApiResult<()> {
    AuditEntry::new(action, "test")
        .on(test_id)
        .in_course(course_id)
        .with(serde_json::json!({ "stream_id": stream_id, "test_id": test_id, "scheduled": true }))
        .record(&mut *tx)
        .await?;
    Ok(())
}

async fn cleanup(connection: &mut PgConnection, now: OffsetDateTime) -> ApiResult<()> {
    let statements = [
        ("DELETE FROM sessions WHERE last_seen_at < $1", now.saturating_sub(Duration::days(30))),
        ("DELETE FROM oauth_states WHERE created_at < $1", now.saturating_sub(Duration::hours(1))),
        ("DELETE FROM auth_tokens WHERE expires_at < $1", now.saturating_sub(Duration::days(7))),
        ("DELETE FROM jobs WHERE done_at < $1", now.saturating_sub(Duration::days(30))),
    ];
    for (statement, cutoff) in statements {
        let _removed = sqlx::query(statement).bind(cutoff).execute(&mut *connection).await?;
    }
    // Неотправленные письма старше срока жизни токенов: ссылки в них уже не работают,
    // а сырые токены в базе держать незачем (повторы писем укладываются в минуты).
    let _scrubbed = sqlx::query(
        "UPDATE jobs SET payload = '{}' WHERE kind = $1 AND done_at IS NULL AND payload <> '{}' AND created_at < $2",
    )
    .bind(JobKind::Email.name())
    .bind(now.saturating_sub(LONGEST_LIFETIME))
    .execute(&mut *connection)
    .await?;
    Ok(())
}

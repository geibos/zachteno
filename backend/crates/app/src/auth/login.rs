//! Вход по паролю и выход (spec identity-auth, «Вход по паролю», «Сессии»).

use std::net::IpAddr;
use std::time::Instant;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse as _, Response};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use utoipa::ToSchema;

use crate::audit::AuditEntry;
use crate::auth::account::{self, LoginRow};
use crate::auth::password::Verification;
use crate::auth::session::{self, ClientMeta, CurrentUser};
use crate::error::{ApiResult, AppError};
use crate::http::extract::{ClientIp, MaybeUser};
use crate::state::AppState;

/// После стольких неудач подряд включается нарастающая пауза.
const FREE_FAILURES: i32 = 5;
/// Верхняя граница паузы.
const MAX_BACKOFF_SECONDS: i64 = 900;

/// Данные входа.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Вход по email и паролю.
#[utoipa::path(post, path = "/api/auth/login", tag = "auth", request_body = LoginRequest,
    responses((status = 200, body = CurrentUser), (status = 401), (status = 429)))]
pub(crate) async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> ApiResult<Response> {
    state
        .login_limiter
        .check(ip, Instant::now())
        .map_err(|wait| AppError::TooManyRequests { retry_after_secs: wait.as_secs().max(1) })?;
    let Some(row) = account::find_for_login(&state.db, request.email.trim()).await? else {
        let _ = state.hasher.hash(request.password).await?;
        return Err(AppError::InvalidCredentials);
    };
    check_backoff(&row, state.clock.now())?;
    if row.blocked {
        return Err(AppError::Forbidden);
    }
    let outcome = match row.stored_hash() {
        Some(stored) => state.hasher.verify(request.password.clone(), stored).await?,
        None => Verification::Invalid,
    };
    succeed_or_fail(&state, &row, (outcome, request.password), ClientMeta::new(ip, &headers)).await
}

fn check_backoff(row: &LoginRow, now: OffsetDateTime) -> ApiResult<()> {
    let excess = row.failed_logins.saturating_sub(FREE_FAILURES);
    let (Some(last_failure), true) = (row.last_failed_login_at, excess >= 0) else {
        return Ok(());
    };
    let exponent = u32::try_from(excess.min(10)).unwrap_or(10);
    let pause = Duration::seconds(2_i64.saturating_pow(exponent).min(MAX_BACKOFF_SECONDS));
    let allowed_at = last_failure + pause;
    if now < allowed_at {
        let wait = u64::try_from((allowed_at - now).whole_seconds()).unwrap_or(1).max(1);
        return Err(AppError::TooManyRequests { retry_after_secs: wait });
    }
    Ok(())
}

async fn succeed_or_fail(
    state: &AppState,
    row: &LoginRow,
    (outcome, password): (Verification, String),
    meta: ClientMeta,
) -> ApiResult<Response> {
    match outcome {
        Verification::Invalid => {
            record_failure(state, row, meta.ip).await?;
            Err(AppError::InvalidCredentials)
        }
        Verification::ValidNeedsRehash => {
            let fresh = state.hasher.hash(password).await?;
            account::set_password(&state.db, row.id, &fresh).await?;
            open_session(state, row, &meta).await
        }
        Verification::Valid => open_session(state, row, &meta).await,
    }
}

async fn record_failure(state: &AppState, row: &LoginRow, ip: IpAddr) -> ApiResult<()> {
    let mut tx = state.db.begin().await?;
    let _updated = sqlx::query(
        "UPDATE users SET failed_logins = failed_logins + 1, last_failed_login_at = $2 WHERE id = $1",
    )
    .bind(row.id)
    .bind(state.clock.now())
    .execute(&mut *tx)
    .await?;
    AuditEntry::new("auth.login_failed", "user").by(None, Some(ip)).on(row.id).record(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

async fn open_session(state: &AppState, row: &LoginRow, meta: &ClientMeta) -> ApiResult<Response> {
    let mut tx = state.db.begin().await?;
    let _reset = sqlx::query("UPDATE users SET failed_logins = 0, last_failed_login_at = NULL WHERE id = $1")
        .bind(row.id)
        .execute(&mut *tx)
        .await?;
    let cookie = session::start(&mut *tx, row.id, meta).await?;
    AuditEntry::new("auth.login", "user").by(Some(row.id), Some(meta.ip)).on(row.id).record(&mut *tx).await?;
    let user = session::load_user(&mut *tx, row.id, uuid::Uuid::nil()).await?;
    tx.commit().await?;
    Ok((StatusCode::OK, AppendHeaders([(header::SET_COOKIE, cookie)]), Json(user)).into_response())
}

/// Выход: удаляет сессию на сервере и cookie в браузере.
#[utoipa::path(post, path = "/api/auth/logout", tag = "auth", responses((status = 204)))]
pub(crate) async fn logout(State(state): State<AppState>, MaybeUser(user): MaybeUser) -> ApiResult<Response> {
    if let Some(current) = user {
        let _deleted = sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(current.session_id)
            .execute(&state.db)
            .await?;
    }
    Ok((StatusCode::NO_CONTENT, AppendHeaders([(header::SET_COOKIE, session::clear_cookie())])).into_response())
}

/// Кто я и что доступно на этом стенде.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct MeResponse {
    pub user: Option<CurrentUser>,
    pub yandex_enabled: bool,
}

/// Текущий пользователь (или `null`).
#[utoipa::path(get, path = "/api/auth/me", tag = "auth", responses((status = 200, body = MeResponse)))]
pub(crate) async fn me(State(state): State<AppState>, MaybeUser(user): MaybeUser) -> Json<MeResponse> {
    Json(MeResponse { user, yandex_enabled: state.config.yandex.is_some() })
}

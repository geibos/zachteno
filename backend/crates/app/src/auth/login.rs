//! Вход по паролю и выход (spec identity-auth, «Вход по паролю», «Сессии»).

use std::net::IpAddr;
use std::time::Instant;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse as _, Response};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::audit::AuditEntry;
use crate::auth::account::{self, LoginRow};
use crate::auth::password::Verification;
use crate::auth::session::{self, ClientMeta, CurrentUser};
use crate::error::{ApiResult, AppError};
use crate::http::extract::{ClientIp, MaybeUser};
use crate::state::{AppState, throttle};

/// Данные входа.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Вход по email и паролю.
///
/// Ответ и пауза не зависят от того, существует ли учётная запись.
#[utoipa::path(post, path = "/api/auth/login", tag = "auth", request_body = LoginRequest,
    responses((status = 200, body = CurrentUser), (status = 401), (status = 429)))]
pub(crate) async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> ApiResult<Response> {
    throttle(&state.limits.login, ip)?;
    let email = account::normalize_email(&request.email).map_err(|_| AppError::InvalidCredentials)?;
    let key = email.to_ascii_lowercase();
    state
        .login_failures
        .try_begin(&key, Instant::now())
        .map_err(|wait| AppError::TooManyRequests { retry_after_secs: wait.as_secs().max(1) })?;
    let row = account::find_for_login(&state.db, &email).await?;
    let outcome = check_password(&state, row.as_ref(), request.password.clone()).await?;
    let Some(row) = row.filter(|_| outcome != Verification::Invalid) else {
        audit_failure(&state, ip, state.login_failures.count(&key)).await?;
        return Err(AppError::InvalidCredentials);
    };
    if row.blocked {
        return Err(AppError::Forbidden);
    }
    state.login_failures.reset(&key);
    if outcome == Verification::ValidNeedsRehash {
        let fresh = state.hasher.hash(request.password).await?;
        account::set_password(&state.db, row.id, &fresh).await?;
    }
    open_session(&state, &row, &ClientMeta::new(&state, ip, &headers)).await
}

/// Проверяет пароль; для несуществующей учётной записи тратит столько же времени.
async fn check_password(state: &AppState, row: Option<&LoginRow>, password: String) -> ApiResult<Verification> {
    match row.and_then(LoginRow::stored_hash) {
        Some(stored) => Ok(state.hasher.verify(password, stored).await?),
        None => {
            let _decoy = state.hasher.hash(password).await?;
            Ok(Verification::Invalid)
        }
    }
}

async fn audit_failure(state: &AppState, ip: IpAddr, failures: u32) -> ApiResult<()> {
    let details = serde_json::json!({ "consecutive_failures": failures });
    AuditEntry::new("auth.login_failed", "user").by(None, Some(ip)).with(details).record(&state.db).await?;
    Ok(())
}

async fn open_session(state: &AppState, row: &LoginRow, meta: &ClientMeta) -> ApiResult<Response> {
    let mut tx = state.db.begin().await?;
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
    /// Преподаёт ли пользователь хотя бы на одном курсе.
    pub teaches: bool,
}

/// Текущий пользователь (или `null`).
#[utoipa::path(get, path = "/api/auth/me", tag = "auth", responses((status = 200, body = MeResponse)))]
pub(crate) async fn me(State(state): State<AppState>, MaybeUser(user): MaybeUser) -> ApiResult<Json<MeResponse>> {
    let teaches = match &user {
        Some(current) => sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM course_teachers WHERE user_id = $1)")
            .bind(current.id)
            .fetch_one(&state.db)
            .await?,
        None => false,
    };
    Ok(Json(MeResponse { user, yandex_enabled: state.config.yandex.is_some(), teaches }))
}

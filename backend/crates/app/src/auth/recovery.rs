//! Подтверждение email и восстановление пароля (spec identity-auth).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::audit::AuditEntry;
use crate::auth::account::{self, LoginRow};
use crate::auth::onetime::{self, Purpose};
use crate::auth::password;
use crate::auth::register::send_verification;
use crate::auth::session::{self, CurrentUser};
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::mail::{self, Email};
use crate::state::{AppState, throttle};

/// Не чаще одного письма одного назначения пользователю за минуту.
const MAIL_COOLDOWN: time::Duration = time::Duration::minutes(1);

/// Токен из ссылки.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct TokenRequest {
    pub token: String,
}

/// Подтверждает email по ссылке из письма.
#[utoipa::path(post, path = "/api/auth/verify-email", tag = "auth", request_body = TokenRequest,
    responses((status = 204), (status = 400), (status = 410, description = "ссылка устарела")))]
pub(crate) async fn verify_email(
    State(state): State<AppState>,
    Json(request): Json<TokenRequest>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let now = state.clock.now();
    let user_id = onetime::consume(&mut tx, &[Purpose::VerifyEmail], &request.token, now).await?;
    let _verified = sqlx::query(
        "UPDATE users SET email_verified_at = $2 WHERE id = $1 AND email_verified_at IS NULL",
    )
    .bind(user_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Отправляет письмо подтверждения ещё раз.
#[utoipa::path(post, path = "/api/auth/verify-email/resend", tag = "auth", responses((status = 202)))]
pub(crate) async fn resend_verification(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    user: CurrentUser,
) -> ApiResult<StatusCode> {
    throttle(&state.limits.mail, ip)?;
    let since = state.clock.now().saturating_sub(MAIL_COOLDOWN);
    if user.email_verified
        || onetime::issued_recently(&state.db, Purpose::VerifyEmail, user.id, since).await?
    {
        return Ok(StatusCode::ACCEPTED);
    }
    let mut tx = state.db.begin().await?;
    send_verification(&state, &mut tx, user.id, (&user.email, &user.display_name)).await?;
    tx.commit().await?;
    state.jobs_wake.notify_one();
    Ok(StatusCode::ACCEPTED)
}

/// Email для восстановления пароля.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ForgotRequest {
    pub email: String,
}

/// Запрос восстановления: ответ одинаковый, есть такой email или нет.
#[utoipa::path(post, path = "/api/auth/password/forgot", tag = "auth", request_body = ForgotRequest,
    responses((status = 202)))]
pub(crate) async fn forgot_password(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(request): Json<ForgotRequest>,
) -> ApiResult<StatusCode> {
    throttle(&state.limits.mail, ip)?;
    let Ok(email) = account::normalize_email(&request.email) else {
        return Ok(StatusCode::ACCEPTED);
    };
    let Some(row) = account::find_for_login(&state.db, &email).await? else {
        return Ok(StatusCode::ACCEPTED);
    };
    let since = state.clock.now().saturating_sub(MAIL_COOLDOWN);
    if onetime::issued_recently(&state.db, Purpose::ResetPassword, row.id, since).await? {
        return Ok(StatusCode::ACCEPTED);
    }
    send_reset(&state, row).await?;
    state.jobs_wake.notify_one();
    Ok(StatusCode::ACCEPTED)
}

/// Выпускает токен восстановления и ставит письмо в очередь.
///
/// cancel-safe: yes — всё в одной транзакции.
async fn send_reset(state: &AppState, row: LoginRow) -> ApiResult<()> {
    let mut tx = state.db.begin().await?;
    let token = onetime::issue(&mut *tx, Purpose::ResetPassword, row.id, state.clock.now()).await?;
    let address: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(row.id)
        .fetch_one(&mut *tx)
        .await?;
    let letter = Email::ResetPassword { to: address, name: row.display_name, token };
    mail::queue(&mut *tx, &letter, state.clock.now()).await?;
    tx.commit().await?;
    Ok(())
}

/// Новый пароль по ссылке.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ResetRequest {
    pub token: String,
    pub password: String,
}

/// Задаёт пароль по ссылке восстановления (или из CLI) и завершает все сессии.
#[utoipa::path(post, path = "/api/auth/password/reset", tag = "auth", request_body = ResetRequest,
    responses((status = 204), (status = 400), (status = 410)))]
pub(crate) async fn reset_password(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(request): Json<ResetRequest>,
) -> ApiResult<StatusCode> {
    throttle(&state.limits.login, ip)?;
    password::check_strength(&request.password)
        .map_err(|weak| AppError::BadRequest(weak.to_string()))?;
    let purposes = [Purpose::ResetPassword, Purpose::SetPassword];
    onetime::ensure_usable(&state.db, &purposes, &request.token, state.clock.now()).await?;
    let hash = state.hasher.hash(request.password).await?;
    let mut tx = state.db.begin().await?;
    let user_id = onetime::consume(&mut tx, &purposes, &request.token, state.clock.now()).await?;
    account::set_password(&mut *tx, user_id, &hash).await?;
    session::revoke_all_except(&mut *tx, user_id, None).await?;
    AuditEntry::new("auth.password_reset", "user")
        .by(Some(user_id), Some(ip))
        .on(user_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

//! Профиль: смена пароля и активные сессии (spec identity-auth).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::audit::AuditEntry;
use crate::auth::account;
use crate::auth::login::begin_attempt;
use crate::auth::password::{self, Verification};
use crate::auth::session::{self, CurrentUser};
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::state::{AppState, throttle};

/// Смена или первое задание пароля.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ChangePasswordRequest {
    /// Текущий пароль; не нужен, если пароля ещё нет.
    #[serde(default, rename = "current_password")]
    pub current: Option<String>,
    #[serde(rename = "new_password")]
    pub replacement: String,
}

/// Меняет пароль и завершает остальные сессии.
///
/// Лимит по IP — как у входа, а неудачные проверки текущего пароля идут в общий со входом
/// счётчик по email: украденная сессия не даёт подбирать пароль и нагружать Argon2.
#[utoipa::path(post, path = "/api/profile/password", tag = "profile", request_body = ChangePasswordRequest,
    responses((status = 204), (status = 400), (status = 401), (status = 429)))]
pub(crate) async fn change_password(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    user: CurrentUser,
    Json(request): Json<ChangePasswordRequest>,
) -> ApiResult<StatusCode> {
    throttle(&state.limits.login, ip)?;
    password::check_strength(&request.replacement)
        .map_err(|weak| AppError::BadRequest(weak.to_string()))?;
    if user.has_password {
        verify_current(&state, &user, request.current.unwrap_or_default()).await?;
    }
    let hash = state.hasher.hash(request.replacement).await?;
    let mut tx = state.db.begin().await?;
    account::set_password(&mut *tx, user.id, &hash).await?;
    session::revoke_all_except(&mut *tx, user.id, Some(user.session_id)).await?;
    AuditEntry::new("auth.password_change", "user")
        .by(Some(user.id), Some(ip))
        .on(user.id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Проверяет текущий пароль; неудача остаётся в счётчике, успех его сбрасывает.
///
/// cancel-safe: yes — только чтение и проверка хеша; при отмене попытка остаётся неудачной,
/// как и при обычном отказе.
async fn verify_current(state: &AppState, user: &CurrentUser, current: String) -> ApiResult<()> {
    let key = user.email.to_ascii_lowercase();
    begin_attempt(state, &key)?;
    let row =
        account::find_for_login(&state.db, &user.email).await?.ok_or(AppError::Unauthorized)?;
    let stored = row.stored_hash().ok_or(AppError::InvalidCredentials)?;
    match state.hasher.verify(current, stored).await? {
        Verification::Valid | Verification::ValidNeedsRehash => {
            state.login_failures.reset(&key);
            Ok(())
        }
        Verification::Invalid => Err(AppError::InvalidCredentials),
    }
}

/// Активная сессия.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct SessionInfo {
    pub id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_seen_at: OffsetDateTime,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    #[sqlx(default)]
    pub current: bool,
}

/// Список моих активных сессий.
#[utoipa::path(get, path = "/api/profile/sessions", tag = "profile", responses((status = 200, body = [SessionInfo])))]
pub(crate) async fn list_sessions(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<SessionInfo>>> {
    let sessions: Vec<SessionInfo> = sqlx::query_as(
        "SELECT id, created_at, last_seen_at, user_agent, host(ip) AS ip, id = $2 AS current
         FROM sessions WHERE user_id = $1 ORDER BY last_seen_at DESC",
    )
    .bind(user.id)
    .bind(user.session_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(sessions))
}

/// Завершает одну из моих сессий.
#[utoipa::path(delete, path = "/api/profile/sessions/{session_id}", tag = "profile",
    params(("session_id" = Uuid, Path)), responses((status = 204)))]
pub(crate) async fn revoke_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let _deleted = sqlx::query("DELETE FROM sessions WHERE id = $1 AND user_id = $2")
        .bind(session_id)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Привязанный внешний аккаунт.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct Identity {
    pub provider: String,
    pub email: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Мои внешние аккаунты.
#[utoipa::path(get, path = "/api/profile/identities", tag = "profile", responses((status = 200, body = [Identity])))]
pub(crate) async fn list_identities(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Identity>>> {
    let identities = sqlx::query_as("SELECT provider, email, created_at FROM external_identities WHERE user_id = $1 ORDER BY created_at")
        .bind(user.id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(identities))
}

/// Отвязывает внешний аккаунт; последний способ входа удалить нельзя.
#[utoipa::path(delete, path = "/api/profile/identities/{provider}", tag = "profile", params(("provider" = String, Path)),
    responses((status = 204), (status = 409)))]
pub(crate) async fn unlink_identity(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(provider): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    ensure_other_login(&mut tx, &user, &provider).await?;
    let _deleted =
        sqlx::query("DELETE FROM external_identities WHERE user_id = $1 AND provider = $2")
            .bind(user.id)
            .bind(&provider)
            .execute(&mut *tx)
            .await?;
    AuditEntry::new("auth.identity_unlink", "user")
        .by(Some(user.id), Some(ip))
        .on(user.id)
        .with(serde_json::json!({ "provider": provider }))
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Конфликт, если без `provider` у пользователя не останется способа войти.
///
/// cancel-safe: yes — только чтение.
async fn ensure_other_login(
    tx: &mut PgConnection,
    user: &CurrentUser,
    provider: &str,
) -> ApiResult<()> {
    let others: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM external_identities WHERE user_id = $1 AND provider <> $2",
    )
    .bind(user.id)
    .bind(provider)
    .fetch_one(&mut *tx)
    .await?;
    if !user.has_password && others == 0 {
        return Err(AppError::Conflict(
            "Это единственный способ входа — сначала задайте пароль".to_owned(),
        ));
    }
    Ok(())
}

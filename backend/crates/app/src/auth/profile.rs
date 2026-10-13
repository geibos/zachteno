//! Профиль: смена пароля и активные сессии (spec identity-auth).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::audit::AuditEntry;
use crate::auth::account;
use crate::auth::password::{self, Verification};
use crate::auth::session::{self, CurrentUser};
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::state::AppState;

/// Смена или первое задание пароля.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ChangePasswordRequest {
    /// Текущий пароль; не нужен, если пароля ещё нет.
    #[serde(default)]
    pub current_password: Option<String>,
    pub new_password: String,
}

/// Меняет пароль и завершает остальные сессии.
#[utoipa::path(post, path = "/api/profile/password", tag = "profile", request_body = ChangePasswordRequest,
    responses((status = 204), (status = 400), (status = 401)))]
pub(crate) async fn change_password(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    user: CurrentUser,
    Json(request): Json<ChangePasswordRequest>,
) -> ApiResult<StatusCode> {
    password::check_strength(&request.new_password).map_err(|weak| AppError::BadRequest(weak.to_string()))?;
    if user.has_password {
        verify_current(&state, &user, request.current_password.unwrap_or_default()).await?;
    }
    let hash = state.hasher.hash(request.new_password).await?;
    let mut tx = state.db.begin().await?;
    account::set_password(&mut *tx, user.id, &hash).await?;
    session::revoke_all_except(&mut *tx, user.id, Some(user.session_id)).await?;
    AuditEntry::new("auth.password_change", "user").by(Some(user.id), Some(ip)).on(user.id).record(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn verify_current(state: &AppState, user: &CurrentUser, current: String) -> ApiResult<()> {
    let row = account::find_for_login(&state.db, &user.email).await?.ok_or(AppError::Unauthorized)?;
    let stored = row.stored_hash().ok_or(AppError::InvalidCredentials)?;
    match state.hasher.verify(current, stored).await? {
        Verification::Valid | Verification::ValidNeedsRehash => Ok(()),
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
pub(crate) async fn list_sessions(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Vec<SessionInfo>>> {
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

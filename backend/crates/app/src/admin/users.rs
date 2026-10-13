//! Пользователи: поиск, роли, блокировка.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::access;
use crate::audit::AuditEntry;
use crate::auth::session::{self, CurrentUser};
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::state::AppState;

/// Фильтр списка пользователей.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct UserQuery {
    /// Часть email или имени.
    #[serde(default)]
    pub search: String,
}

/// Пользователь в списке администратора.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct UserSummary {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub roles: Vec<String>,
    pub blocked: bool,
    pub email_verified: bool,
    pub trusted_registration: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Поиск пользователей.
#[utoipa::path(get, path = "/api/admin/users", tag = "admin", params(UserQuery),
    responses((status = 200, body = [UserSummary])))]
pub(crate) async fn list_users(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<UserQuery>,
) -> ApiResult<Json<Vec<UserSummary>>> {
    access::require_superuser(&user)?;
    let users = sqlx::query_as(
        "SELECT u.id, u.email, u.display_name, u.blocked_at IS NOT NULL AS blocked,
                u.email_verified_at IS NOT NULL AS email_verified, u.trusted_registration, u.created_at,
                coalesce(array_agg(r.role ORDER BY r.role) FILTER (WHERE r.role IS NOT NULL), '{}') AS roles
         FROM users u LEFT JOIN user_roles r ON r.user_id = u.id
         WHERE $1 = '' OR u.email ILIKE '%' || $1 || '%' OR u.display_name ILIKE '%' || $1 || '%'
         GROUP BY u.id ORDER BY u.created_at DESC LIMIT 200",
    )
    .bind(query.search.trim())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(users))
}

/// Глобальная роль.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GlobalRole {
    Superuser,
    Methodist,
}

impl GlobalRole {
    const fn name(self) -> &'static str {
        match self {
            Self::Superuser => "superuser",
            Self::Methodist => "methodist",
        }
    }
}

/// Выдать или снять роль.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RoleChange {
    pub role: GlobalRole,
    pub granted: bool,
}

/// Выдаёт или снимает роль методиста или суперпользователя.
#[utoipa::path(post, path = "/api/admin/users/{user_id}/roles", tag = "admin",
    params(("user_id" = Uuid, Path)), request_body = RoleChange, responses((status = 204), (status = 409)))]
pub(crate) async fn set_role(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(target): Path<Uuid>,
    Json(change): Json<RoleChange>,
) -> ApiResult<StatusCode> {
    access::require_superuser(&user)?;
    let mut tx = state.db.begin().await?;
    if change.granted {
        grant_role(&mut tx, target, change.role.name(), Some(user.id)).await?;
    } else {
        revoke_role(&mut tx, target, change.role).await?;
    }
    let action = if change.granted { "role.grant" } else { "role.revoke" };
    let details = serde_json::json!({ "role": change.role.name() });
    AuditEntry::new(action, "user").by(Some(user.id), Some(ip)).on(target).with(details).record(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Выдаёт глобальную роль (идемпотентно).
///
/// cancel-safe: yes — один INSERT.
pub(crate) async fn grant_role(tx: &mut PgConnection, target: Uuid, role: &str, by: Option<Uuid>) -> ApiResult<()> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)")
        .bind(target)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(AppError::NotFound);
    }
    let _inserted =
        sqlx::query("INSERT INTO user_roles (user_id, role, granted_by) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(target)
            .bind(role)
            .bind(by)
            .execute(&mut *tx)
            .await?;
    Ok(())
}

async fn revoke_role(tx: &mut PgConnection, target: Uuid, role: GlobalRole) -> ApiResult<()> {
    if role == GlobalRole::Superuser {
        let _locked = sqlx::query("SELECT 1 FROM user_roles WHERE role = 'superuser' FOR UPDATE").execute(&mut *tx).await?;
        let others: i64 = sqlx::query_scalar("SELECT count(*) FROM user_roles WHERE role = 'superuser' AND user_id <> $1")
            .bind(target)
            .fetch_one(&mut *tx)
            .await?;
        if others == 0 {
            return Err(AppError::Conflict("Нельзя снять роль с последнего суперпользователя".to_owned()));
        }
    }
    let _deleted = sqlx::query("DELETE FROM user_roles WHERE user_id = $1 AND role = $2")
        .bind(target)
        .bind(role.name())
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Заблокировать или разблокировать.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct BlockChange {
    pub blocked: bool,
}

/// Блокирует пользователя (с завершением всех сессий) или снимает блокировку.
#[utoipa::path(post, path = "/api/admin/users/{user_id}/block", tag = "admin",
    params(("user_id" = Uuid, Path)), request_body = BlockChange, responses((status = 204)))]
pub(crate) async fn set_blocked(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(target): Path<Uuid>,
    Json(change): Json<BlockChange>,
) -> ApiResult<StatusCode> {
    access::require_superuser(&user)?;
    if target == user.id && change.blocked {
        return Err(AppError::Conflict("Нельзя заблокировать самого себя".to_owned()));
    }
    let mut tx = state.db.begin().await?;
    let updated = sqlx::query("UPDATE users SET blocked_at = CASE WHEN $2 THEN $3 END WHERE id = $1")
        .bind(target)
        .bind(change.blocked)
        .bind(state.clock.now())
        .execute(&mut *tx)
        .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    if change.blocked {
        session::revoke_all_except(&mut *tx, target, None).await?;
    }
    let action = if change.blocked { "user.block" } else { "user.unblock" };
    AuditEntry::new(action, "user").by(Some(user.id), Some(ip)).on(target).record(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

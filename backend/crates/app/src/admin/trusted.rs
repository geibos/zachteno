//! Ссылки доверенной регистрации (spec identity-auth, «Доверенная регистрация»).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access;
use crate::audit::AuditEntry;
use crate::auth::session::CurrentUser;
use crate::error::ApiResult;
use crate::http::extract::ClientIp;
use crate::state::AppState;
use crate::tokens;

/// Новая ссылка.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct NewTrustedLink {
    #[serde(default)]
    pub label: String,
}

/// Выпущенная ссылка; полный URL показывается один раз.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct IssuedLink {
    pub id: Uuid,
    pub url: String,
}

/// Создаёт ссылку доверенной регистрации.
#[utoipa::path(post, path = "/api/admin/trusted-links", tag = "admin", request_body = NewTrustedLink,
    responses((status = 201, body = IssuedLink)))]
pub(crate) async fn create_link(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Json(request): Json<NewTrustedLink>,
) -> ApiResult<(StatusCode, Json<IssuedLink>)> {
    access::require_superuser(&user)?;
    let token = tokens::issue();
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO trusted_links (token_hash, label, created_by) VALUES ($1, $2, $3) RETURNING id")
        .bind(&token.hash)
        .bind(request.label.trim())
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    AuditEntry::new("trusted_link.create", "trusted_link").by(Some(user.id), Some(ip)).on(id).record(&mut *tx).await?;
    tx.commit().await?;
    let url = format!("{}/register?trusted={}", state.config.http.public_url, token.raw);
    Ok((StatusCode::CREATED, Json(IssuedLink { id, url })))
}

/// Ссылка в списке.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct TrustedLinkSummary {
    pub id: Uuid,
    pub label: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub uses: i64,
}

/// Список ссылок доверенной регистрации.
#[utoipa::path(get, path = "/api/admin/trusted-links", tag = "admin", responses((status = 200, body = [TrustedLinkSummary])))]
pub(crate) async fn list_links(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Vec<TrustedLinkSummary>>> {
    access::require_superuser(&user)?;
    let links = sqlx::query_as(
        "SELECT l.id, l.label, l.created_at, count(u.id) AS uses
         FROM trusted_links l LEFT JOIN trusted_link_uses u ON u.link_id = l.id
         GROUP BY l.id ORDER BY l.created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(links))
}

/// Использование ссылки.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct TrustedLinkUse {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub used_at: OffsetDateTime,
}

/// Журнал использования ссылки.
#[utoipa::path(get, path = "/api/admin/trusted-links/{link_id}/uses", tag = "admin",
    params(("link_id" = Uuid, Path)), responses((status = 200, body = [TrustedLinkUse])))]
pub(crate) async fn list_uses(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(link_id): Path<Uuid>,
) -> ApiResult<Json<Vec<TrustedLinkUse>>> {
    access::require_superuser(&user)?;
    let uses = sqlx::query_as(
        "SELECT u.user_id, p.email, p.display_name, u.used_at FROM trusted_link_uses u
         JOIN users p ON p.id = u.user_id WHERE u.link_id = $1 ORDER BY u.used_at",
    )
    .bind(link_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(uses))
}

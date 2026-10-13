//! Пригласительные ссылки и вступление в поток (spec streams-enrollment).

use std::net::IpAddr;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::audit::AuditEntry;
use crate::auth::session::CurrentUser;
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::state::AppState;
use crate::streams::access::require_manager;
use crate::tokens;

/// Новая ссылка.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct NewInvite {
    #[serde(default)]
    pub label: String,
}

/// Выпущенная ссылка; полный URL виден только при создании.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct IssuedInvite {
    pub id: Uuid,
    pub url: String,
}

const INSERT_INVITE: &str = "INSERT INTO invite_links (stream_id, token_hash, label, created_by) VALUES ($1, $2, $3, $4) RETURNING id";

/// Создаёт многоразовую пригласительную ссылку.
#[utoipa::path(post, path = "/api/streams/{stream_id}/invites", tag = "streams", params(("stream_id" = Uuid, Path)),
    request_body = NewInvite, responses((status = 201, body = IssuedInvite)))]
pub(crate) async fn create_invite(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(stream_id): Path<Uuid>,
    Json(request): Json<NewInvite>,
) -> ApiResult<(StatusCode, Json<IssuedInvite>)> {
    require_manager(&state.db, &user, stream_id).await?;
    let token = tokens::issue();
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(INSERT_INVITE)
        .bind(stream_id)
        .bind(&token.hash)
        .bind(request.label.trim())
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    AuditEntry::new("invite.create", "invite_link")
        .by(Some(user.id), Some(ip))
        .on(id)
        .with(serde_json::json!({ "stream_id": stream_id }))
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(IssuedInvite { id, url: invite_url(&state, &token.raw) })))
}

/// Адрес страницы приглашения.
fn invite_url(state: &AppState, raw: &str) -> String {
    format!("{}/invite/{raw}", state.config.http.public_url)
}

/// Ссылка в списке.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct InviteSummary {
    pub id: Uuid,
    pub label: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub uses: i64,
    pub students: i64,
}

/// Ссылки потока.
#[utoipa::path(get, path = "/api/streams/{stream_id}/invites", tag = "streams", params(("stream_id" = Uuid, Path)),
    responses((status = 200, body = [InviteSummary])))]
pub(crate) async fn list_invites(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(stream_id): Path<Uuid>,
) -> ApiResult<Json<Vec<InviteSummary>>> {
    require_manager(&state.db, &user, stream_id).await?;
    let invites = sqlx::query_as(
        "SELECT l.id, l.label, l.created_at, count(u.id) AS uses, count(DISTINCT u.user_id) AS students
         FROM invite_links l LEFT JOIN invite_link_uses u ON u.link_id = l.id
         WHERE l.stream_id = $1 GROUP BY l.id ORDER BY l.created_at DESC",
    )
    .bind(stream_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(invites))
}

/// Фильтр журнала.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct UsesQuery {
    #[serde(rename = "link_id")]
    pub link: Option<Uuid>,
    #[serde(rename = "user_id")]
    pub student: Option<Uuid>,
}

/// Переход по ссылке.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct InviteUse {
    pub link_id: Uuid,
    pub link_label: String,
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub used_at: OffsetDateTime,
}

/// Журнал переходов по ссылкам потока, с фильтрами по ссылке и студенту.
#[utoipa::path(get, path = "/api/streams/{stream_id}/invite-uses", tag = "streams",
    params(("stream_id" = Uuid, Path), UsesQuery), responses((status = 200, body = [InviteUse])))]
pub(crate) async fn invite_uses(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(stream_id): Path<Uuid>,
    Query(filter): Query<UsesQuery>,
) -> ApiResult<Json<Vec<InviteUse>>> {
    require_manager(&state.db, &user, stream_id).await?;
    let history = sqlx::query_as(
        "SELECT u.link_id, l.label AS link_label, u.user_id, p.email, p.display_name, u.used_at
         FROM invite_link_uses u JOIN invite_links l ON l.id = u.link_id JOIN users p ON p.id = u.user_id
         WHERE l.stream_id = $1 AND ($2::uuid IS NULL OR u.link_id = $2) AND ($3::uuid IS NULL OR u.user_id = $3)
         ORDER BY u.used_at",
    )
    .bind(stream_id)
    .bind(filter.link)
    .bind(filter.student)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(history))
}

/// Что видит посетитель на странице приглашения.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct InviteInfo {
    pub stream_name: String,
    pub courses: Vec<String>,
}

#[derive(FromRow)]
struct InviteRow {
    link_id: Uuid,
    stream_id: Uuid,
    stream_name: String,
}

async fn find(executor: &mut PgConnection, raw: &str) -> ApiResult<InviteRow> {
    sqlx::query_as(
        "SELECT l.id AS link_id, s.id AS stream_id, s.name AS stream_name
         FROM invite_links l JOIN streams s ON s.id = l.stream_id WHERE l.token_hash = $1",
    )
    .bind(tokens::digest(raw))
    .fetch_optional(&mut *executor)
    .await?
    .ok_or(AppError::NotFound)
}

/// Название потока по ссылке (доступно без входа).
#[utoipa::path(get, path = "/api/invites/{token}", tag = "streams", params(("token" = String, Path)),
    responses((status = 200, body = InviteInfo), (status = 404)))]
pub(crate) async fn invite_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<InviteInfo>> {
    let mut connection = state.db.acquire().await?;
    let invite = find(&mut connection, &token).await?;
    let courses = sqlx::query_scalar(
        "SELECT c.title FROM stream_courses sc JOIN courses c ON c.id = sc.course_id WHERE sc.stream_id = $1 ORDER BY c.title",
    )
    .bind(invite.stream_id)
    .fetch_all(&mut *connection)
    .await?;
    Ok(Json(InviteInfo { stream_name: invite.stream_name, courses }))
}

/// Результат вступления.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct Joined {
    pub stream_id: Uuid,
    pub stream_name: String,
    /// Был ли уже участником.
    pub already_member: bool,
}

/// Вступление в поток по ссылке; каждый переход записывается в журнал.
#[utoipa::path(post, path = "/api/invites/{token}/join", tag = "streams", params(("token" = String, Path)),
    responses((status = 200, body = Joined), (status = 404)))]
pub(crate) async fn join(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(token): Path<String>,
) -> ApiResult<Json<Joined>> {
    let mut tx = state.db.begin().await?;
    let invite = find(&mut tx, &token).await?;
    let joined =
        join_by_link(&mut tx, (invite.link_id, invite.stream_id), user.id, (ip, state.clock.now()))
            .await?;
    tx.commit().await?;
    Ok(Json(Joined {
        stream_id: invite.stream_id,
        stream_name: invite.stream_name,
        already_member: !joined,
    }))
}

const RECORD_USE: &str =
    "INSERT INTO invite_link_uses (link_id, user_id, used_at, ip) VALUES ($1, $2, $3, $4)";

/// Делает пользователя студентом потока и пишет переход в журнал. Возвращает, стал ли он участником сейчас.
///
/// cancel-safe: yes — в транзакции вызывающего.
pub(crate) async fn join_by_link(
    tx: &mut PgConnection,
    (link_id, stream_id): LinkAndStream,
    user_id: Uuid,
    (ip, now): (IpAddr, OffsetDateTime),
) -> ApiResult<bool> {
    let _used = sqlx::query(RECORD_USE)
        .bind(link_id)
        .bind(user_id)
        .bind(now)
        .bind(ip)
        .execute(&mut *tx)
        .await?;
    let joined = sqlx::query(
        "INSERT INTO stream_members (stream_id, user_id, joined_at) VALUES ($1, $2, $3)
         ON CONFLICT (stream_id, user_id) DO UPDATE SET removed_at = NULL, removed_by = NULL
         WHERE stream_members.removed_at IS NOT NULL",
    )
    .bind(stream_id)
    .bind(user_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    Ok(joined.rows_affected() > 0)
}

/// Ссылка и её поток.
type LinkAndStream = (Uuid, Uuid);

/// Вступление по хешу токена (после входа через внешний провайдер).
///
/// cancel-safe: yes — в транзакции вызывающего.
pub(crate) async fn join_by_hash(
    tx: &mut PgConnection,
    hash: &[u8],
    user_id: Uuid,
    (ip, now): (IpAddr, OffsetDateTime),
) -> ApiResult<()> {
    let found: Option<LinkAndStream> =
        sqlx::query_as("SELECT id, stream_id FROM invite_links WHERE token_hash = $1")
            .bind(hash)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(link) = found {
        let _joined = join_by_link(tx, link, user_id, (ip, now)).await?;
    }
    Ok(())
}

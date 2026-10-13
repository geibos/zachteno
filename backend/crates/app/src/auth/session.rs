//! Серверные сессии и текущий пользователь (spec identity-auth, «Сессии»).

use std::net::IpAddr;

use axum::http::{HeaderMap, HeaderValue, header};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use time::{Duration, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ApiResult, AppError};
use crate::state::AppState;
use crate::tokens;

/// Имя cookie сессии.
pub(crate) const COOKIE_NAME: &str = "__Host-session";
/// Сессия истекает после стольких дней неактивности.
const IDLE_DAYS: i64 = 30;
/// Как часто обновлять `last_seen_at`, чтобы не писать на каждый запрос.
const TOUCH_MINUTES: i64 = 5;

/// Глобальные роли пользователя.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
pub(crate) struct Roles {
    pub superuser: bool,
    pub methodist: bool,
}

impl Roles {
    fn from_names(names: &[String]) -> Self {
        Self {
            superuser: names.iter().any(|name| name == "superuser"),
            methodist: names.iter().any(|name| name == "methodist"),
        }
    }
}

/// Вошедший пользователь.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub roles: Roles,
    pub email_verified: bool,
    pub trusted_registration: bool,
    pub has_password: bool,
    /// Сессия этого запроса.
    #[serde(skip)]
    pub session_id: Uuid,
}

/// Признаки учётной записи, которые попадают в [`CurrentUser`].
#[derive(FromRow)]
struct AccountFlags {
    email_verified: bool,
    trusted_registration: bool,
    has_password: bool,
}

#[derive(FromRow)]
struct SessionRow {
    session_id: Uuid,
    user_id: Uuid,
    email: String,
    display_name: String,
    last_seen_at: OffsetDateTime,
    blocked: bool,
    #[sqlx(flatten)]
    flags: AccountFlags,
    roles: Vec<String>,
}

const SESSION_QUERY: &str = "
    SELECT s.id AS session_id, u.id AS user_id, u.email, u.display_name, s.last_seen_at,
           u.blocked_at IS NOT NULL AS blocked, u.email_verified_at IS NOT NULL AS email_verified,
           u.trusted_registration, u.password_hash IS NOT NULL AS has_password,
           coalesce(array_agg(r.role) FILTER (WHERE r.role IS NOT NULL), '{}') AS roles
    FROM sessions s
    JOIN users u ON u.id = s.user_id
    LEFT JOIN user_roles r ON r.user_id = u.id
    WHERE s.token_hash = $1
    GROUP BY s.token_hash, u.id";

/// Пользователь по cookie сессии; `None`, если сессии нет, она истекла или аккаунт заблокирован.
///
/// cancel-safe: yes — чтение и идемпотентное обновление `last_seen_at`.
pub(crate) async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> ApiResult<Option<CurrentUser>> {
    let Some(raw) = session_cookie(headers) else {
        return Ok(None);
    };
    let hash = tokens::digest(&raw);
    let found: Option<SessionRow> =
        sqlx::query_as(SESSION_QUERY).bind(&hash).fetch_optional(&state.db).await?;
    let now = state.clock.now();
    let Some(row) = found.filter(|session| !session.blocked && is_fresh(session.last_seen_at, now))
    else {
        return Ok(None);
    };
    touch(state, &hash, row.last_seen_at, now).await?;
    Ok(Some(into_user(row)))
}

/// Обновляет `last_seen_at`, если с прошлого обновления прошло больше `TOUCH_MINUTES`.
///
/// cancel-safe: yes — идемпотентный UPDATE.
async fn touch(
    state: &AppState,
    hash: &[u8],
    last_seen_at: OffsetDateTime,
    now: OffsetDateTime,
) -> ApiResult<()> {
    if now > last_seen_at.saturating_add(Duration::minutes(TOUCH_MINUTES)) {
        let _touched = sqlx::query("UPDATE sessions SET last_seen_at = $2 WHERE token_hash = $1")
            .bind(hash)
            .bind(now)
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

fn is_fresh(last_seen_at: OffsetDateTime, now: OffsetDateTime) -> bool {
    now <= last_seen_at.saturating_add(Duration::days(IDLE_DAYS))
}

fn into_user(row: SessionRow) -> CurrentUser {
    CurrentUser {
        id: row.user_id,
        email: row.email,
        display_name: row.display_name,
        roles: Roles::from_names(&row.roles),
        email_verified: row.flags.email_verified,
        trusted_registration: row.flags.trusted_registration,
        has_password: row.flags.has_password,
        session_id: row.session_id,
    }
}

fn session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|cookie| cookie.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE_NAME)
        .map(|(_, token)| token.to_owned())
}

/// Откуда пришёл запрос на вход.
#[derive(Debug, Clone)]
pub(crate) struct ClientMeta {
    pub ip: IpAddr,
    pub user_agent: Option<String>,
    pub now: OffsetDateTime,
}

impl ClientMeta {
    pub(crate) fn new(state: &AppState, ip: IpAddr, headers: &HeaderMap) -> Self {
        let user_agent = headers
            .get(header::USER_AGENT)
            .and_then(|agent| agent.to_str().ok())
            .map(|agent| agent.chars().take(400).collect());
        Self { ip, user_agent, now: state.clock.now() }
    }
}

/// Создаёт сессию и возвращает заголовок `Set-Cookie`.
///
/// cancel-safe: yes — один INSERT; при отмене cookie просто не уходит клиенту.
pub(crate) async fn start<'conn>(
    executor: impl PgExecutor<'conn>,
    user_id: Uuid,
    meta: &ClientMeta,
) -> ApiResult<HeaderValue> {
    let token = tokens::issue();
    let _inserted = sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, user_agent, ip, created_at, last_seen_at)
         VALUES ($1, $2, $3, $4, $5, $5)",
    )
    .bind(&token.hash)
    .bind(user_id)
    .bind(&meta.user_agent)
    .bind(meta.ip)
    .bind(meta.now)
    .execute(executor)
    .await?;
    let max_age = IDLE_DAYS * 24 * 3600;
    HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={max_age}",
        token.raw
    ))
    .map_err(|error| AppError::Internal(error.into()))
}

/// `Set-Cookie`, удаляющий сессию в браузере.
pub(crate) const fn clear_cookie() -> HeaderValue {
    HeaderValue::from_static("__Host-session=; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=0")
}

/// Завершает все сессии пользователя, кроме `keep`.
///
/// cancel-safe: yes — один DELETE.
pub(crate) async fn revoke_all_except<'conn>(
    executor: impl PgExecutor<'conn>,
    user_id: Uuid,
    keep: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    let _deleted =
        sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND ($2::uuid IS NULL OR id <> $2)")
            .bind(user_id)
            .bind(keep)
            .execute(executor)
            .await?;
    Ok(())
}

const USER_QUERY: &str = "
    SELECT u.id AS user_id, u.email, u.display_name,
           u.email_verified_at IS NOT NULL AS email_verified,
           u.trusted_registration, u.password_hash IS NOT NULL AS has_password,
           coalesce(array_agg(r.role) FILTER (WHERE r.role IS NOT NULL), '{}') AS roles
    FROM users u
    LEFT JOIN user_roles r ON r.user_id = u.id
    WHERE u.id = $1
    GROUP BY u.id";

#[derive(FromRow)]
struct UserRow {
    user_id: Uuid,
    email: String,
    display_name: String,
    email_verified: bool,
    trusted_registration: bool,
    has_password: bool,
    roles: Vec<String>,
}

/// Профиль пользователя по id (для ответов после регистрации и входа).
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn load_user<'conn>(
    executor: impl PgExecutor<'conn>,
    user_id: Uuid,
    session_id: Uuid,
) -> ApiResult<CurrentUser> {
    let row: UserRow = sqlx::query_as(USER_QUERY).bind(user_id).fetch_one(executor).await?;
    Ok(CurrentUser {
        id: row.user_id,
        email: row.email,
        display_name: row.display_name,
        roles: Roles::from_names(&row.roles),
        email_verified: row.email_verified,
        trusted_registration: row.trusted_registration,
        has_password: row.has_password,
        session_id,
    })
}

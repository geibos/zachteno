//! Вход через Яндекс ID: OAuth 2.0 + PKCE, `state` в cookie браузера (spec identity-auth, design D5).

use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse as _, Response};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use sqlx::{FromRow, PgConnection};
use time::Duration;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::audit::AuditEntry;
use crate::auth::account::{self, NewUser};
use crate::auth::register::record_trusted_use;
use crate::auth::session::{self, ClientMeta};
use crate::config::OAuthClient;
use crate::error::{ApiResult, AppError};
use crate::http::extract::{ClientIp, MaybeUser};
use crate::state::AppState;
use crate::streams::invites::join_by_hash;
use crate::tokens;

#[cfg(test)]
mod tests;

/// Провайдер в базе.
const PROVIDER: &str = "yandex";
/// Cookie, привязывающая `state` к браузеру.
const STATE_COOKIE: &str = "__Host-oauth";
/// Сколько живёт незавершённый вход.
const STATE_TTL: Duration = Duration::minutes(10);

/// Параметры начала входа.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct StartQuery {
    /// Куда вернуться после входа (только путь этого сайта).
    pub next: Option<String>,
    /// Токен ссылки доверенной регистрации.
    pub trusted: Option<String>,
    /// Токен приглашения в поток.
    pub invite: Option<String>,
    /// `link` — привязать Яндекс ID к текущей учётке.
    pub intent: Option<String>,
}

/// Только относительный путь этого сайта: защита от открытого редиректа.
pub(crate) fn safe_next(next: Option<&str>) -> String {
    next.filter(|path| path.starts_with('/') && !path.starts_with("//") && !path.contains('\\'))
        .map_or_else(|| "/".to_owned(), str::to_owned)
}

fn client(state: &AppState) -> ApiResult<&OAuthClient> {
    state.config.yandex.as_ref().ok_or(AppError::NotFound)
}

fn redirect_uri(state: &AppState) -> String {
    format!("{}/api/auth/oauth/yandex/callback", state.config.http.public_url)
}

/// Начинает вход: сохраняет `state` и PKCE и уводит на страницу Яндекса.
#[utoipa::path(get, path = "/api/auth/oauth/yandex/start", tag = "auth", operation_id = "yandex_start", params(StartQuery),
    responses((status = 303, description = "редирект на Яндекс"), (status = 404, description = "провайдер не настроен")))]
pub(crate) async fn start(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Query(query): Query<StartQuery>,
) -> ApiResult<Response> {
    let provider = client(&state)?;
    let state_token = tokens::issue();
    let verifier = tokens::issue().raw;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let link_user = user.filter(|_| query.intent.as_deref() == Some("link")).map(|current| current.id);
    let _saved = sqlx::query(
        "INSERT INTO oauth_states (state_hash, provider, pkce_verifier, link_user_id, invite_hash, trusted_hash, next_path, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&state_token.hash)
    .bind(PROVIDER)
    .bind(&verifier)
    .bind(link_user)
    .bind(query.invite.as_deref().map(tokens::digest))
    .bind(query.trusted.as_deref().map(tokens::digest))
    .bind(safe_next(query.next.as_deref()))
    .bind(state.clock.now())
    .execute(&state.db)
    .await?;
    let location = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&state={}&code_challenge={challenge}&code_challenge_method=S256",
        provider.authorize_url,
        urlencode(&provider.client_id),
        urlencode(&redirect_uri(&state)),
        state_token.raw
    );
    let cookie = format!("{STATE_COOKIE}={}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=600", state_token.raw);
    redirect(&location, Some(cookie))
}

fn urlencode(raw: &str) -> String {
    raw.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => char::from(byte).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn redirect(location: &str, cookie: Option<String>) -> ApiResult<Response> {
    let location = HeaderValue::from_str(location).map_err(|error| AppError::Internal(error.into()))?;
    let mut response = (StatusCode::SEE_OTHER, AppendHeaders([(header::LOCATION, location)])).into_response();
    for value in cookie {
        let cookie_header = HeaderValue::from_str(&value).map_err(|error| AppError::Internal(error.into()))?;
        let _appended = response.headers_mut().append(header::SET_COOKIE, cookie_header);
    }
    Ok(response)
}

/// Ответ провайдера.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

#[derive(FromRow)]
struct PendingLogin {
    pkce_verifier: String,
    link_user_id: Option<Uuid>,
    invite_hash: Option<Vec<u8>>,
    trusted_hash: Option<Vec<u8>>,
    next_path: Option<String>,
    created_at: time::OffsetDateTime,
}

/// Профиль из Яндекс ID.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct YandexProfile {
    pub id: String,
    pub default_email: Option<String>,
    pub real_name: Option<String>,
    pub display_name: Option<String>,
    pub login: Option<String>,
}

/// Возврат с Яндекса: проверка `state`, обмен кода, вход или регистрация.
#[utoipa::path(get, path = "/api/auth/oauth/yandex/callback", tag = "auth", params(CallbackQuery),
    responses((status = 303, description = "редирект в приложение")))]
pub(crate) async fn callback(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> ApiResult<Response> {
    match finish(&state, ip, &headers, &query).await {
        Ok((cookie, next)) => redirect(&next, Some(cookie)).map(clear_state_cookie),
        Err(reason) => {
            tracing::info!(reason, "вход через Яндекс не завершён");
            redirect(&format!("/login?oauth_error={reason}"), None).map(clear_state_cookie)
        }
    }
}

fn clear_state_cookie(mut response: Response) -> Response {
    let cleared = HeaderValue::from_static("__Host-oauth=; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=0");
    let _appended = response.headers_mut().append(header::SET_COOKIE, cleared);
    response
}

/// Код ошибки для страницы входа.
type Failure = &'static str;

async fn finish(state: &AppState, ip: std::net::IpAddr, headers: &HeaderMap, query: &CallbackQuery) -> Result<(String, String), Failure> {
    if query.error.is_some() {
        return Err("denied");
    }
    let (Some(code), Some(raw_state)) = (query.code.as_deref(), query.state.as_deref()) else { return Err("bad_request") };
    if state_cookie(headers).as_deref() != Some(raw_state) {
        return Err("state_mismatch");
    }
    let pending = take_pending(state, raw_state).await.map_err(|_| "internal")?.ok_or("state_mismatch")?;
    if state.clock.now() - pending.created_at > STATE_TTL {
        return Err("expired");
    }
    let profile = fetch_profile(state, code, &pending.pkce_verifier).await.map_err(|error| {
        tracing::warn!(%error, "обмен кода Яндекса не удался");
        "provider_error"
    })?;
    let next = pending.next_path.clone().unwrap_or_else(|| "/".to_owned());
    let cookie = sign_in(state, &profile, &pending, (ip, headers)).await?;
    Ok((cookie, next))
}

fn state_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|cookie| cookie.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == STATE_COOKIE)
        .map(|(_, token)| token.to_owned())
}

async fn take_pending(state: &AppState, raw_state: &str) -> Result<Option<PendingLogin>, sqlx::Error> {
    sqlx::query_as(
        "DELETE FROM oauth_states WHERE state_hash = $1 AND provider = $2
         RETURNING pkce_verifier, link_user_id, invite_hash, trusted_hash, next_path, created_at",
    )
    .bind(tokens::digest(raw_state))
    .bind(PROVIDER)
    .fetch_optional(&state.db)
    .await
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// Обменивает код на токен и читает профиль.
///
/// cancel-safe: yes — только сетевые запросы без изменения состояния.
async fn fetch_profile(state: &AppState, code: &str, verifier: &str) -> anyhow::Result<YandexProfile> {
    let provider = state.config.yandex.as_ref().ok_or_else(|| anyhow::anyhow!("Яндекс не настроен"))?;
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", provider.client_id.as_str()),
        ("client_secret", provider.client_secret.expose().as_str()),
        ("code_verifier", verifier),
        ("redirect_uri", &redirect_uri(state)),
    ];
    let token: TokenResponse = state.http_client.post(&provider.token_url).form(&form).send().await?.error_for_status()?.json().await?;
    Ok(state
        .http_client
        .get(&provider.userinfo_url)
        .query(&[("format", "json")])
        .header(header::AUTHORIZATION, format!("OAuth {}", token.access_token))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

async fn sign_in(
    state: &AppState,
    profile: &YandexProfile,
    pending: &PendingLogin,
    (ip, headers): (std::net::IpAddr, &HeaderMap),
) -> Result<String, Failure> {
    let mut tx = state.db.begin().await.map_err(|_| "internal")?;
    let user_id = resolve_user(state, &mut tx, profile, pending, ip).await?;
    let blocked: bool = sqlx::query_scalar("SELECT blocked_at IS NOT NULL FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| "internal")?;
    if blocked {
        return Err("blocked");
    }
    if let Some(hash) = &pending.invite_hash {
        join_by_hash(&mut tx, hash, user_id, (ip, state.clock.now())).await.map_err(|_| "internal")?;
    }
    let cookie = session::start(&mut *tx, user_id, &ClientMeta::new(state, ip, headers)).await.map_err(|_| "internal")?;
    let details = serde_json::json!({ "provider": PROVIDER });
    AuditEntry::new("auth.login", "user").by(Some(user_id), Some(ip)).on(user_id).with(details).record(&mut *tx).await.map_err(|_| "internal")?;
    tx.commit().await.map_err(|_| "internal")?;
    cookie.to_str().map(str::to_owned).map_err(|_| "internal")
}

async fn resolve_user(
    state: &AppState,
    tx: &mut PgConnection,
    profile: &YandexProfile,
    pending: &PendingLogin,
    ip: std::net::IpAddr,
) -> Result<Uuid, Failure> {
    let linked: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM external_identities WHERE provider = $1 AND subject = $2")
        .bind(PROVIDER)
        .bind(&profile.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "internal")?;
    match (linked, pending.link_user_id) {
        (Some(owner), Some(target)) if owner != target => Err("already_linked"),
        (Some(owner), _) => Ok(owner),
        (None, Some(target)) => link(tx, target, profile).await.map(|()| target),
        (None, None) => register(state, tx, profile, pending, ip).await,
    }
}

async fn link(tx: &mut PgConnection, user_id: Uuid, profile: &YandexProfile) -> Result<(), Failure> {
    let _linked = sqlx::query("INSERT INTO external_identities (provider, subject, user_id, email) VALUES ($1, $2, $3, $4)")
        .bind(PROVIDER)
        .bind(&profile.id)
        .bind(user_id)
        .bind(&profile.default_email)
        .execute(&mut *tx)
        .await
        .map_err(|_| "internal")?;
    Ok(())
}

async fn register(
    state: &AppState,
    tx: &mut PgConnection,
    profile: &YandexProfile,
    pending: &PendingLogin,
    ip: std::net::IpAddr,
) -> Result<Uuid, Failure> {
    let email = profile.default_email.as_deref().and_then(|raw| account::normalize_email(raw).ok()).ok_or("no_email")?;
    if account::email_taken(&mut *tx, &email).await.map_err(|_| "internal")? {
        return Err("link_required");
    }
    let trusted_link: Option<Uuid> = match &pending.trusted_hash {
        Some(hash) => sqlx::query_scalar("SELECT id FROM trusted_links WHERE token_hash = $1").bind(hash).fetch_optional(&mut *tx).await.map_err(|_| "internal")?,
        None => None,
    };
    let name = [&profile.real_name, &profile.display_name, &profile.login].into_iter().flatten().find(|name| !name.trim().is_empty()).cloned().unwrap_or_else(|| email.clone());
    let new_user = NewUser { email: &email, display_name: name.trim(), password: None, email_verified: true, trusted: trusted_link.is_some() };
    let user_id = account::create(&mut *tx, &new_user).await.map_err(|_| "link_required")?;
    link(tx, user_id, profile).await?;
    if let Some(link_id) = trusted_link {
        record_trusted_use(tx, link_id, user_id, ip).await.map_err(|_| "internal")?;
    }
    let _state = state;
    AuditEntry::new("user.register", "user").by(Some(user_id), Some(ip)).on(user_id).with(serde_json::json!({ "provider": PROVIDER })).record(&mut *tx).await.map_err(|_| "internal")?;
    Ok(user_id)
}

//! Регистрация по email и паролю, в том числе по доверенной ссылке (spec identity-auth).

use std::net::IpAddr;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse as _, Response};
use serde::Deserialize;
use sqlx::PgConnection;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::audit::AuditEntry;
use crate::auth::account::{self, NewUser};
use crate::auth::onetime::{self, Purpose};
use crate::auth::password::{self, StoredHash};
use crate::auth::session::{self, ClientMeta, CurrentUser};
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::mail::{self, Email};
use crate::state::{AppState, throttle};
use crate::tokens;

/// Данные регистрации.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RegisterRequest {
    pub email: String,
    pub display_name: String,
    pub password: String,
    /// Токен ссылки доверенной регистрации, если пришли по ней.
    #[serde(default)]
    pub trusted_token: Option<String>,
}

/// Регистрирует пользователя, открывает сессию и отправляет письмо подтверждения.
#[utoipa::path(post, path = "/api/auth/register", tag = "auth", request_body = RegisterRequest,
    responses((status = 201, body = CurrentUser), (status = 409, description = "email занят")))]
pub(crate) async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(request): Json<RegisterRequest>,
) -> ApiResult<Response> {
    throttle(&state.limits.register, ip)?;
    let (email, name) = validate(&state, &request).await?;
    let hash = state.hasher.hash(request.password).await?;
    let mut tx = state.db.begin().await?;
    let trusted_link = match &request.trusted_token {
        Some(raw) => trusted_link_id(&mut tx, raw).await?,
        None => None,
    };
    let fields = Fields { email: &email, name: &name, hash: &hash, trusted_link };
    let user_id = create_account(&state, &mut tx, &fields, ip).await?;
    let cookie = session::start(&mut *tx, user_id, &ClientMeta::new(&state, ip, &headers)).await?;
    let user = session::load_user(&mut *tx, user_id, Uuid::nil()).await?;
    tx.commit().await?;
    state.jobs_wake.notify_one();
    Ok((StatusCode::CREATED, AppendHeaders([(header::SET_COOKIE, cookie)]), Json(user))
        .into_response())
}

/// Нормализованные email и имя; пароль достаточно стойкий, email свободен.
///
/// cancel-safe: yes — только чтение.
async fn validate(state: &AppState, request: &RegisterRequest) -> ApiResult<(String, String)> {
    let email = account::normalize_email(&request.email)?;
    let name = account::normalize_name(&request.display_name)?;
    password::check_strength(&request.password)
        .map_err(|weak| AppError::BadRequest(weak.to_string()))?;
    if account::email_taken(&state.db, &email).await? {
        return Err(AppError::Conflict(
            "Этот email уже зарегистрирован. Войдите или восстановите пароль.".to_owned(),
        ));
    }
    Ok((email, name))
}

struct Fields<'fields> {
    email: &'fields str,
    name: &'fields str,
    hash: &'fields StoredHash,
    trusted_link: Option<Uuid>,
}

async fn create_account(
    state: &AppState,
    tx: &mut PgConnection,
    fields: &Fields<'_>,
    ip: IpAddr,
) -> ApiResult<Uuid> {
    let new_user = NewUser {
        email: fields.email,
        display_name: fields.name,
        password: Some(fields.hash),
        email_verified: false,
        trusted: fields.trusted_link.is_some(),
    };
    let user_id = account::create(&mut *tx, &new_user).await?;
    if let Some(link_id) = fields.trusted_link {
        record_trusted_use(&mut *tx, link_id, user_id, ip).await?;
    }
    send_verification(state, tx, user_id, (fields.email, fields.name)).await?;
    AuditEntry::new("user.register", "user")
        .by(Some(user_id), Some(ip))
        .on(user_id)
        .record(&mut *tx)
        .await?;
    Ok(user_id)
}

/// Ставит в очередь письмо со ссылкой подтверждения.
///
/// cancel-safe: yes — в транзакции вызывающего.
pub(crate) async fn send_verification(
    state: &AppState,
    tx: &mut PgConnection,
    user_id: Uuid,
    (email, name): (&str, &str),
) -> ApiResult<()> {
    let token = onetime::issue(&mut *tx, Purpose::VerifyEmail, user_id, state.clock.now()).await?;
    let letter = Email::VerifyAddress { to: email.to_owned(), name: name.to_owned(), token };
    mail::queue(&mut *tx, &letter).await?;
    Ok(())
}

/// Находит ссылку доверенной регистрации по сырому токену.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn trusted_link_id(tx: &mut PgConnection, raw: &str) -> ApiResult<Option<Uuid>> {
    let link: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM trusted_links WHERE token_hash = $1")
            .bind(tokens::digest(raw))
            .fetch_optional(&mut *tx)
            .await?;
    link.map(Some).ok_or_else(|| {
        AppError::BadRequest("Ссылка доверенной регистрации недействительна".to_owned())
    })
}

/// Записывает использование доверенной ссылки.
///
/// cancel-safe: yes — в транзакции вызывающего.
pub(crate) async fn record_trusted_use(
    tx: &mut PgConnection,
    link_id: Uuid,
    user_id: Uuid,
    ip: IpAddr,
) -> ApiResult<()> {
    let _inserted =
        sqlx::query("INSERT INTO trusted_link_uses (link_id, user_id, ip) VALUES ($1, $2, $3)")
            .bind(link_id)
            .bind(user_id)
            .bind(ip)
            .execute(&mut *tx)
            .await?;
    AuditEntry::new("trusted_link.use", "trusted_link")
        .by(Some(user_id), Some(ip))
        .on(link_id)
        .record(&mut *tx)
        .await?;
    Ok(())
}

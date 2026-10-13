//! Одноразовые токены ссылок: подтверждение email, сброс и задание пароля.

use sqlx::{FromRow, PgConnection, PgExecutor};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::error::AppError;
use crate::tokens;

/// Назначение токена и срок его жизни.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// Подтверждение email, 24 часа.
    VerifyEmail,
    /// Восстановление пароля, 1 час.
    ResetPassword,
    /// Первый пароль суперпользователя из CLI, 24 часа.
    SetPassword,
}

impl Purpose {
    const fn name(self) -> &'static str {
        match self {
            Self::VerifyEmail => "email_verify",
            Self::ResetPassword => "password_reset",
            Self::SetPassword => "set_password",
        }
    }

    const fn lifetime(self) -> Duration {
        match self {
            Self::VerifyEmail | Self::SetPassword => Duration::hours(24),
            Self::ResetPassword => Duration::hours(1),
        }
    }
}

/// Выпускает токен и возвращает его сырое значение для ссылки.
///
/// cancel-safe: yes — один INSERT.
pub(crate) async fn issue<'conn>(
    executor: impl PgExecutor<'conn>,
    purpose: Purpose,
    user_id: Uuid,
    now: OffsetDateTime,
) -> Result<String, sqlx::Error> {
    let token = tokens::issue();
    let _inserted =
        sqlx::query("INSERT INTO auth_tokens (token_hash, kind, user_id, expires_at) VALUES ($1, $2, $3, $4)")
            .bind(&token.hash)
            .bind(purpose.name())
            .bind(user_id)
            .bind(now + purpose.lifetime())
            .execute(executor)
            .await?;
    Ok(token.raw)
}

#[derive(FromRow)]
struct TokenRow {
    user_id: Uuid,
    expires_at: OffsetDateTime,
    used: bool,
}

/// Гасит токен одного из назначений `purposes` и возвращает пользователя.
///
/// cancel-safe: yes — работает в транзакции вызывающего.
pub(crate) async fn consume(
    connection: &mut PgConnection,
    purposes: &[Purpose],
    raw: &str,
    now: OffsetDateTime,
) -> Result<Uuid, AppError> {
    let names: Vec<&str> = purposes.iter().map(|purpose| purpose.name()).collect();
    let row: Option<TokenRow> = sqlx::query_as(
        "SELECT user_id, expires_at, used_at IS NOT NULL AS used FROM auth_tokens
         WHERE token_hash = $1 AND kind = ANY($2) FOR UPDATE",
    )
    .bind(tokens::digest(raw))
    .bind(&names)
    .fetch_optional(&mut *connection)
    .await?;
    let row = row.filter(|found| !found.used).ok_or_else(|| {
        AppError::BadRequest("Ссылка недействительна или уже использована".to_owned())
    })?;
    if row.expires_at < now {
        return Err(AppError::Gone("Ссылка устарела — запросите новую".to_owned()));
    }
    let _used = sqlx::query("UPDATE auth_tokens SET used_at = $2 WHERE token_hash = $1")
        .bind(tokens::digest(raw))
        .bind(now)
        .execute(&mut *connection)
        .await?;
    Ok(row.user_id)
}

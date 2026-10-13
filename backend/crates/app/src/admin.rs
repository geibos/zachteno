//! Раздел суперпользователя (spec `access-control`, «Права суперпользователя»).

mod trusted;
mod users;

#[cfg(test)]
mod tests;

pub(crate) use users::grant_role;

use sqlx::PgConnection;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::audit::AuditEntry;
use crate::auth::account::{self, NewUser};
use crate::auth::onetime::{self, Purpose};
use crate::state::AppState;

/// Маршруты администрирования.
pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(users::list_users))
        .routes(routes!(users::set_role))
        .routes(routes!(users::set_blocked))
        .routes(routes!(trusted::create_link, trusted::list_links))
        .routes(routes!(trusted::list_uses))
}

/// Создаёт суперпользователя или выдаёт роль; для учётки без пароля — ссылка на задание пароля.
///
/// cancel-safe: yes — всё в одной транзакции.
pub(crate) async fn bootstrap_superuser(
    state: &AppState,
    raw_email: &str,
    name: &str,
) -> anyhow::Result<Option<String>> {
    let email = account::normalize_email(raw_email).map_err(|error| anyhow::anyhow!("{error}"))?;
    let mut tx = state.db.begin().await?;
    let (user_id, has_password) = find_or_create(&mut tx, &email, name).await?;
    grant_role(&mut tx, user_id, "superuser", None)
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    AuditEntry::new("role.grant", "user")
        .on(user_id)
        .with(serde_json::json!({ "role": "superuser", "via": "cli" }))
        .record(&mut *tx)
        .await?;
    let link =
        if has_password { None } else { Some(password_link(state, &mut tx, user_id).await?) };
    tx.commit().await?;
    Ok(link)
}

/// Существующая учётка (защищённая [`secure_existing`]) или новая без пароля.
/// Возвращает id и признак, остался ли у учётки пароль.
///
/// cancel-safe: yes — в транзакции вызывающего.
async fn find_or_create(
    tx: &mut PgConnection,
    email: &str,
    name: &str,
) -> anyhow::Result<(Uuid, bool)> {
    if let Some(row) = account::find_for_login(&mut *tx, email).await? {
        return Ok((row.id, secure_existing(tx, row.id).await?));
    }
    let new_user =
        NewUser { email, display_name: name, password: None, email_verified: true, trusted: true };
    let user_id =
        account::create(&mut *tx, &new_user).await.map_err(|error| anyhow::anyhow!("{error}"))?;
    Ok((user_id, false))
}

/// Ссылка на задание пароля.
///
/// cancel-safe: yes — в транзакции вызывающего.
async fn password_link(
    state: &AppState,
    tx: &mut PgConnection,
    user_id: Uuid,
) -> anyhow::Result<String> {
    let token = onetime::issue(&mut *tx, Purpose::SetPassword, user_id, state.clock.now()).await?;
    Ok(format!("{}/reset-password?token={token}", state.config.http.public_url))
}

/// Учётку с неподтверждённым email мог заранее завести кто угодно: прежде чем выдать роль,
/// сбрасываем её пароль, сессии и внешние входы, а пароль задаётся только по ссылке из CLI.
/// Возвращает, остался ли у учётки пароль (то есть email был подтверждён владельцем).
///
/// cancel-safe: yes — в транзакции вызывающего.
async fn secure_existing(tx: &mut PgConnection, user_id: Uuid) -> anyhow::Result<bool> {
    let verified_with_password: bool = sqlx::query_scalar(
        "SELECT email_verified_at IS NOT NULL AND password_hash IS NOT NULL FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    if verified_with_password {
        return Ok(true);
    }
    for statement in [
        "UPDATE users SET password_hash = NULL, pepper_version = NULL, email_verified_at = now() WHERE id = $1",
        "DELETE FROM sessions WHERE user_id = $1",
        "DELETE FROM external_identities WHERE user_id = $1",
    ] {
        let _done = sqlx::query(statement).bind(user_id).execute(&mut *tx).await?;
    }
    Ok(false)
}

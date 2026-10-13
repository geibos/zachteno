//! Учётные записи: поиск, создание, проверка формата email.

use sqlx::{FromRow, PgExecutor};
use uuid::Uuid;

use crate::auth::password::StoredHash;
use crate::error::AppError;

/// Пользователь так, как его видит вход по паролю.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct LoginRow {
    pub id: Uuid,
    pub display_name: String,
    pub password_hash: Option<String>,
    pub pepper_version: Option<i32>,
    pub blocked: bool,
    pub failed_logins: i32,
    pub last_failed_login_at: Option<time::OffsetDateTime>,
}

impl LoginRow {
    /// Сохранённый хеш пароля, если пароль задан.
    pub(crate) fn stored_hash(&self) -> Option<StoredHash> {
        let phc = self.password_hash.clone()?;
        let pepper_version = u32::try_from(self.pepper_version?).ok()?;
        Some(StoredHash { phc, pepper_version })
    }
}

/// Ищет пользователя по email без учёта регистра.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn find_for_login<'conn>(
    executor: impl PgExecutor<'conn>,
    email: &str,
) -> Result<Option<LoginRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, display_name, password_hash, pepper_version, blocked_at IS NOT NULL AS blocked,
                failed_logins, last_failed_login_at
         FROM users WHERE email_lower = lower($1)",
    )
    .bind(email)
    .fetch_optional(executor)
    .await
}

/// Данные новой учётной записи.
#[derive(Debug, Clone)]
pub(crate) struct NewUser<'fields> {
    pub email: &'fields str,
    pub display_name: &'fields str,
    pub password: Option<&'fields StoredHash>,
    pub email_verified: bool,
    pub trusted: bool,
}

/// Создаёт пользователя; занятый email — конфликт без подробностей.
///
/// cancel-safe: yes — один INSERT.
pub(crate) async fn create<'conn>(
    executor: impl PgExecutor<'conn>,
    user: &NewUser<'_>,
) -> Result<Uuid, AppError> {
    let inserted = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash, pepper_version, email_verified_at, trusted_registration)
         VALUES ($1, $2, $3, $4, CASE WHEN $5 THEN now() END, $6) RETURNING id",
    )
    .bind(user.email)
    .bind(user.display_name)
    .bind(user.password.map(|hash| hash.phc.as_str()))
    .bind(user.password.and_then(|hash| i32::try_from(hash.pepper_version).ok()))
    .bind(user.email_verified)
    .bind(user.trusted)
    .fetch_one(executor)
    .await;
    inserted.map_err(|error| match error.as_database_error() {
        Some(db_error) if db_error.is_unique_violation() => {
            AppError::Conflict("Этот email уже зарегистрирован. Войдите или восстановите пароль.".to_owned())
        }
        Some(_) | None => error.into(),
    })
}

/// Сохраняет новый хеш пароля.
///
/// cancel-safe: yes — один UPDATE.
pub(crate) async fn set_password<'conn>(
    executor: impl PgExecutor<'conn>,
    user_id: Uuid,
    hash: &StoredHash,
) -> Result<(), sqlx::Error> {
    let _updated = sqlx::query("UPDATE users SET password_hash = $2, pepper_version = $3 WHERE id = $1")
        .bind(user_id)
        .bind(&hash.phc)
        .bind(i32::try_from(hash.pepper_version).unwrap_or(i32::MAX))
        .execute(executor)
        .await?;
    Ok(())
}

/// Нормализует и проверяет email: обрезка пробелов, одна `@`, точка в домене, до 254 символов.
pub(crate) fn normalize_email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim();
    let valid = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.contains('@')
        })
        && !email.contains(char::is_whitespace);
    if valid { Ok(email.to_owned()) } else { Err(AppError::BadRequest("Некорректный email".to_owned())) }
}

/// Проверяет отображаемое имя: 1–200 символов после обрезки.
pub(crate) fn normalize_name(raw: &str) -> Result<String, AppError> {
    let name = raw.trim();
    let length = name.chars().count();
    if (1..=200).contains(&length) {
        Ok(name.to_owned())
    } else {
        Err(AppError::BadRequest("Имя должно быть от 1 до 200 символов".to_owned()))
    }
}

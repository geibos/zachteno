//! Ошибки HTTP API и их отображение в ответы.

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Ошибка обработки запроса.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    /// Объекта нет или он недоступен (для студента это одно и то же).
    #[error("не найдено")]
    NotFound,
    /// Нужна авторизация.
    #[error("нужна авторизация")]
    Unauthorized,
    /// Неверный email или пароль (одинаково для обоих случаев).
    #[error("Неверный email или пароль")]
    InvalidCredentials,
    /// Авторизован, но прав нет.
    #[error("нет доступа")]
    Forbidden,
    /// Некорректный запрос; текст показывается пользователю.
    #[error("{0}")]
    BadRequest(String),
    /// Конфликт состояния; текст показывается пользователю.
    #[error("{0}")]
    Conflict(String),
    /// Ссылка или токен устарели.
    #[error("{0}")]
    Gone(String),
    /// Слишком частые запросы.
    #[error("слишком много попыток, повторите через {retry_after_secs} с")]
    TooManyRequests {
        /// Через сколько секунд можно повторить.
        retry_after_secs: u64,
    },
    /// Внутренняя ошибка; подробности только в логе.
    #[error("внутренняя ошибка")]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(source: sqlx::Error) -> Self {
        Self::Internal(source.into())
    }
}

/// Результат обработчика.
pub(crate) type ApiResult<T> = Result<T, AppError>;

#[derive(Serialize)]
struct ErrorBody<'message> {
    error: &'static str,
    message: &'message str,
}

impl AppError {
    const fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::Gone(_) => (StatusCode::GONE, "gone"),
            Self::TooManyRequests { .. } => (StatusCode::TOO_MANY_REQUESTS, "too_many_requests"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Self::Internal(source) = &self {
            tracing::error!(error = %source, "внутренняя ошибка");
        }
        let (status, code) = self.parts();
        let message = self.to_string();
        let mut response =
            (status, Json(ErrorBody { error: code, message: &message })).into_response();
        if let Self::TooManyRequests { retry_after_secs } = self
            && let Ok(header_value) = HeaderValue::from_str(&retry_after_secs.to_string())
        {
            let _previous = response.headers_mut().insert(header::RETRY_AFTER, header_value);
        }
        response
    }
}

//! Защитные middleware: CSRF по `Origin` и заголовки безопасности (spec identity-auth, D4).

use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};

use crate::error::AppError;
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Изменяющие запросы к `/api` принимаются только с `Origin` нашего сайта.
pub(crate) async fn csrf(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let safe = matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if safe || !request.uri().path().starts_with("/api/") {
        return next.run(request).await;
    }
    let origin = request.headers().get(header::ORIGIN).and_then(|sent| sent.to_str().ok());
    if origin.is_some_and(|sent| origin_allowed(&state, sent)) {
        next.run(request).await
    } else {
        let route = request.extensions().get::<MatchedPath>().map(MatchedPath::as_str);
        reject(origin, route)
    }
}

/// Отказ запросу без своего `Origin`. В журнал — шаблон маршрута, а не сырой путь:
/// в пути бывают токены ссылок (приглашения, подтверждения).
fn reject(origin: Option<&str>, route: Option<&str>) -> Response {
    tracing::warn!(origin = ?origin, route = ?route, "запрос без своего Origin отклонён");
    AppError::Forbidden.into_response()
}

fn origin_allowed(state: &AppState, origin: &str) -> bool {
    origin == state.config.http.public_url
        || state.config.http.extra_origins.iter().any(|extra| extra == origin)
}

/// Заголовки безопасности для всех ответов. Ответы `/api` по умолчанию не кешируются
/// (`no-store`): в них персональные данные; свой `Cache-Control` (картинки) не трогаем.
pub(crate) async fn security_headers(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let api = request.uri().path().starts_with("/api/");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    if api && !headers.contains_key(header::CACHE_CONTROL) {
        let _ = headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    let _ = headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    let _ = headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    let _ = headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    if state.config.http.public_url.starts_with("https://") {
        let hsts = HeaderValue::from_static("max-age=31536000; includeSubDomains");
        let _ = headers.insert(header::STRICT_TRANSPORT_SECURITY, hsts);
    }
    response
}

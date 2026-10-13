//! HTTP-слой: маршруты, защитные middleware, раздача фронтенда.

pub(crate) mod extract;
pub(crate) mod guard;
pub(crate) mod limiter;
pub(crate) mod spa;

use axum::Router;
use axum::middleware;
use axum::routing::get;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;
use crate::{admin, auth, ops};

/// Наибольший размер тела запроса (картинки до 10 МБ плюс запас).
const BODY_LIMIT_BYTES: usize = 12 * 1024 * 1024;

#[derive(OpenApi)]
#[openapi(info(title = "Зачтено API", description = "Сервис тестирования студентов"))]
struct ApiDoc;

/// Маршруты API без состояния — для генерации OpenAPI.
pub(crate) fn api() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(auth::routes()).merge(admin::routes())
}

/// Документ OpenAPI в JSON.
pub(crate) fn openapi_json() -> anyhow::Result<String> {
    let (_router, document) = api().split_for_parts();
    Ok(document.to_pretty_json()?)
}

/// Полное приложение со всеми слоями.
pub(crate) fn app(state: AppState) -> Router {
    let (router, _document) = api().split_for_parts();
    router
        .route("/health/live", get(ops::live))
        .route("/health/ready", get(ops::ready))
        .fallback(spa::serve)
        .layer(middleware::from_fn_with_state(state.clone(), guard::csrf))
        .layer(middleware::from_fn_with_state(state.clone(), guard::security_headers))
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT_BYTES))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

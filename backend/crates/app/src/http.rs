//! HTTP-слой: маршруты, защитные middleware, раздача фронтенда.

pub(crate) mod extract;
pub(crate) mod guard;
pub(crate) mod limiter;
pub(crate) mod spa;

use axum::Router;
use axum::body::Body;
use axum::extract::{MatchedPath, Request};
use axum::middleware;
use axum::routing::get;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{Level, Span};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;
use crate::{admin, antifraud, attempts, audit, auth, authoring, ops, reporting, streams};

/// Наибольший размер тела запроса (картинки до 10 МБ плюс запас).
const BODY_LIMIT_BYTES: usize = 12 * 1024 * 1024;

/// Заголовок с идентификатором запроса (ставит `SetRequestIdLayer`).
const REQUEST_ID: &str = "x-request-id";

#[derive(OpenApi)]
#[openapi(info(title = "Зачтено API", description = "Сервис тестирования студентов"))]
struct ApiDoc;

/// Маршруты API без состояния — для генерации `OpenAPI`.
pub(crate) fn api() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(auth::routes())
        .merge(admin::routes())
        .merge(authoring::routes())
        .merge(streams::routes())
        .merge(attempts::routes())
        .merge(reporting::routes())
        .merge(antifraud::routes())
        .merge(audit::routes())
}

/// Документ `OpenAPI` в JSON.
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
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(request_span)
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

/// Span запроса: метод, шаблон маршрута и `request_id`.
///
/// Сырой URI не пишется: в пути и строке запроса бывают токены ссылок (spec platform-operations,
/// «Наблюдаемость»). Для маршрутов фронтенда шаблона нет — поле пустое.
fn request_span(request: &Request<Body>) -> Span {
    let request_id =
        request.headers().get(REQUEST_ID).and_then(|header_value| header_value.to_str().ok());
    let route = request.extensions().get::<MatchedPath>().map(MatchedPath::as_str);
    tracing::info_span!("request", method = %request.method(), route, request_id)
}

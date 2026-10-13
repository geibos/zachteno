//! Раздача собранного фронтенда (`SvelteKit`, adapter-static) с SPA-fallback.
//!
//! CSP строится один раз: в `script-src` попадают хеши inline-скриптов `index.html`.

use std::sync::LazyLock;

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse as _, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest as _, Sha256};

use crate::error::AppError;

mod assets;
#[cfg(test)]
mod tests;

use assets::Assets;

/// Политика CSP для страниц приложения.
static CSP: LazyLock<String> = LazyLock::new(|| {
    let index = Assets::get("index.html").map(|file| file.data.into_owned()).unwrap_or_default();
    content_security_policy(&String::from_utf8_lossy(&index))
});

/// CSP с хешами inline-скриптов страницы.
pub(crate) fn content_security_policy(page: &str) -> String {
    let mut hashes = String::new();
    for script in inline_scripts(page) {
        hashes.push_str(" 'sha256-");
        hashes.push_str(&STANDARD.encode(Sha256::digest(script.as_bytes())));
        hashes.push('\'');
    }
    format!(
        "default-src 'self'; script-src 'self'{hashes}; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; \
         frame-ancestors 'none'; base-uri 'self'; form-action 'self'; object-src 'none'"
    )
}

fn inline_scripts(page: &str) -> impl Iterator<Item = &str> {
    page.split("<script").skip(1).filter_map(|chunk| {
        let (attributes, rest) = chunk.split_once('>')?;
        if attributes.contains("src=") {
            return None;
        }
        rest.split_once("</script>").map(|(body, _)| body)
    })
}

/// Отдаёт файл сборки или `index.html` для клиентских маршрутов. Прямой `/index.html`
/// тоже идёт через [`index_response`], чтобы страница не осталась без CSP.
pub(crate) async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return AppError::NotFound.into_response();
    }
    match Assets::get(path).filter(|_| !path.is_empty() && path != "index.html") {
        Some(file) => asset_response(path, file.data.into_owned()),
        None => index_response(),
    }
}

fn asset_response(path: &str, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path.starts_with("_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=300"
    };
    let mut response = Response::new(Body::from(bytes));
    insert(&mut response, header::CONTENT_TYPE, mime.as_ref());
    insert(&mut response, header::CACHE_CONTROL, cache);
    response
}

fn index_response() -> Response {
    let Some(index) = Assets::get("index.html") else {
        let mut unbuilt = (StatusCode::SERVICE_UNAVAILABLE, "Фронтенд не собран").into_response();
        insert(&mut unbuilt, header::CONTENT_SECURITY_POLICY, &CSP);
        return unbuilt;
    };
    let mut response = Response::new(Body::from(index.data.into_owned()));
    insert(&mut response, header::CONTENT_TYPE, "text/html; charset=utf-8");
    insert(&mut response, header::CACHE_CONTROL, "no-cache");
    insert(&mut response, header::CONTENT_SECURITY_POLICY, &CSP);
    response
}

fn insert(response: &mut Response, name: header::HeaderName, header_text: &str) {
    if let Ok(header_value) = HeaderValue::from_str(header_text) {
        let _previous = response.headers_mut().insert(name, header_value);
    }
}

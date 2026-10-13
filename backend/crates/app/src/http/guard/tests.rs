use axum::http::{HeaderMap, Method, header};
use sqlx::PgPool;

use crate::testing::{self, Client};

/// Заголовки, которые middleware ставит на любой ответ.
fn assert_common(headers: &HeaderMap, path: &str) {
    let get = |name: header::HeaderName| -> Option<&str> { headers.get(name)?.to_str().ok() };
    assert_eq!(get(header::X_CONTENT_TYPE_OPTIONS), Some("nosniff"), "nosniff на {path}");
    assert_eq!(get(header::REFERRER_POLICY), Some("no-referrer"), "no-referrer на {path}");
    assert_eq!(get(header::X_FRAME_OPTIONS), Some("DENY"), "запрет фреймов на {path}");
    assert_eq!(
        get(header::STRICT_TRANSPORT_SECURITY),
        Some("max-age=31536000; includeSubDomains"),
        "HSTS на {path}"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn index_page_has_strict_csp_and_security_headers(db: PgPool) {
    let state = testing::state_with_public_url(db, "https://zachteno.example");
    let response = Client::new(&state).raw(Method::GET, "/").await;
    assert_common(response.headers(), "/");
    let csp = response
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .and_then(|found| found.to_str().ok())
        .unwrap_or_default();
    let scripts =
        csp.split(';').map(str::trim).find(|directive| directive.starts_with("script-src"));
    assert!(
        scripts.is_some_and(|directive| directive.starts_with("script-src 'self'")
            && !directive.contains("unsafe")
            && !directive.contains('*')),
        "скрипты только свои, без unsafe-inline/eval: {csp}"
    );
    assert!(csp.contains("frame-ancestors 'none'"), "без фреймов: {csp}");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn api_responses_have_security_headers(db: PgPool) {
    let state = testing::state_with_public_url(db, "https://zachteno.example");
    let mut client = Client::new(&state);
    for path in ["/api/auth/me", "/api/courses"] {
        let response = client.raw(Method::GET, path).await;
        assert_common(response.headers(), path);
    }
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn plain_http_stand_has_no_hsts(db: PgPool) {
    let state = testing::state(db);
    let response = Client::new(&state).raw(Method::GET, "/api/auth/me").await;
    assert!(
        !response.headers().contains_key(header::STRICT_TRANSPORT_SECURITY),
        "HSTS только для HTTPS"
    );
}

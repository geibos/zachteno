use axum::Router;
use axum::http::{Method, StatusCode, header};
use axum::routing::{get, post};
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use tokio::net::TcpListener;

use crate::state::AppState;
use crate::testing::{self, Client, PASSWORD};

/// Мок Яндекса: токен по любому коду и профиль с заданным email.
async fn mock_yandex(email: &'static str) -> String {
    let app = Router::new()
        .route("/token", post(|| async { axum::Json(json!({ "access_token": "token" })) }))
        .route("/info", get(move || async move { axum::Json(json!({ "id": "ya-42", "default_email": email, "real_name": "Иван Яндексов" })) }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(tokio::spawn(async move { axum::serve(listener, app).await }));
    format!("http://{address}")
}

/// Проходит редирект на Яндекс и возвращает `state`.
async fn begin(client: &mut Client, query: &str) -> String {
    let response = client.raw(Method::GET, &format!("/api/auth/oauth/yandex/start{query}")).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER, "редирект на Яндекс");
    let location = response.headers()[header::LOCATION].to_str().unwrap().to_owned();
    assert!(location.contains("code_challenge_method=S256"), "PKCE: {location}");
    location.split("state=").nth(1).unwrap().split('&').next().unwrap().to_owned()
}

async fn finish(client: &mut Client, state_token: &str) -> String {
    let response = client.raw(Method::GET, &format!("/api/auth/oauth/yandex/callback?code=abc&state={state_token}")).await;
    response.headers()[header::LOCATION].to_str().unwrap().to_owned()
}

async fn me(client: &mut Client) -> Json {
    client.get("/api/auth/me").await.1
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn first_yandex_login_creates_verified_user(db: PgPool) {
    let state: AppState = testing::state_with_yandex(db, &mock_yandex("ivan@yandex.ru").await);
    let mut client = Client::new(&state);
    let state_token = begin(&mut client, "?next=/profile").await;
    assert_eq!(finish(&mut client, &state_token).await, "/profile", "возврат на next");
    let user = me(&mut client).await;
    assert_eq!(user["user"]["email"], "ivan@yandex.ru", "пользователь создан: {user}");
    assert_eq!(user["user"]["email_verified"], true, "email подтверждён Яндексом");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn matching_email_is_not_linked_automatically(db: PgPool) {
    let state = testing::state_with_yandex(db, &mock_yandex("ivan@example.com").await);
    let _existing = testing::register(&state, "ivan@example.com").await;
    let mut client = Client::new(&state);
    let state_token = begin(&mut client, "").await;
    assert_eq!(finish(&mut client, &state_token).await, "/login?oauth_error=link_required", "предлагает войти паролем");
    assert!(me(&mut client).await["user"].is_null(), "вход не выполнен");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn forged_state_is_rejected(db: PgPool) {
    let state = testing::state_with_yandex(db, &mock_yandex("ivan@yandex.ru").await);
    let mut victim = Client::new(&state);
    let attacker_state = begin(&mut Client::new(&state), "").await;
    assert_eq!(finish(&mut victim, &attacker_state).await, "/login?oauth_error=state_mismatch", "чужой state");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn logged_in_user_links_yandex_and_can_unlink(db: PgPool) {
    let state = testing::state_with_yandex(db, &mock_yandex("other@yandex.ru").await);
    let (mut client, _user) = testing::register(&state, "ivan@example.com").await;
    let state_token = begin(&mut client, "?intent=link&next=/profile").await;
    assert_eq!(finish(&mut client, &state_token).await, "/profile", "привязка");
    let (_, identities) = client.get("/api/profile/identities").await;
    assert_eq!(identities[0]["provider"], "yandex", "Яндекс привязан");
    let login = json!({ "email": "ivan@example.com", "password": PASSWORD });
    assert_eq!(Client::new(&state).post("/api/auth/login", login).await.0, StatusCode::OK, "пароль работает");
    assert_eq!(client.delete("/api/profile/identities/yandex").await.0, StatusCode::NO_CONTENT, "отвязка");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn last_login_method_cannot_be_unlinked(db: PgPool) {
    let state = testing::state_with_yandex(db, &mock_yandex("ivan@yandex.ru").await);
    let mut client = Client::new(&state);
    let state_token = begin(&mut client, "").await;
    let _done = finish(&mut client, &state_token).await;
    assert_eq!(client.delete("/api/profile/identities/yandex").await.0, StatusCode::CONFLICT, "единственный способ входа");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn yandex_registration_by_trusted_link_and_invite(db: PgPool) {
    let state = testing::state_with_yandex(db, &mock_yandex("new@yandex.ru").await);
    let mut room = testing::Classroom::new(&state, 1, json!({})).await;
    let (admin, admin_id) = testing::register(&state, "admin@example.com").await;
    testing::grant(&state, admin_id, "superuser").await;
    let mut admin = admin;
    let (_, link) = admin.post("/api/admin/trusted-links", json!({})).await;
    let trusted = link["url"].as_str().unwrap().split("trusted=").nth(1).unwrap().to_owned();
    let mut visitor = Client::new(&state);
    let state_token = begin(&mut visitor, &format!("?trusted={trusted}&invite={}", room.invite_token)).await;
    let _done = finish(&mut visitor, &state_token).await;
    assert_eq!(me(&mut visitor).await["user"]["trusted_registration"], true, "доверенная регистрация");
    let (_, stream) = room.methodist.get(&format!("/api/streams/{}", room.stream_id)).await;
    assert_eq!(stream["members"].as_array().map(Vec::len), Some(2), "сразу в потоке");
}

#[test]
fn next_path_rejects_open_redirect_tricks() {
    use super::safe_next;
    for evil in ["//evil.com", "/\\evil.com", "/\t/evil.com", "/\n/evil.com", "https://evil.com", "evil", "/ /evil"] {
        assert_eq!(safe_next(Some(evil)), "/", "отклонено: {evil:?}");
    }
    assert_eq!(safe_next(Some("/invite/abc?autojoin=1")), "/invite/abc?autojoin=1", "свой путь");
}

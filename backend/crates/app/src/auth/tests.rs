use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use time::Duration;
use tracing::Level;
use tracing::subscriber::set_default;

use crate::state::AppState;
use crate::testing::{self, Client, PASSWORD};

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn successful_registration_opens_session_and_queues_verification(db: PgPool) {
    let state = testing::state(db);
    let (mut client, _user) = testing::register(&state, "Ivan@Example.com").await;
    let (_, me) = client.get("/api/auth/me").await;
    assert_eq!(me["user"]["email"], "Ivan@Example.com", "сессия открыта: {me}");
    assert_eq!(me["user"]["email_verified"], false, "email ещё не подтверждён");
    let token = testing::last_mail_token(&state, "verify_email").await;
    assert!(token.len() >= 43, "ссылка подтверждения в очереди писем");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn taken_email_is_rejected_case_insensitively(db: PgPool) {
    let state = testing::state(db);
    let _first = testing::register(&state, "ivan@example.com").await;
    let mut other = Client::new(&state);
    let body =
        json!({ "email": "IVAN@example.com", "display_name": "Двойник", "password": PASSWORD });
    let (status, error) = other.post("/api/auth/register", body).await;
    assert_eq!(status, StatusCode::CONFLICT, "занятый email: {error}");
    assert!(
        !error["message"].as_str().unwrap().contains("ivan"),
        "сообщение не раскрывает владельца"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn weak_password_is_rejected_with_reason(db: PgPool) {
    let state = testing::state(db);
    let mut client = Client::new(&state);
    let body = json!({ "email": "a@b.ru", "display_name": "А", "password": "qwertyuiop123" });
    let (status, error) = client.post("/api/auth/register", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "распространённый пароль");
    assert!(error["message"].as_str().unwrap().contains("распространён"), "причина: {error}");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn wrong_password_and_unknown_email_look_the_same(db: PgPool) {
    let state = testing::state(db);
    let _user = testing::register(&state, "ivan@example.com").await;
    let mut client = Client::new(&state);
    let wrong = client
        .post(
            "/api/auth/login",
            json!({ "email": "ivan@example.com", "password": "не тот пароль 12" }),
        )
        .await;
    let unknown = client
        .post("/api/auth/login", json!({ "email": "nobody@example.com", "password": PASSWORD }))
        .await;
    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED, "неверный пароль");
    assert_eq!(wrong, unknown, "ответы одинаковые");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn sixth_failed_login_waits_and_is_audited(db: PgPool) {
    let state = testing::state(db);
    let _user = testing::register(&state, "ivan@example.com").await;
    let mut client = Client::new(&state);
    let attempt = json!({ "email": "ivan@example.com", "password": "не тот пароль 12" });
    for _ in 0..5 {
        assert_eq!(
            client.post("/api/auth/login", attempt.clone()).await.0,
            StatusCode::UNAUTHORIZED,
            "неудача"
        );
    }
    assert_eq!(
        client.post("/api/auth/login", attempt).await.0,
        StatusCode::TOO_MANY_REQUESTS,
        "пауза"
    );
    let audited: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'auth.login_failed'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(audited, 5, "каждая неудача в аудит-логе");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn unknown_email_gets_same_backoff(db: PgPool) {
    let state = testing::state(db);
    let mut client = Client::new(&state);
    let attempt = json!({ "email": "ghost@example.com", "password": "не тот пароль 12" });
    for _ in 0..5 {
        let _failed = client.post("/api/auth/login", attempt.clone()).await;
    }
    assert_eq!(
        client.post("/api/auth/login", attempt).await.0,
        StatusCode::TOO_MANY_REQUESTS,
        "пауза не выдаёт отсутствие"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn logout_invalidates_session(db: PgPool) {
    let state = testing::state(db);
    let (mut client, _user) = testing::register(&state, "ivan@example.com").await;
    let stolen = client.anonymous_with_same_cookie();
    assert_eq!(client.post("/api/auth/logout", json!({})).await.0, StatusCode::NO_CONTENT, "выход");
    let mut replay = stolen;
    let (_, me) = replay.get("/api/auth/me").await;
    assert!(me["user"].is_null(), "старая cookie больше не действует");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn email_verification_link_works_once(db: PgPool) {
    let state = testing::state(db);
    let (mut client, _user) = testing::register(&state, "ivan@example.com").await;
    let token = testing::last_mail_token(&state, "verify_email").await;
    assert_eq!(
        client.post("/api/auth/verify-email", json!({ "token": token })).await.0,
        StatusCode::NO_CONTENT,
        "подтверждение"
    );
    let (_, me) = client.get("/api/auth/me").await;
    assert_eq!(me["user"]["email_verified"], true, "email подтверждён");
    assert_eq!(
        client.post("/api/auth/verify-email", json!({ "token": token })).await.0,
        StatusCode::BAD_REQUEST,
        "второй раз нельзя"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn expired_verification_link_is_gone(db: PgPool) {
    let state = testing::state(db);
    let (mut client, _user) = testing::register(&state, "ivan@example.com").await;
    let token = testing::last_mail_token(&state, "verify_email").await;
    state.clock.advance(Duration::hours(25));
    assert_eq!(
        client.post("/api/auth/verify-email", json!({ "token": token })).await.0,
        StatusCode::GONE,
        "устарела"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn forgot_password_does_not_reveal_unknown_email(db: PgPool) {
    let state = testing::state(db);
    let mut client = Client::new(&state);
    let (status, _) =
        client.post("/api/auth/password/forgot", json!({ "email": "ghost@example.com" })).await;
    assert_eq!(status, StatusCode::ACCEPTED, "тот же ответ");
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs").fetch_one(&state.db).await.unwrap();
    assert_eq!(queued, 0, "письмо не отправляется");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn password_reset_revokes_all_sessions(db: PgPool) {
    let state = testing::state(db);
    let (mut session, _user) = testing::register(&state, "ivan@example.com").await;
    let mut visitor = Client::new(&state);
    let _ = visitor.post("/api/auth/password/forgot", json!({ "email": "ivan@example.com" })).await;
    let token = testing::last_mail_token(&state, "reset_password").await;
    let body = json!({ "token": token, "password": "новый очень длинный пароль" });
    assert_eq!(
        visitor.post("/api/auth/password/reset", body).await.0,
        StatusCode::NO_CONTENT,
        "пароль сменён"
    );
    let (_, me) = session.get("/api/auth/me").await;
    assert!(me["user"].is_null(), "старые сессии завершены");
    let login = json!({ "email": "ivan@example.com", "password": "новый очень длинный пароль" });
    assert_eq!(
        visitor.post("/api/auth/login", login).await.0,
        StatusCode::OK,
        "вход новым паролем"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn cross_site_request_is_rejected(db: PgPool) {
    let state = testing::state(db);
    let (client, _user) = testing::register(&state, "ivan@example.com").await;
    let (status, _) =
        client.send_with_origin(Method::POST, "/api/auth/logout", "https://evil.example").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "чужой Origin");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn blocked_user_cannot_log_in_even_with_right_password(db: PgPool) {
    let state = testing::state(db);
    let (_client, user) = testing::register(&state, "ivan@example.com").await;
    let _blocked = sqlx::query("UPDATE users SET blocked_at = now() WHERE id = $1")
        .bind(user)
        .execute(&state.db)
        .await
        .unwrap();
    let mut client = Client::new(&state);
    let (status, _) = client
        .post("/api/auth/login", json!({ "email": "ivan@example.com", "password": PASSWORD }))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "заблокирован");
}

/// Общий буфер байтов.
type Buffer = Arc<Mutex<Vec<u8>>>;

/// Буфер, куда подписчик tracing пишет JSON-логи.
#[derive(Clone, Default)]
struct Captured(Buffer);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|poisoned| io::Error::other(poisoned.to_string()))?
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// Регистрация, вход, сброс пароля и запрос с сессией; возвращает секреты, которых не должно быть в логах.
async fn login_flow(state: &AppState) -> Vec<String> {
    let (_, _) = testing::register(state, "ivan@example.com").await;
    let mut client = Client::new(state);
    let login = json!({ "email": "ivan@example.com", "password": PASSWORD });
    assert_eq!(client.post("/api/auth/login", login).await.0, StatusCode::OK, "вход");
    let cookie = client.session_cookie().unwrap().to_owned();
    let _me = client.get("/api/auth/me").await;
    let _ = client.post("/api/auth/password/forgot", json!({ "email": "ivan@example.com" })).await;
    let token = testing::last_mail_token(state, "reset_password").await;
    let fresh = "совсем новый секретный пароль 5";
    let reset = json!({ "token": token, "password": fresh });
    assert_eq!(
        client.post("/api/auth/password/reset", reset).await.0,
        StatusCode::NO_CONTENT,
        "сброс"
    );
    let verify = testing::last_mail_token(state, "verify_email").await;
    let session_value = cookie.split_once('=').map(|(_, value)| value.to_owned()).unwrap();
    vec![PASSWORD.to_owned(), fresh.to_owned(), token, verify, session_value]
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn logs_have_requests_but_no_passwords_tokens_or_cookies(db: PgPool) {
    let state = testing::state(db);
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_max_level(Level::TRACE)
        .with_current_span(true)
        .with_span_list(true)
        .with_writer(move || writer.clone())
        .finish();
    let guard = set_default(subscriber);
    let secrets = login_flow(&state).await;
    drop(guard);
    let logs = captured.text();
    assert!(logs.contains("/api/auth/login"), "запрос входа есть в логе");
    assert!(logs.contains("request_id"), "у записей есть идентификатор запроса");
    for secret in &secrets {
        assert!(!logs.contains(secret.as_str()), "в логе нет секрета «{secret}»");
    }
}

//! Тестовая обвязка: состояние на чистой базе, клиент к роутеру, фабрики данных.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt as _;
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use time::macros::datetime;
use tokio::sync::Notify;
use tower::ServiceExt as _;
use uuid::Uuid;

use crate::auth::password::Hasher;
use crate::clock::Clock;
use crate::config::{Config, DatabaseConfig, HttpConfig, MailConfig, Secrets, StorageConfig};
use crate::http::limiter::LoginFailures;
use crate::secret::Secret;
use crate::state::{AppState, Limits};

pub(crate) const ORIGIN: &str = "http://test.local";
pub(crate) const PASSWORD: &str = "корова летит на север 7";

fn config() -> Config {
    Config {
        http: HttpConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            metrics_bind: "127.0.0.1:0".parse().unwrap(),
            public_url: ORIGIN.to_owned(),
            extra_origins: Vec::new(),
            trusted_proxies: vec!["10.0.0.0/8".parse().unwrap()],
        },
        database: DatabaseConfig { app: Secret::new(String::new()), migrations: Secret::new(String::new()) },
        secrets: Secrets {
            peppers: Secret::new(BTreeMap::from([(1, vec![7; 32])])),
            signal_key: Secret::new(vec![9; 32]),
        },
        storage: StorageConfig {
            endpoint: String::new(),
            region: "garage".to_owned(),
            bucket: "test".to_owned(),
            access_key_id: String::new(),
            secret_access_key: Secret::new(String::new()),
        },
        mail: MailConfig::Directory { path: std::env::temp_dir().display().to_string(), from: "t <t@test.local>".to_owned() },
        yandex: None,
        signal_retention_days: 365,
    }
}

/// Состояние на тестовой базе с управляемыми часами.
pub(crate) fn state(db: PgPool) -> AppState {
    let config = config();
    AppState {
        db,
        hasher: Hasher::new(config.secrets.peppers.clone(), 4),
        config: Arc::new(config),
        clock: Clock::manual(datetime!(2026-10-13 09:00 UTC)),
        jobs_wake: Arc::new(Notify::new()),
        limits: Arc::new(Limits::relaxed()),
        login_failures: Arc::new(LoginFailures::default()),
        http_client: reqwest::Client::new(),
        blobs: Arc::new(object_store::memory::InMemory::new()),
    }
}

/// Клиент к приложению: хранит cookie сессии и ставит свой `Origin`.
pub(crate) struct Client {
    app: Router,
    cookie: Option<String>,
}

impl Client {
    pub(crate) fn new(state: &AppState) -> Self {
        Self { app: crate::http::app(state.clone()), cookie: None }
    }

    pub(crate) fn anonymous(&self) -> Self {
        Self { app: self.app.clone(), cookie: None }
    }

    /// Копия клиента с той же cookie (как украденная сессия).
    pub(crate) fn anonymous_with_same_cookie(&self) -> Self {
        Self { app: self.app.clone(), cookie: self.cookie.clone() }
    }

    /// Запрос с произвольным `Origin`.
    pub(crate) async fn send_with_origin(&mut self, method: Method, path: &str, origin: &str) -> (StatusCode, Json) {
        let mut builder = Request::builder().method(method).uri(path).header(header::ORIGIN, origin);
        if let Some(cookie) = &self.cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        let request = builder.header(header::CONTENT_TYPE, "application/json").body(Body::from("{}")).unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        (response.status(), Json::Null)
    }

    pub(crate) async fn send(&mut self, method: Method, path: &str, body: Option<Json>) -> (StatusCode, Json) {
        let mut builder = Request::builder().method(method).uri(path).header(header::ORIGIN, ORIGIN);
        if let Some(cookie) = &self.cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        let request = match body {
            Some(payload) => builder.header(header::CONTENT_TYPE, "application/json").body(Body::from(payload.to_string())),
            None => builder.body(Body::empty()),
        }
        .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        self.remember_cookie(response.headers());
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Json::Null))
    }

    fn remember_cookie(&mut self, headers: &axum::http::HeaderMap) {
        let Some(set) = headers.get(header::SET_COOKIE).and_then(|cookie| cookie.to_str().ok()) else { return };
        let pair = set.split(';').next().unwrap_or_default().to_owned();
        self.cookie = if pair.ends_with('=') { None } else { Some(pair) };
    }

    pub(crate) async fn get(&mut self, path: &str) -> (StatusCode, Json) {
        self.send(Method::GET, path, None).await
    }

    pub(crate) async fn post(&mut self, path: &str, body: Json) -> (StatusCode, Json) {
        self.send(Method::POST, path, Some(body)).await
    }

    pub(crate) async fn put(&mut self, path: &str, body: Json) -> (StatusCode, Json) {
        self.send(Method::PUT, path, Some(body)).await
    }

    pub(crate) async fn patch(&mut self, path: &str, body: Json) -> (StatusCode, Json) {
        self.send(Method::PATCH, path, Some(body)).await
    }

    pub(crate) async fn delete(&mut self, path: &str) -> (StatusCode, Json) {
        self.send(Method::DELETE, path, None).await
    }
}

/// Регистрирует пользователя и возвращает клиент с его сессией и id.
pub(crate) async fn register(state: &AppState, email: &str) -> (Client, Uuid) {
    let mut client = Client::new(state);
    let (status, body) = client
        .post("/api/auth/register", json!({ "email": email, "display_name": email, "password": PASSWORD }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "регистрация {email}: {body}");
    let user_id = body["id"].as_str().and_then(|id| id.parse().ok()).unwrap();
    (client, user_id)
}

/// Выдаёт глобальную роль напрямую в базе.
pub(crate) async fn grant(state: &AppState, user_id: Uuid, role: &str) {
    let _granted = sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1, $2)")
        .bind(user_id)
        .bind(role)
        .execute(&state.db)
        .await
        .unwrap();
}

/// Последний сырой токен из письма заданного шаблона.
pub(crate) async fn last_mail_token(state: &AppState, template: &str) -> String {
    let payload: Json = sqlx::query_scalar(
        "SELECT payload FROM jobs WHERE kind = 'email' AND payload->>'template' = $1 ORDER BY id DESC LIMIT 1",
    )
    .bind(template)
    .fetch_one(&state.db)
    .await
    .unwrap();
    payload["token"].as_str().unwrap().to_owned()
}

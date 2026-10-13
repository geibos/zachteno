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
        asn_db_path: None,
    }
}

/// Состояние на тестовой базе с управляемыми часами.
pub(crate) fn state(db: PgPool) -> AppState {
    state_with(db, config())
}

/// Состояние с изменённой конфигурацией.
pub(crate) fn state_with_yandex(db: PgPool, base_url: &str) -> AppState {
    let mut custom = config();
    custom.yandex = Some(crate::config::OAuthClient {
        client_id: "client".to_owned(),
        client_secret: Secret::new("secret".to_owned()),
        authorize_url: format!("{base_url}/authorize"),
        token_url: format!("{base_url}/token"),
        userinfo_url: format!("{base_url}/info"),
    });
    state_with(db, custom)
}

fn state_with(db: PgPool, config: Config) -> AppState {
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
        image_permits: Arc::new(tokio::sync::Semaphore::new(2)),
        asn: Arc::new(crate::antifraud::asn::AsnTable::default()),
        uploads: Arc::new(crate::http::limiter::Limiter::new(std::time::Duration::from_millis(1), 10_000)),
    }
}

/// Клиент к приложению: хранит cookie сессии и ставит свой `Origin`.
pub(crate) struct Client {
    app: Router,
    cookie: Option<String>,
    /// Прочие cookie (например, состояние OAuth).
    extra: Vec<(String, String)>,
}

impl Client {
    pub(crate) fn new(state: &AppState) -> Self {
        Self { app: crate::http::app(state.clone()), cookie: None, extra: Vec::new() }
    }

    pub(crate) fn anonymous(&self) -> Self {
        Self { app: self.app.clone(), cookie: None, extra: Vec::new() }
    }

    /// Копия клиента с той же cookie (как украденная сессия).
    pub(crate) fn anonymous_with_same_cookie(&self) -> Self {
        Self { app: self.app.clone(), cookie: self.cookie.clone(), extra: self.extra.clone() }
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
        if let Some(cookie) = self.cookie_header() {
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
        for set in headers.get_all(header::SET_COOKIE).iter().filter_map(|cookie| cookie.to_str().ok()) {
            let pair = set.split(';').next().unwrap_or_default().to_owned();
            let (name, value) = pair.split_once('=').unwrap_or((pair.as_str(), ""));
            if name == crate::auth::session::COOKIE_NAME {
                self.cookie = if value.is_empty() { None } else { Some(pair.clone()) };
            } else {
                self.extra.retain(|(known, _)| known != name);
                if !value.is_empty() {
                    self.extra.push((name.to_owned(), value.to_owned()));
                }
            }
        }
    }

    /// Все cookie для заголовка запроса.
    fn cookie_header(&self) -> Option<String> {
        let parts: Vec<String> =
            self.cookie.iter().cloned().chain(self.extra.iter().map(|(name, value)| format!("{name}={value}"))).collect();
        (!parts.is_empty()).then(|| parts.join("; "))
    }

    /// Выполняет запрос и возвращает сырой ответ (для редиректов).
    pub(crate) async fn raw(&mut self, method: Method, path: &str) -> axum::response::Response {
        let mut builder = Request::builder().method(method).uri(path).header(header::ORIGIN, ORIGIN);
        if let Some(cookie) = self.cookie_header() {
            builder = builder.header(header::COOKIE, cookie);
        }
        let response = self.app.clone().oneshot(builder.body(Body::empty()).unwrap()).await.unwrap();
        self.remember_cookie(response.headers());
        response
    }

    /// GET с телом в байтах (для выгрузок).
    pub(crate) async fn get_bytes(&mut self, path: &str) -> (StatusCode, Vec<u8>) {
        let mut builder = Request::builder().method(Method::GET).uri(path);
        if let Some(cookie) = &self.cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        let response = self.app.clone().oneshot(builder.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        (status, response.into_body().collect().await.unwrap().to_bytes().to_vec())
    }

    /// GET без разбора тела (для файлов).
    pub(crate) async fn get_raw(&mut self, path: &str) -> StatusCode {
        let mut builder = Request::builder().method(Method::GET).uri(path);
        if let Some(cookie) = &self.cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        self.app.clone().oneshot(builder.body(Body::empty()).unwrap()).await.unwrap().status()
    }

    /// Отправка одного файла multipart.
    pub(crate) async fn multipart(&mut self, path: &str, field: &str, filename: &str, bytes: Vec<u8>) -> (StatusCode, Json) {
        let boundary = "zachteno-test-boundary";
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(&bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::ORIGIN, ORIGIN)
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"));
        if let Some(cookie) = &self.cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        let response = self.app.clone().oneshot(builder.body(Body::from(body)).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Json::Null))
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

/// Готовый стенд: методист с курсом и опубликованным тестом, поток с открытым окном и студент в нём.
pub(crate) struct Classroom {
    pub methodist: Client,
    pub student: Client,
    pub student_id: Uuid,
    pub course_id: String,
    pub test_id: String,
    pub stream_id: String,
    pub invite_token: String,
}

/// Настройки теста поверх значений по умолчанию.
pub(crate) fn settings(overrides: &Json) -> Json {
    let mut base = json!({
        "attempts_limit": 1, "time_limit_minutes": null, "questions_per_attempt": null,
        "free_navigation": true, "shuffle_questions": false, "shuffle_options": false,
        "review_policy": "after_submit", "review_not_before": null, "final_policy": "best"
    });
    if let (Some(target), Some(extra)) = (base.as_object_mut(), overrides.as_object()) {
        for (key, field) in extra {
            let _previous = target.insert(key.clone(), field.clone());
        }
    }
    base
}

/// Вопрос с одиночным выбором: верен вариант `correct` из трёх.
pub(crate) fn single_question(correct: usize) -> Json {
    let options: Vec<Json> =
        (0..3).map(|index| json!({ "body": format!("вариант {index}"), "is_correct": index == correct })).collect();
    json!({ "kind": "single", "body": "Вопрос", "options": options })
}

impl Classroom {
    /// Стенд с `questions` вопросами (верен вариант 0) и настройками `overrides`.
    pub(crate) async fn new(state: &AppState, questions: usize, overrides: Json) -> Self {
        let (mut methodist, methodist_id) = register(state, "metod@example.com").await;
        grant(state, methodist_id, "methodist").await;
        let course_id = id_of(methodist.post("/api/courses", json!({ "title": "ПДД" })).await);
        let test_id = id_of(methodist.post(&format!("/api/courses/{course_id}/tests"), json!({ "title": "Экзамен" })).await);
        for _ in 0..questions {
            let _added = methodist.post(&format!("/api/tests/{test_id}/questions"), single_question(0)).await;
        }
        let (status, _) = methodist.patch(&format!("/api/tests/{test_id}"), json!({ "settings": settings(&overrides) })).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "настройки");
        let (status, body) = methodist.post(&format!("/api/tests/{test_id}/publish"), json!({})).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "публикация: {body}");
        let stream_id = id_of(methodist.post("/api/streams", json!({ "name": "ИВТ-21" })).await);
        let (status, _) = methodist.post(&format!("/api/streams/{stream_id}/courses"), json!({ "course_id": course_id })).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "поток на курс");
        let (_, invite) = methodist.post(&format!("/api/streams/{stream_id}/invites"), json!({ "label": "общая" })).await;
        let invite_token = invite["url"].as_str().unwrap().rsplit('/').next().unwrap().to_owned();
        let (mut student, student_id) = register(state, "student@example.com").await;
        let (status, _) = student.post(&format!("/api/invites/{invite_token}/join"), json!({})).await;
        assert_eq!(status, StatusCode::OK, "вступление");
        let mut classroom = Self { methodist, student, student_id, course_id, test_id, stream_id, invite_token };
        classroom.window(json!({ "action": "open" })).await;
        classroom
    }

    /// Меняет окно теста для потока.
    pub(crate) async fn window(&mut self, change: Json) {
        let path = format!("/api/tests/{}/windows/{}", self.test_id, self.stream_id);
        let (status, body) = self.methodist.put(&path, change).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "окно: {body}");
    }

    /// Студент начинает попытку и получает её вид.
    pub(crate) async fn start(&mut self) -> Json {
        let (status, started) = self.student.post(&format!("/api/tests/{}/attempts", self.test_id), json!({})).await;
        assert!(status.is_success(), "старт: {started}");
        let attempt_id = started["attempt_id"].as_str().unwrap().to_owned();
        self.student.get(&format!("/api/attempts/{attempt_id}")).await.1
    }
}

/// `id` из ответа `201 Created`.
pub(crate) fn id_of((status, body): (StatusCode, Json)) -> String {
    assert_eq!(status, StatusCode::CREATED, "создание: {body}");
    body["id"].as_str().unwrap().to_owned()
}

/// Ответ студента: первый вариант вопроса `index` попытки (`option` — какой по счёту в раскладке).
pub(crate) fn answer_body(attempt: &Json, index: usize, option: usize, seq: i64) -> (String, Json) {
    let question = &attempt["questions"][index];
    let path = format!("/api/attempts/{}/answers/{}", attempt["id"].as_str().unwrap(), question["id"].as_str().unwrap());
    let selected = question["options"][option]["id"].clone();
    (path, json!({ "selected_option_ids": [selected], "client_seq": seq }))
}

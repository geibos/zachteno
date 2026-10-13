//! Приём сигналов браузера и устройства (spec antifraud-signals, «Сбор сигналов»).
//!
//! Сырые значения не хранятся: каждый компонент — HMAC-SHA256 с ключом из конфигурации.

use std::collections::BTreeMap;
use std::net::IpAddr;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use hmac::{Hmac, KeyInit as _, Mac as _};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgConnection;
use sqlx::types::Json as SqlJson;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::antifraud::asn;
use crate::auth::session::CurrentUser;
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::jobs::{self, JobKind};
use crate::state::AppState;

/// Событие, при котором собраны сигналы.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SignalEvent {
    Register,
    Login,
    Invite,
    AttemptStart,
    Answer,
}

impl SignalEvent {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Login => "login",
            Self::Invite => "invite",
            Self::AttemptStart => "attempt_start",
            Self::Answer => "answer",
        }
    }
}

/// Ритм набора: интервалы удержания и между нажатиями, мс (без кодов клавиш).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub(crate) struct Typing {
    #[serde(default)]
    pub dwell: Vec<f32>,
    #[serde(default)]
    pub flight: Vec<f32>,
    /// Тип устройства ввода: `touch` или `keyboard`.
    #[serde(default)]
    pub input: String,
}

/// Сигналы от клиента.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct SignalInput {
    pub event: SignalEvent,
    /// Постоянный маркер браузера.
    pub marker: String,
    /// Признаки конкретного браузера.
    #[serde(default)]
    pub browser: BTreeMap<String, String>,
    /// Признаки устройства, не зависящие от браузера.
    #[serde(default)]
    pub device: BTreeMap<String, String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub typing: Option<Typing>,
}

const MAX_COMPONENTS: usize = 64;
const MAX_VALUE_CHARS: usize = 2048;
const MAX_SAMPLES: usize = 256;

impl SignalInput {
    fn validate(&self) -> ApiResult<()> {
        let too_big = |map: &BTreeMap<String, String>| {
            map.len() > MAX_COMPONENTS
                || map
                    .iter()
                    .any(|(key, raw)| key.len() > 64 || raw.chars().count() > MAX_VALUE_CHARS)
        };
        let typing_too_big = self.typing.as_ref().is_some_and(|typing| {
            typing.dwell.len() > MAX_SAMPLES || typing.flight.len() > MAX_SAMPLES
        });
        if self.marker.is_empty()
            || self.marker.len() > 128
            || too_big(&self.browser)
            || too_big(&self.device)
            || typing_too_big
        {
            return Err(AppError::BadRequest("Некорректные сигналы".to_owned()));
        }
        Ok(())
    }
}

/// HMAC значения с указанием компонента (чтобы одинаковые значения разных полей не совпадали).
pub(crate) fn keyed(key: &[u8], component: &str, raw: &str) -> Vec<u8> {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else { return Vec::new() };
    mac.update(component.as_bytes());
    mac.update(b"\0");
    mac.update(raw.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn combined(key: &[u8], name: &str, components: &BTreeMap<String, String>) -> Option<Vec<u8>> {
    (!components.is_empty()).then(|| {
        let joined: Vec<String> = components
            .iter()
            .map(|(field, raw)| format!("{field}={}", raw.trim().to_lowercase()))
            .collect();
        keyed(key, name, &joined.join("\n"))
    })
}

fn component_hashes(key: &[u8], components: &BTreeMap<String, String>) -> serde_json::Value {
    let map: serde_json::Map<String, serde_json::Value> = components
        .iter()
        .map(|(field, raw)| {
            (
                field.clone(),
                serde_json::Value::String(hex::encode(keyed(
                    key,
                    field,
                    &raw.trim().to_lowercase(),
                ))),
            )
        })
        .collect();
    serde_json::Value::Object(map)
}

/// Принимает сигналы текущего пользователя и ставит пересчёт его связей.
#[utoipa::path(post, path = "/api/signals", tag = "antifraud", operation_id = "submit_signals", request_body = SignalInput, responses((status = 204)))]
pub(crate) async fn submit(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Json(input): Json<SignalInput>,
) -> ApiResult<StatusCode> {
    input.validate()?;
    let mut tx = state.db.begin().await?;
    record(&state, &mut tx, user.id, (&input, ip)).await?;
    enqueue_recompute(&mut tx, user.id).await?;
    tx.commit().await?;
    state.jobs_wake.notify_waiters();
    Ok(StatusCode::NO_CONTENT)
}

/// Ставит пересчёт связей пользователя, если такой ещё не ждёт в очереди.
///
/// cancel-safe: yes — в транзакции вызывающего.
async fn enqueue_recompute(tx: &mut PgConnection, user_id: Uuid) -> ApiResult<()> {
    let pending: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM jobs WHERE kind = 'antifraud_recompute' AND done_at IS NULL AND payload->>'user_id' = $1::text)",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    if !pending {
        jobs::enqueue(
            &mut *tx,
            JobKind::AntifraudRecompute,
            &serde_json::json!({ "user_id": user_id }),
        )
        .await?;
    }
    Ok(())
}

/// Вычисленные значения строки `signals`: сведения о сети и HMAC компонентов.
struct SignalRow {
    asn: Option<i32>,
    country: Option<String>,
    hosting: bool,
    tz_mismatch: bool,
    marker_hash: Vec<u8>,
    browser_hash: Option<Vec<u8>>,
    device_hash: Option<Vec<u8>>,
    device_components: serde_json::Value,
}

impl SignalRow {
    fn new(state: &AppState, input: &SignalInput, ip: IpAddr) -> Self {
        let key = state.config.secrets.signal_key.expose();
        let network = state.asn.lookup(ip);
        Self {
            asn: network.as_ref().and_then(|found| i32::try_from(found.asn).ok()),
            country: network.as_ref().map(|found| found.country.clone()),
            hosting: network.as_ref().is_some_and(|found| found.hosting),
            tz_mismatch: network.as_ref().is_some_and(|found| {
                asn::timezone_mismatch(&found.country, input.timezone.as_deref())
            }),
            marker_hash: keyed(key, "marker", &input.marker),
            browser_hash: combined(key, "browser", &input.browser),
            device_hash: combined(key, "device", &input.device),
            device_components: component_hashes(key, &input.device),
        }
    }
}

const INSERT_SIGNAL: &str = "INSERT INTO signals (user_id, event, at, ip, asn, country, hosting, tz_mismatch, marker_hash, browser_hash,
                              device_hash, device_components, typing)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)";

/// cancel-safe: yes — в транзакции вызывающего.
async fn record(
    state: &AppState,
    tx: &mut PgConnection,
    user_id: Uuid,
    (input, ip): (&SignalInput, IpAddr),
) -> ApiResult<()> {
    let row = SignalRow::new(state, input, ip);
    let _inserted = sqlx::query(INSERT_SIGNAL)
        .bind(user_id)
        .bind(input.event.as_str())
        .bind(state.clock.now())
        .bind(ip)
        .bind(row.asn)
        .bind(row.country)
        .bind(row.hosting)
        .bind(row.tz_mismatch)
        .bind(row.marker_hash)
        .bind(row.browser_hash)
        .bind(row.device_hash)
        .bind(row.device_components)
        .bind(input.typing.as_ref().map(SqlJson))
        .execute(&mut *tx)
        .await?;
    Ok(())
}

//! Общее состояние обработчиков.

use std::net::IpAddr;
use std::num::NonZero;
use std::path::Path;
use std::sync::Arc;
use std::thread::available_parallelism;
use std::time::{Duration, Instant};

use sqlx::PgPool;
use tokio::sync::{Notify, Semaphore};

use crate::antifraud::asn::AsnTable;
use crate::auth::password::Hasher;
use crate::blobs::{self, Blobs};
use crate::clock::Clock;
use crate::config::Config;
use crate::error::AppError;
use crate::http::limiter::{Limiter, LoginFailures};

/// Состояние, доступное каждому обработчику.
#[derive(Clone)]
pub(crate) struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub hasher: Hasher,
    pub clock: Clock,
    /// Будит воркер очереди задач после постановки новой.
    pub jobs_wake: Arc<Notify>,
    /// Ограничения частоты по IP: вход, регистрация, письма.
    pub limits: Arc<Limits>,
    /// Неудачные входы по email.
    pub login_failures: Arc<LoginFailures>,
    pub http_client: reqwest::Client,
    /// Хранилище картинок.
    pub blobs: Blobs,
    /// Загрузки картинок на пользователя: 60 подряд, дальше одна в 2 с.
    pub uploads: Arc<Limiter<uuid::Uuid>>,
    /// Таблица IP → ASN.
    pub asn: Arc<AsnTable>,
    /// Не больше двух картинок обрабатываются одновременно.
    pub image_permits: Arc<Semaphore>,
}

impl AppState {
    /// Собирает состояние из конфигурации и пула.
    pub(crate) fn new(config: Config, db: PgPool) -> anyhow::Result<Self> {
        let concurrency = available_parallelism().map_or(4, NonZero::get);
        let blobs = blobs::connect(&config.storage)?;
        let asn = load_asn(&config)?;
        let http_client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?;
        let hasher = Hasher::new(config.secrets.peppers.clone(), concurrency);
        Ok(Self {
            db,
            config: Arc::new(config),
            hasher,
            clock: Clock::System,
            jobs_wake: Arc::new(Notify::new()),
            limits: Arc::new(Limits::production()),
            login_failures: Arc::new(LoginFailures::default()),
            http_client,
            blobs,
            uploads: Arc::new(Limiter::new(Duration::from_secs(2), 60)),
            asn: Arc::new(asn),
            image_permits: Arc::new(Semaphore::new(2)),
        })
    }
}

/// Таблица IP → ASN из файла `ASN_DB_PATH`; без переменной или файла — пустая.
fn load_asn(config: &Config) -> anyhow::Result<AsnTable> {
    Ok(config
        .asn_db_path
        .as_deref()
        .map(|path| AsnTable::load_if_present(Path::new(path)))
        .transpose()?
        .unwrap_or_default())
}

/// Ограничения частоты по IP.
#[derive(Debug)]
pub(crate) struct Limits {
    /// Вход: 10 подряд, дальше раз в 6 с.
    pub login: Limiter,
    /// Регистрация: общежитие за одним NAT — 100 подряд, дальше раз в секунду.
    pub register: Limiter,
    /// Письма (восстановление, повторное подтверждение): 10 подряд, дальше раз в 30 с.
    pub mail: Limiter,
}

impl Limits {
    pub(crate) fn production() -> Self {
        Self {
            login: Limiter::new(Duration::from_secs(6), 10),
            register: Limiter::new(Duration::from_secs(1), 100),
            mail: Limiter::new(Duration::from_secs(30), 10),
        }
    }

    /// Для тестов: практически без ограничений.
    #[cfg(test)]
    pub(crate) fn relaxed() -> Self {
        let loose = || Limiter::new(Duration::from_millis(1), 10_000);
        Self { login: loose(), register: loose(), mail: loose() }
    }
}

/// Проверка лимита с ответом 429.
pub(crate) fn throttle(limiter: &Limiter, ip: IpAddr) -> Result<(), AppError> {
    limiter
        .check(&ip, Instant::now())
        .map_err(|wait| AppError::TooManyRequests { retry_after_secs: wait.as_secs().max(1) })
}

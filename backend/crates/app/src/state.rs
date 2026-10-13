//! Общее состояние обработчиков.

use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::Notify;

use crate::auth::password::Hasher;
use crate::clock::Clock;
use crate::config::Config;
use crate::http::limiter::Limiter;

/// Состояние, доступное каждому обработчику.
#[derive(Clone)]
pub(crate) struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub hasher: Hasher,
    pub clock: Clock,
    /// Будит воркер очереди задач после постановки новой.
    pub jobs_wake: Arc<Notify>,
    /// Ограничение частоты входа по IP.
    pub login_limiter: Arc<Limiter>,
    pub http_client: reqwest::Client,
}

impl AppState {
    /// Собирает состояние из конфигурации и пула.
    pub(crate) fn new(config: Config, db: PgPool) -> anyhow::Result<Self> {
        let concurrency = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
        Ok(Self {
            db,
            hasher: Hasher::new(config.secrets.peppers.clone(), concurrency),
            config: Arc::new(config),
            clock: Clock::System,
            jobs_wake: Arc::new(Notify::new()),
            login_limiter: Arc::new(Limiter::new(std::time::Duration::from_secs(6), 10)),
            http_client: reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build()?,
        })
    }
}

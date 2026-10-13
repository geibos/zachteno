//! Конфигурация из переменных окружения (design D4, D6, D13, D16).

use std::collections::BTreeMap;
use std::env;
use std::net::SocketAddr;

use anyhow::{Context as _, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ipnet::IpNet;

use crate::secret::Secret;

/// Перцы паролей по номеру версии.
pub(crate) type PepperMap = BTreeMap<u32, Vec<u8>>;
type Pepper = (u32, Vec<u8>);

/// Полная конфигурация процесса.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub http: HttpConfig,
    pub database: DatabaseConfig,
    pub secrets: Secrets,
    pub storage: StorageConfig,
    pub mail: MailConfig,
    pub yandex: Option<OAuthClient>,
    pub signal_retention_days: u32,
    /// Файл iptoasn `ip2asn-v4.tsv` (необязательно).
    pub asn_db_path: Option<String>,
}

/// Адреса HTTP и доверенные прокси.
#[derive(Debug, Clone)]
pub(crate) struct HttpConfig {
    pub bind: SocketAddr,
    pub metrics_bind: SocketAddr,
    pub public_url: String,
    /// Дополнительные разрешённые `Origin` (dev-сервер фронтенда).
    pub extra_origins: Vec<String>,
    pub trusted_proxies: Vec<IpNet>,
}

/// Подключения к базе: роль приложения и роль-владелец для миграций.
#[derive(Debug, Clone)]
pub(crate) struct DatabaseConfig {
    pub app: Secret<String>,
    pub migrations: Secret<String>,
}

/// Серверные секреты, которых нет в базе.
#[derive(Debug, Clone)]
pub(crate) struct Secrets {
    /// Перцы паролей по версиям; активен последний.
    pub peppers: Secret<PepperMap>,
    /// Ключ HMAC для сигналов антифрода.
    pub signal_key: Secret<Vec<u8>>,
}

/// S3-совместимое хранилище картинок.
#[derive(Debug, Clone)]
pub(crate) struct StorageConfig {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: Secret<String>,
}

/// Отправка писем.
#[derive(Debug, Clone)]
pub(crate) enum MailConfig {
    /// SMTP со STARTTLS (Postbox).
    Smtp { host: String, port: u16, username: String, password: Secret<String>, from: String },
    /// Письма складываются файлами в каталог (разработка).
    Directory { path: String, from: String },
}

/// OAuth-клиент внешнего провайдера.
#[derive(Debug, Clone)]
pub(crate) struct OAuthClient {
    pub client_id: String,
    pub client_secret: Secret<String>,
    pub authorize_url: String,
    pub token_url: String,
    pub userinfo_url: String,
}

impl Config {
    /// Читает конфигурацию; без перца и ключа сигналов запуск невозможен.
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            http: http_from_env()?,
            database: DatabaseConfig {
                app: Secret::new(required("DATABASE_URL")?),
                migrations: Secret::new(required("MIGRATION_DATABASE_URL")?),
            },
            secrets: secrets_from_env()?,
            storage: storage_from_env()?,
            mail: mail_from_env()?,
            yandex: yandex_from_env(),
            signal_retention_days: optional("SIGNAL_RETENTION_DAYS", "365").parse()?,
            asn_db_path: env::var("ASN_DB_PATH").ok().filter(|path| !path.is_empty()),
        })
    }
}

fn required(name: &str) -> anyhow::Result<String> {
    env::var(name).with_context(|| format!("переменная окружения {name} не задана"))
}

fn optional(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn http_from_env() -> anyhow::Result<HttpConfig> {
    let proxies = optional("TRUSTED_PROXIES", "");
    Ok(HttpConfig {
        bind: optional("BIND", "0.0.0.0:8080").parse()?,
        metrics_bind: optional("METRICS_BIND", "127.0.0.1:9100").parse()?,
        public_url: optional("PUBLIC_URL", "http://localhost:8080")
            .trim_end_matches('/')
            .to_owned(),
        extra_origins: optional("EXTRA_ORIGINS", "")
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(str::to_owned)
            .collect(),
        trusted_proxies: parse_proxies(&proxies)?,
    })
}

fn parse_proxies(list: &str) -> anyhow::Result<Vec<IpNet>> {
    list.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().with_context(|| format!("TRUSTED_PROXIES: {part}")))
        .collect()
}

fn secrets_from_env() -> anyhow::Result<Secrets> {
    let peppers = parse_peppers(&required("PASSWORD_PEPPERS")?)?;
    let signal_key = STANDARD.decode(required("SIGNAL_HMAC_KEY")?).context("SIGNAL_HMAC_KEY")?;
    if signal_key.len() < 32 {
        bail!("SIGNAL_HMAC_KEY короче 32 байт");
    }
    Ok(Secrets { peppers: Secret::new(peppers), signal_key: Secret::new(signal_key) })
}

/// Формат `PASSWORD_PEPPERS`: `1:<base64>,2:<base64>`, активен наибольший номер.
pub(crate) fn parse_peppers(list: &str) -> anyhow::Result<PepperMap> {
    let peppers = list
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(parse_pepper)
        .collect::<anyhow::Result<PepperMap>>()?;
    if peppers.is_empty() {
        bail!("PASSWORD_PEPPERS пуст");
    }
    Ok(peppers)
}

fn parse_pepper(part: &str) -> anyhow::Result<Pepper> {
    let (version, encoded) =
        part.split_once(':').context("PASSWORD_PEPPERS: нужен формат N:base64")?;
    let secret = STANDARD.decode(encoded).context("PASSWORD_PEPPERS: base64")?;
    if secret.len() < 32 {
        bail!("перец версии {version} короче 32 байт");
    }
    Ok((version.parse()?, secret))
}

fn storage_from_env() -> anyhow::Result<StorageConfig> {
    Ok(StorageConfig {
        endpoint: required("S3_ENDPOINT")?,
        region: optional("S3_REGION", "garage"),
        bucket: optional("S3_BUCKET", "attachments"),
        access_key_id: required("S3_ACCESS_KEY_ID")?,
        secret_access_key: Secret::new(required("S3_SECRET_ACCESS_KEY")?),
    })
}

fn mail_from_env() -> anyhow::Result<MailConfig> {
    let from = optional("MAIL_FROM", "Зачтено <no-reply@localhost>");
    match optional("MAIL_MODE", "directory").as_str() {
        "smtp" => Ok(MailConfig::Smtp {
            host: required("SMTP_HOST")?,
            port: optional("SMTP_PORT", "587").parse()?,
            username: required("SMTP_USERNAME")?,
            password: Secret::new(required("SMTP_PASSWORD")?),
            from,
        }),
        "directory" => {
            Ok(MailConfig::Directory { path: optional("MAIL_DIR", "./data/mail"), from })
        }
        other => bail!("MAIL_MODE={other}: допустимо smtp или directory"),
    }
}

fn yandex_from_env() -> Option<OAuthClient> {
    let client_id = env::var("YANDEX_CLIENT_ID").ok().filter(|client| !client.is_empty())?;
    let client_secret =
        env::var("YANDEX_CLIENT_SECRET").ok().filter(|secret| !secret.is_empty())?;
    Some(OAuthClient {
        client_id,
        client_secret: Secret::new(client_secret),
        authorize_url: optional("YANDEX_AUTHORIZE_URL", "https://oauth.yandex.ru/authorize"),
        token_url: optional("YANDEX_TOKEN_URL", "https://oauth.yandex.ru/token"),
        userinfo_url: optional("YANDEX_USERINFO_URL", "https://login.yandex.ru/info"),
    })
}

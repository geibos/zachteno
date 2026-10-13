//! Конфигурация из переменных окружения (design D4, D6, D13, D16).

use std::collections::BTreeMap;
use std::env;
use std::net::SocketAddr;

use anyhow::{Context as _, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ipnet::IpNet;

use crate::secret::Secret;

#[cfg(test)]
mod tests;

/// Источник переменных окружения: имя → значение (процесс или словарь в тестах).
pub(crate) type EnvSource<'source> = &'source dyn Fn(&str) -> Option<String>;

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
    /// Роль-владелец (`MIGRATION_DATABASE_URL`). В production у сервера её нет: миграции
    /// применяет разовый `migrate`, а `serve` только проверяет, что они применены.
    pub migrations: Option<Secret<String>>,
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
    /// Читает конфигурацию из окружения процесса.
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        Self::from_vars(&|name| env::var(name).ok())
    }

    /// Читает конфигурацию из источника `source`; без перца и ключа сигналов запуск невозможен
    /// (spec identity-auth, «Нет перца»).
    pub(crate) fn from_vars(source: EnvSource<'_>) -> anyhow::Result<Self> {
        Ok(Self {
            http: http_from_env(source)?,
            database: DatabaseConfig {
                app: Secret::new(required(source, "DATABASE_URL")?),
                migrations: migration_url(source),
            },
            secrets: secrets_from_env(source)?,
            storage: storage_from_env(source)?,
            mail: mail_from_env(source)?,
            yandex: yandex_from_env(source),
            signal_retention_days: optional(source, "SIGNAL_RETENTION_DAYS", "365").parse()?,
            asn_db_path: source("ASN_DB_PATH").filter(|path| !path.is_empty()),
        })
    }
}

/// Строка подключения роли-владельца для команды `migrate`: остальная конфигурация ей не нужна,
/// поэтому разовому сервису миграций не передаются ни перцы, ни ключи хранилища.
pub(crate) fn migration_url_from_env() -> anyhow::Result<Secret<String>> {
    migration_url(&|name| env::var(name).ok())
        .context("переменная окружения MIGRATION_DATABASE_URL не задана")
}

/// `MIGRATION_DATABASE_URL`, если задана и не пуста.
fn migration_url(source: EnvSource<'_>) -> Option<Secret<String>> {
    source("MIGRATION_DATABASE_URL").filter(|url| !url.is_empty()).map(Secret::new)
}

fn required(source: EnvSource<'_>, name: &str) -> anyhow::Result<String> {
    source(name).with_context(|| format!("переменная окружения {name} не задана"))
}

fn optional(source: EnvSource<'_>, name: &str, default: &str) -> String {
    source(name).unwrap_or_else(|| default.to_owned())
}

fn http_from_env(source: EnvSource<'_>) -> anyhow::Result<HttpConfig> {
    let proxies = optional(source, "TRUSTED_PROXIES", "");
    Ok(HttpConfig {
        bind: optional(source, "BIND", "0.0.0.0:8080").parse()?,
        metrics_bind: optional(source, "METRICS_BIND", "127.0.0.1:9100").parse()?,
        public_url: optional(source, "PUBLIC_URL", "http://localhost:8080")
            .trim_end_matches('/')
            .to_owned(),
        extra_origins: optional(source, "EXTRA_ORIGINS", "")
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

fn secrets_from_env(source: EnvSource<'_>) -> anyhow::Result<Secrets> {
    let peppers = parse_peppers(&required(source, "PASSWORD_PEPPERS")?)?;
    let signal_key =
        STANDARD.decode(required(source, "SIGNAL_HMAC_KEY")?).context("SIGNAL_HMAC_KEY")?;
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

fn storage_from_env(source: EnvSource<'_>) -> anyhow::Result<StorageConfig> {
    Ok(StorageConfig {
        endpoint: required(source, "S3_ENDPOINT")?,
        region: optional(source, "S3_REGION", "garage"),
        bucket: optional(source, "S3_BUCKET", "attachments"),
        access_key_id: required(source, "S3_ACCESS_KEY_ID")?,
        secret_access_key: Secret::new(required(source, "S3_SECRET_ACCESS_KEY")?),
    })
}

fn mail_from_env(source: EnvSource<'_>) -> anyhow::Result<MailConfig> {
    let from = optional(source, "MAIL_FROM", "Зачтено <no-reply@localhost>");
    match optional(source, "MAIL_MODE", "directory").as_str() {
        "smtp" => Ok(MailConfig::Smtp {
            host: required(source, "SMTP_HOST")?,
            port: optional(source, "SMTP_PORT", "587").parse()?,
            username: required(source, "SMTP_USERNAME")?,
            password: Secret::new(required(source, "SMTP_PASSWORD")?),
            from,
        }),
        "directory" => {
            Ok(MailConfig::Directory { path: optional(source, "MAIL_DIR", "./data/mail"), from })
        }
        other => bail!("MAIL_MODE={other}: допустимо smtp или directory"),
    }
}

fn yandex_from_env(source: EnvSource<'_>) -> Option<OAuthClient> {
    let client_id = source("YANDEX_CLIENT_ID").filter(|client| !client.is_empty())?;
    let client_secret = source("YANDEX_CLIENT_SECRET").filter(|secret| !secret.is_empty())?;
    Some(OAuthClient {
        client_id,
        client_secret: Secret::new(client_secret),
        authorize_url: optional(
            source,
            "YANDEX_AUTHORIZE_URL",
            "https://oauth.yandex.ru/authorize",
        ),
        token_url: optional(source, "YANDEX_TOKEN_URL", "https://oauth.yandex.ru/token"),
        userinfo_url: optional(source, "YANDEX_USERINFO_URL", "https://login.yandex.ru/info"),
    })
}

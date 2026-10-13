use std::collections::BTreeMap;

use super::Config;

/// Минимальное рабочее окружение: 32-байтовые перец и ключ сигналов в base64.
fn environment() -> BTreeMap<&'static str, String> {
    let key = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".to_owned();
    BTreeMap::from([
        ("DATABASE_URL", "postgres://app@db/zachteno".to_owned()),
        ("MIGRATION_DATABASE_URL", "postgres://owner@db/zachteno".to_owned()),
        ("PASSWORD_PEPPERS", format!("1:{key}")),
        ("SIGNAL_HMAC_KEY", key),
        ("S3_ENDPOINT", "http://garage:3900".to_owned()),
        ("S3_ACCESS_KEY_ID", "key".to_owned()),
        ("S3_SECRET_ACCESS_KEY", "secret".to_owned()),
    ])
}

/// Загрузка конфигурации без переменной `missing`.
fn load_without(missing: &str) -> anyhow::Result<Config> {
    let mut known = environment();
    let _removed = known.remove(missing);
    Config::from_vars(&|name| known.get(name).cloned())
}

#[test]
fn full_environment_loads() {
    let known = environment();
    let config = Config::from_vars(&|name| known.get(name).cloned()).unwrap();
    assert_eq!(config.secrets.peppers.expose().len(), 1, "один перец");
    assert_eq!(config.secrets.signal_key.expose().len(), 32, "ключ сигналов");
}

#[test]
fn missing_pepper_is_configuration_error() {
    let error = load_without("PASSWORD_PEPPERS").unwrap_err().to_string();
    assert!(error.contains("PASSWORD_PEPPERS"), "ошибка называет перец: {error}");
}

#[test]
fn missing_signal_key_is_configuration_error() {
    let error = load_without("SIGNAL_HMAC_KEY").unwrap_err().to_string();
    assert!(error.contains("SIGNAL_HMAC_KEY"), "ошибка называет ключ сигналов: {error}");
}

#[test]
fn short_signal_key_is_rejected() {
    let mut known = environment();
    let _replaced = known.insert("SIGNAL_HMAC_KEY", "c2hvcnQ=".to_owned());
    let error = Config::from_vars(&|name| known.get(name).cloned()).unwrap_err().to_string();
    assert!(error.contains("короче 32 байт"), "короткий ключ: {error}");
}

#[test]
fn server_config_loads_without_owner_role() {
    let config = load_without("MIGRATION_DATABASE_URL").unwrap();
    assert!(
        config.database.migrations.is_none(),
        "без роли-владельца сервер только проверяет схему"
    );
}

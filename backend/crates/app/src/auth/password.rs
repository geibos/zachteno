//! Хеширование паролей: Argon2id, соль на пароль, серверный перец (spec identity-auth, D4).

use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher as _, PasswordVerifier as _};
use argon2::{Algorithm, Argon2, Params, Version};
use tokio::sync::Semaphore;

use crate::config::PepperMap;
use crate::secret::Secret;

#[cfg(test)]
mod tests;

/// Память Argon2 в КиБ (64 МиБ), итерации и параллелизм.
const MEMORY_KIB: u32 = 64 * 1024;
const ITERATIONS: u32 = 3;
const LANES: u32 = 1;

/// Минимальная и максимальная длина пароля в символах.
pub(crate) const MIN_LENGTH: usize = 12;
pub(crate) const MAX_LENGTH: usize = 128;

/// Встроенный список распространённых паролей (по одному в строке, в нижнем регистре).
static COMMON_PASSWORDS: &str = include_str!("password/common.txt");

/// Почему пароль не подходит.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum WeakPassword {
    /// Слишком короткий.
    #[error("пароль должен быть не короче 12 символов")]
    TooShort,
    /// Слишком длинный.
    #[error("пароль должен быть не длиннее 128 символов")]
    TooLong,
    /// Есть в списке распространённых.
    #[error("этот пароль слишком распространён — придумайте другой")]
    Common,
}

/// Проверяет пароль на длину и распространённость. Состав символов не проверяется.
pub(crate) fn check_strength(password: &str) -> Result<(), WeakPassword> {
    let length = password.chars().count();
    if length < MIN_LENGTH {
        return Err(WeakPassword::TooShort);
    }
    if length > MAX_LENGTH {
        return Err(WeakPassword::TooLong);
    }
    let lowered = password.to_lowercase();
    if COMMON_PASSWORDS.lines().any(|common| common == lowered) {
        return Err(WeakPassword::Common);
    }
    Ok(())
}

/// Результат проверки пароля.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verification {
    /// Пароль верный; хеш свежий.
    Valid,
    /// Пароль верный, но хеш надо пересчитать (старые параметры или перец).
    ValidNeedsRehash,
    /// Пароль неверный.
    Invalid,
}

/// Новый хеш вместе с версией перца.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredHash {
    pub phc: String,
    pub pepper_version: u32,
}

/// Хешер паролей с ограничением одновременных вычислений.
#[derive(Clone)]
pub(crate) struct Hasher {
    peppers: Arc<Secret<PepperMap>>,
    permits: Arc<Semaphore>,
}

impl Hasher {
    /// `concurrency` — сколько хешей считается одновременно (по числу ядер).
    pub(crate) fn new(peppers: Secret<PepperMap>, concurrency: usize) -> Self {
        Self { peppers: Arc::new(peppers), permits: Arc::new(Semaphore::new(concurrency.max(1))) }
    }

    /// Хеширует пароль активным (последним) перцем.
    ///
    /// cancel-safe: yes — вычисление в `spawn_blocking` не меняет состояния.
    pub(crate) async fn hash(&self, password: String) -> anyhow::Result<StoredHash> {
        let _permit = self.permits.acquire().await.context("семафор хешера закрыт")?;
        let peppers = Arc::clone(&self.peppers);
        tokio::task::spawn_blocking(move || hash_blocking(peppers.expose(), &password)).await?
    }

    /// Проверяет пароль против сохранённого хеша.
    ///
    /// cancel-safe: yes — только вычисление.
    pub(crate) async fn verify(&self, password: String, stored: StoredHash) -> anyhow::Result<Verification> {
        let _permit = self.permits.acquire().await.context("семафор хешера закрыт")?;
        let peppers = Arc::clone(&self.peppers);
        tokio::task::spawn_blocking(move || verify_blocking(peppers.expose(), &password, &stored)).await?
    }
}

fn argon2(pepper: &[u8]) -> anyhow::Result<Argon2<'_>> {
    let params = Params::new(MEMORY_KIB, ITERATIONS, LANES, None).map_err(|error| anyhow!("{error}"))?;
    Argon2::new_with_secret(pepper, Algorithm::Argon2id, Version::V0x13, params)
        .map_err(|error| anyhow!("{error}"))
}

fn hash_blocking(peppers: &PepperMap, password: &str) -> anyhow::Result<StoredHash> {
    let (version, pepper) = peppers.last_key_value().context("нет ни одного перца")?;
    let hash = argon2(pepper)?.hash_password(password.as_bytes()).map_err(|error| anyhow!("{error}"))?;
    Ok(StoredHash { phc: hash.to_string(), pepper_version: *version })
}

fn verify_blocking(peppers: &PepperMap, password: &str, stored: &StoredHash) -> anyhow::Result<Verification> {
    let Some(pepper) = peppers.get(&stored.pepper_version) else {
        return Ok(Verification::Invalid);
    };
    if argon2(pepper)?.verify_password(password.as_bytes(), stored.phc.as_str()).is_err() {
        return Ok(Verification::Invalid);
    }
    let latest = peppers.last_key_value().map(|(version, _)| *version);
    let fresh = latest == Some(stored.pepper_version) && has_current_params(&stored.phc);
    Ok(if fresh { Verification::Valid } else { Verification::ValidNeedsRehash })
}

fn has_current_params(phc: &str) -> bool {
    let expected = format!("m={MEMORY_KIB},t={ITERATIONS},p={LANES}");
    PasswordHash::new(phc).is_ok_and(|parsed| parsed.params.to_string() == expected)
}

//! Одноразовые токены ссылок и сессий: 256 бит случайности, в базе только SHA-256.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng as _;
use sha2::{Digest as _, Sha256};

/// Выпущенный токен: сырое значение уходит пользователю, хеш — в базу.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IssuedToken {
    pub raw: String,
    pub hash: Vec<u8>,
}

/// Выпускает новый токен из системного CSPRNG.
pub(crate) fn issue() -> IssuedToken {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let raw = URL_SAFE_NO_PAD.encode(bytes);
    let hash = digest(&raw);
    IssuedToken { raw, hash }
}

/// SHA-256 сырого токена — ключ поиска в базе.
pub(crate) fn digest(raw: &str) -> Vec<u8> {
    Sha256::digest(raw.as_bytes()).to_vec()
}

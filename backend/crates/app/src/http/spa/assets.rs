//! Собранный фронтенд, встроенный в бинарь.
#![expect(
    clippy::same_name_method,
    reason = "derive(Embed) сам генерирует одноимённые inherent- и trait-методы `get`/`iter`"
)]

use rust_embed::Embed;

/// Файлы `frontend/build`.
#[derive(Embed)]
#[folder = "../../../frontend/build"]
#[allow_missing = true]
pub(super) struct Assets;

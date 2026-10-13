//! Обёртка для секретов: не печатается ни в `Debug`, ни в логах.

use std::fmt;

/// Значение, которое нельзя случайно вывести в лог.
#[derive(Clone, Default)]
pub(crate) struct Secret<T>(T);

impl<T> Secret<T> {
    /// Оборачивает секрет.
    pub(crate) const fn new(secret: T) -> Self {
        Self(secret)
    }

    /// Доступ к значению; вызывать только там, где секрет действительно нужен.
    pub(crate) const fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

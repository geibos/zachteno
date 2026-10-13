//! Уровни связей между аккаунтами (spec `antifraud-signals`, design D14).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// Уровень уверенности в связи двух аккаунтов.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkLevel {
    /// Низкий.
    Low,
    /// Средний.
    Medium,
    /// Высокий.
    High,
}

/// Вид совпадения, на котором основана связь.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// Общий маркер браузера.
    SharedMarker,
    /// Общий редкий отпечаток устройства.
    SharedRareDevice,
    /// Общий распространённый отпечаток устройства.
    SharedCommonDevice,
    /// Общая домашняя сеть в разные дни.
    HomeNetwork,
    /// Один вышел — другой вошёл из той же сети или с того же устройства.
    SessionHandoff,
    /// Идентичный свободный ответ.
    IdenticalFreeText,
    /// Совпадающие наборы ошибок в нескольких тестах.
    MatchingMistakes,
    /// Похожая динамика набора текста.
    TypingSimilarity,
    /// Похожие регистрации по одной ссылке.
    RegistrationPattern,
    /// IP-адрес или подсеть общего пользования.
    SharedPublicNetwork,
}

/// Таблица уровней для отдельных видов совпадений.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelTable {
    /// Общий маркер браузера.
    pub shared_marker: LinkLevel,
    /// Редкий отпечаток устройства.
    pub rare_device: LinkLevel,
    /// Общая домашняя сеть.
    pub home_network: LinkLevel,
    /// Передача сессии.
    pub session_handoff: LinkLevel,
    /// Совпадение ответов (свободных и ошибок).
    pub answers: LinkLevel,
    /// Динамика набора.
    pub typing: LinkLevel,
}

impl Default for LevelTable {
    fn default() -> Self {
        Self {
            shared_marker: LinkLevel::High,
            rare_device: LinkLevel::Medium,
            home_network: LinkLevel::Medium,
            session_handoff: LinkLevel::Medium,
            answers: LinkLevel::Medium,
            typing: LinkLevel::Low,
        }
    }
}

impl LevelTable {
    /// Уровень одного вида совпадения.
    ///
    /// Сигналы общего пользования и распространённые отпечатки всегда низкие.
    #[must_use]
    pub const fn level_of(self, kind: EvidenceKind) -> LinkLevel {
        match kind {
            EvidenceKind::SharedMarker => self.shared_marker,
            EvidenceKind::SharedRareDevice => self.rare_device,
            EvidenceKind::HomeNetwork => self.home_network,
            EvidenceKind::SessionHandoff => self.session_handoff,
            EvidenceKind::IdenticalFreeText | EvidenceKind::MatchingMistakes => self.answers,
            EvidenceKind::TypingSimilarity => self.typing,
            EvidenceKind::SharedCommonDevice
            | EvidenceKind::RegistrationPattern
            | EvidenceKind::SharedPublicNetwork => LinkLevel::Low,
        }
    }

    /// Итоговый уровень связи по набору видов совпадений.
    ///
    /// Максимум отдельных уровней; два разных средних сигнала дают высокий.
    #[must_use]
    pub fn combine(self, kinds: &BTreeSet<EvidenceKind>) -> Option<LinkLevel> {
        let strongest = kinds.iter().map(|kind| self.level_of(*kind)).max()?;
        let medium_count =
            kinds.iter().filter(|kind| self.level_of(**kind) == LinkLevel::Medium).count();
        Some(if medium_count >= 2 { LinkLevel::High } else { strongest })
    }
}

/// Уровень транзитивной связи: уровень самого слабого ребра пути.
#[must_use]
pub fn transitive_level(path: &[LinkLevel]) -> Option<LinkLevel> {
    path.iter().copied().min()
}

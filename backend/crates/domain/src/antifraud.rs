//! Уровни связей между аккаунтами (spec `antifraud-signals`, design D14).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// Общий маркер браузера.
    SharedMarker,
    /// Общий редкий отпечаток устройства.
    SharedRareDevice,
    /// Общий распространённый отпечаток устройства.
    SharedCommonDevice,
    /// Транзитивная связь через общее устройство: оба аккаунта делили браузер или редкое
    /// устройство с третьим аккаунтом. Уровень — самого слабого ребра пути ([`transitive_level`]).
    TransitiveDevice,
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
    /// Сигналы общего пользования и распространённые отпечатки всегда низкие. Уровень
    /// транзитивной связи зависит от пути ([`transitive_level`]); без пути — низкий.
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
            | EvidenceKind::TransitiveDevice
            | EvidenceKind::RegistrationPattern
            | EvidenceKind::SharedPublicNetwork => LinkLevel::Low,
        }
    }

    /// Итоговый уровень связи по набору видов совпадений.
    ///
    /// Максимум отдельных уровней; два разных средних сигнала дают высокий.
    #[must_use]
    pub fn combine(self, kinds: &BTreeSet<EvidenceKind>) -> Option<LinkLevel> {
        combine_levels(&kinds.iter().map(|kind| (*kind, self.level_of(*kind))).collect())
    }
}

/// Итоговый уровень связи по уровням разных видов совпадений.
///
/// Максимум уровней; два разных вида среднего уровня дают высокий.
#[must_use]
pub fn combine_levels(levels: &BTreeMap<EvidenceKind, LinkLevel>) -> Option<LinkLevel> {
    let strongest = levels.values().copied().max()?;
    let medium_count = levels.values().filter(|level| **level == LinkLevel::Medium).count();
    Some(if medium_count >= 2 { LinkLevel::High } else { strongest })
}

/// Уровень транзитивной связи: уровень самого слабого ребра пути.
#[must_use]
pub fn transitive_level(path: &[LinkLevel]) -> Option<LinkLevel> {
    path.iter().copied().min()
}

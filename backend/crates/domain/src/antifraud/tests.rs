use std::collections::BTreeSet;

use super::{EvidenceKind, LevelTable, LinkLevel, transitive_level};

fn combine(kinds: &[EvidenceKind]) -> Option<LinkLevel> {
    LevelTable::default().combine(&kinds.iter().copied().collect::<BTreeSet<_>>())
}

#[test]
fn shared_public_ip_alone_is_low() {
    assert_eq!(combine(&[EvidenceKind::SharedPublicNetwork]), Some(LinkLevel::Low), "общий NAT");
}

#[test]
fn shared_marker_is_high() {
    assert_eq!(combine(&[EvidenceKind::SharedMarker]), Some(LinkLevel::High), "один браузер");
}

#[test]
fn common_device_fingerprint_is_low() {
    assert_eq!(
        combine(&[EvidenceKind::SharedCommonDevice]),
        Some(LinkLevel::Low),
        "компьютерный класс"
    );
}

#[test]
fn two_independent_medium_signals_become_high() {
    let kinds = [EvidenceKind::HomeNetwork, EvidenceKind::IdenticalFreeText];
    assert_eq!(combine(&kinds), Some(LinkLevel::High), "два средних");
}

#[test]
fn low_signals_never_upgrade_each_other() {
    let kinds = [
        EvidenceKind::SharedPublicNetwork,
        EvidenceKind::RegistrationPattern,
        EvidenceKind::TypingSimilarity,
    ];
    assert_eq!(combine(&kinds), Some(LinkLevel::Low), "низкие не складываются");
}

#[test]
fn no_evidence_means_no_link() {
    assert_eq!(combine(&[]), None, "нет совпадений");
}

#[test]
fn transitive_link_takes_weakest_edge() {
    assert_eq!(
        transitive_level(&[LinkLevel::High, LinkLevel::Medium]),
        Some(LinkLevel::Medium),
        "слабейшее ребро"
    );
}

use std::net::IpAddr;
use std::path::Path;

use super::{AsnTable, timezone_mismatch};

const SAMPLE: &str = "198.51.100.0\t198.51.100.255\t64500\tRU\tExample Home ISP\n\
                      192.0.2.0\t192.0.2.255\t64501\tRU\tExample Cloud Hosting\n\
                      203.0.113.0\t203.0.113.255\t0\tNone\tNot routed\n";

#[test]
fn lookup_finds_asn_and_hosting_flag() {
    let table = AsnTable::from_lines(SAMPLE);
    let home = table.lookup("198.51.100.42".parse::<IpAddr>().unwrap()).unwrap();
    assert_eq!(
        (home.asn, home.country.as_str(), home.hosting),
        (64500, "RU", false),
        "домашний провайдер"
    );
    let vps = table.lookup("192.0.2.10".parse::<IpAddr>().unwrap()).unwrap();
    assert!(vps.hosting, "хостинг");
    assert_eq!(table.lookup("203.0.113.5".parse::<IpAddr>().unwrap()), None, "не маршрутизируется");
    assert_eq!(table.lookup("8.8.8.8".parse::<IpAddr>().unwrap()), None, "нет в таблице");
}

#[test]
fn timezone_mismatch_detects_foreign_zone_for_russian_ip() {
    assert!(!timezone_mismatch("RU", Some("Europe/Moscow")), "Москва");
    assert!(timezone_mismatch("RU", Some("America/New_York")), "чужой пояс");
    assert!(timezone_mismatch("NL", Some("Europe/Moscow")), "российский пояс за VPN");
    assert!(!timezone_mismatch("RU", None), "пояс неизвестен");
}

#[test]
fn missing_database_file_gives_empty_table() {
    let table = AsnTable::load_if_present(Path::new("/nonexistent/ip2asn-v4.tsv")).unwrap();
    assert_eq!(table.lookup("198.51.100.42".parse::<IpAddr>().unwrap()), None, "пустая таблица");
}

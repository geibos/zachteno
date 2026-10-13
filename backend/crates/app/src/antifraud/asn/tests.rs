use std::net::IpAddr;

use super::{AsnTable, timezone_mismatch};

const SAMPLE: &str = "94.19.0.0\t94.19.255.255\t35807\tRU\tSkyNet Networks\n\
                      5.188.0.0\t5.188.255.255\t49505\tRU\tSelectel Hosting\n\
                      203.0.113.0\t203.0.113.255\t0\tNone\tNot routed\n";

#[test]
fn lookup_finds_asn_and_hosting_flag() {
    let table = AsnTable::from_lines(SAMPLE);
    let home = table.lookup("203.0.113.10".parse::<IpAddr>().unwrap()).unwrap();
    assert_eq!(
        (home.asn, home.country.as_str(), home.hosting),
        (35807, "RU", false),
        "домашний провайдер"
    );
    let vps = table.lookup("5.188.10.1".parse::<IpAddr>().unwrap()).unwrap();
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

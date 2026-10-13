use std::net::IpAddr;

use axum::http::{HeaderMap, HeaderValue};
use ipnet::IpNet;

use super::resolve_client_ip;

fn headers(forwarded: &str) -> HeaderMap {
    let mut map = HeaderMap::new();
    let _ = map.insert("x-forwarded-for", HeaderValue::from_str(forwarded).unwrap());
    map
}

fn trusted() -> Vec<IpNet> {
    vec!["172.30.90.0/24".parse().unwrap()]
}

#[test]
fn forwarded_header_is_used_from_trusted_proxy() {
    let peer: IpAddr = "172.30.90.5".parse().unwrap();
    let client = resolve_client_ip(peer, &headers("203.0.113.7"), &trusted());
    assert_eq!(client, "203.0.113.7".parse::<IpAddr>().unwrap(), "адрес из заголовка");
}

#[test]
fn forwarded_header_is_ignored_from_untrusted_peer() {
    let peer: IpAddr = "198.51.100.9".parse().unwrap();
    let client = resolve_client_ip(peer, &headers("203.0.113.7"), &trusted());
    assert_eq!(client, peer, "подставной заголовок игнорируется");
}

#[test]
fn spoofed_left_entries_are_skipped() {
    let peer: IpAddr = "172.30.90.5".parse().unwrap();
    let client = resolve_client_ip(peer, &headers("1.1.1.1, 203.0.113.7"), &trusted());
    assert_eq!(client, "203.0.113.7".parse::<IpAddr>().unwrap(), "берётся правый недоверенный");
}

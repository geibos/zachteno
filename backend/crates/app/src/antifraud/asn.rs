//! IP → ASN и страна по офлайн-базе iptoasn.com (PDDL), признак хостинга или VPN (design D14).

use std::fs::File;
use std::io::{BufRead as _, BufReader};
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;

#[cfg(test)]
mod tests;

/// Сведения о сети.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NetworkInfo {
    pub asn: u32,
    pub country: String,
    pub hosting: bool,
}

#[derive(Debug, Clone)]
struct Range {
    start: u32,
    end: u32,
    asn: u32,
    country: String,
    hosting: bool,
}

/// Таблица диапазонов IPv4, отсортированная по началу.
#[derive(Debug, Clone, Default)]
pub(crate) struct AsnTable {
    ranges: Vec<Range>,
}

/// Подстроки в описании AS, по которым сеть считается хостингом или VPN.
const HOSTING_HINTS: [&str; 24] = [
    "hosting",
    "cloud",
    "datacenter",
    "data center",
    "vps",
    "server",
    "colo",
    "amazon",
    "google",
    "microsoft",
    "digitalocean",
    "hetzner",
    "ovh",
    "linode",
    "vultr",
    "m247",
    "selectel",
    "timeweb",
    "aeza",
    "contabo",
    "leaseweb",
    "choopa",
    "vpn",
    "proxy",
];

impl AsnTable {
    /// Читает `ip2asn-v4.tsv`: `start\tend\tasn\tcountry\tdescription`.
    pub(crate) fn load(path: &Path) -> anyhow::Result<Self> {
        let file = File::open(path)?;
        let mut ranges: Vec<Range> = BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| parse_line(&line))
            .collect();
        ranges.sort_unstable_by_key(|range| range.start);
        Ok(Self { ranges })
    }

    /// Из строк (для тестов).
    #[cfg(test)]
    pub(crate) fn from_lines(text: &str) -> Self {
        let mut ranges: Vec<Range> = text.lines().filter_map(parse_line).collect();
        ranges.sort_unstable_by_key(|range| range.start);
        Self { ranges }
    }

    /// Сеть адреса; для IPv6 и неизвестных адресов — `None`.
    pub(crate) fn lookup(&self, ip: IpAddr) -> Option<NetworkInfo> {
        let IpAddr::V4(v4) = ip else { return None };
        let key = u32::from(v4);
        let index = self.ranges.partition_point(|range| range.start <= key).checked_sub(1)?;
        let range = self.ranges.get(index).filter(|range| key <= range.end && range.asn != 0)?;
        Some(NetworkInfo { asn: range.asn, country: range.country.clone(), hosting: range.hosting })
    }
}

fn parse_line(line: &str) -> Option<Range> {
    let mut fields = line.split('\t');
    let start: Ipv4Addr = fields.next()?.parse().ok()?;
    let end: Ipv4Addr = fields.next()?.parse().ok()?;
    let asn = fields.next()?.parse().ok()?;
    let country = fields.next()?.to_owned();
    let description = fields.next().unwrap_or_default().to_lowercase();
    let hosting = HOSTING_HINTS.iter().any(|hint| description.contains(hint));
    Some(Range { start: u32::from(start), end: u32::from(end), asn, country, hosting })
}

/// Часовые пояса России (для признака «часовой пояс не соответствует стране IP»).
const RUSSIAN_ZONES: [&str; 27] = [
    "Europe/Moscow",
    "Europe/Kaliningrad",
    "Europe/Samara",
    "Europe/Volgograd",
    "Europe/Kirov",
    "Europe/Astrakhan",
    "Europe/Saratov",
    "Europe/Ulyanovsk",
    "Asia/Yekaterinburg",
    "Asia/Omsk",
    "Asia/Novosibirsk",
    "Asia/Barnaul",
    "Asia/Tomsk",
    "Asia/Novokuznetsk",
    "Asia/Krasnoyarsk",
    "Asia/Irkutsk",
    "Asia/Chita",
    "Asia/Yakutsk",
    "Asia/Khandyga",
    "Asia/Vladivostok",
    "Asia/Ust-Nera",
    "Asia/Sakhalin",
    "Asia/Magadan",
    "Asia/Srednekolymsk",
    "Asia/Kamchatka",
    "Asia/Anadyr",
    "Europe/Simferopol",
];

/// Не соответствует ли часовой пояс устройства стране IP (проверяем только Россию).
pub(crate) fn timezone_mismatch(country: &str, timezone: Option<&str>) -> bool {
    let Some(zone) = timezone else { return false };
    let russian_zone = RUSSIAN_ZONES.contains(&zone);
    (country == "RU") != russian_zone
}

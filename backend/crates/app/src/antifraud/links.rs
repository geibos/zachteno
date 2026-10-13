//! Пересчёт связей пользователя по совпавшим сигналам (spec antifraud-signals, design D14).

use std::collections::{BTreeSet, HashMap};

use domain::antifraud::{EvidenceKind, LevelTable, LinkLevel};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::{Duration, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::state::AppState;

/// Основание связи: что совпало и когда (без сырых значений).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct Evidence {
    pub kind: EvidenceKind,
    pub detail: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub at: Option<OffsetDateTime>,
}

/// IP с числом разных пользователей за 30 дней не больше этого — «домашняя сеть».
const HOME_NETWORK_USERS: i64 = 3;
/// Больше этого — сеть общего пользования.
const PUBLIC_NETWORK_USERS: i64 = 5;
/// Отпечаток устройства у стольких пользователей и меньше — «редкий».
const RARE_DEVICE_USERS: i64 = 3;
/// Не больше стольких связей по сети общего пользования на пользователя.
const PUBLIC_LINK_CAP: i64 = 50;

#[derive(FromRow)]
struct Hit {
    other: Uuid,
    at: Option<OffsetDateTime>,
    extra: i64,
}

type Found = HashMap<Uuid, Vec<Evidence>>;

fn add(found: &mut Found, hits: Vec<Hit>, describe: impl Fn(&Hit) -> Option<(EvidenceKind, String)>) {
    for hit in hits {
        if let Some((kind, detail)) = describe(&hit) {
            found.entry(hit.other).or_default().push(Evidence { kind, detail, at: hit.at });
        }
    }
}

async fn hits(state: &AppState, sql: &'static str, user_id: Uuid, since: OffsetDateTime) -> ApiResult<Vec<Hit>> {
    Ok(sqlx::query_as(sql).bind(user_id).bind(since).fetch_all(&state.db).await?)
}

const SHARED_MARKER: &str = "
    SELECT b.user_id AS other, max(b.at) AS at, 0::bigint AS extra
    FROM signals a JOIN signals b ON b.marker_hash = a.marker_hash AND b.user_id <> a.user_id
    WHERE a.user_id = $1 AND a.at > $2 - interval '365 days' GROUP BY b.user_id";

const SHARED_DEVICE: &str = "
    SELECT b.user_id AS other, max(b.at) AS at,
           (SELECT count(DISTINCT c.user_id) FROM signals c WHERE c.device_hash = a.device_hash AND c.at > $2) AS extra
    FROM signals a JOIN signals b ON b.device_hash = a.device_hash AND b.user_id <> a.user_id
    WHERE a.user_id = $1 AND a.device_hash IS NOT NULL GROUP BY b.user_id, a.device_hash";

const SHARED_NETWORK: &str = "
    WITH mine AS (SELECT DISTINCT ip FROM signals WHERE user_id = $1 AND ip IS NOT NULL AND NOT hosting),
         crowd AS (SELECT s.ip, count(DISTINCT s.user_id) AS users FROM signals s JOIN mine USING (ip) WHERE s.at > $2 GROUP BY s.ip)
    SELECT s.user_id AS other, max(s.at) AS at,
           CASE WHEN max(c.users) <= 3 THEN count(DISTINCT date_trunc('day', s.at)) ELSE -max(c.users) END AS extra
    FROM signals s JOIN crowd c USING (ip)
    WHERE s.user_id <> $1 GROUP BY s.user_id, s.ip LIMIT 200";

const SESSION_HANDOFF: &str = "
    SELECT DISTINCT b.user_id AS other, a.at, 0::bigint AS extra
    FROM signals a JOIN signals b
      ON b.user_id <> a.user_id AND b.at BETWEEN a.at - interval '5 minutes' AND a.at
     AND (b.device_hash = a.device_hash OR (b.ip = a.ip AND
          (SELECT count(DISTINCT c.user_id) FROM signals c WHERE c.ip = a.ip AND c.at > $2) <= 5))
    WHERE a.user_id = $1 AND a.event IN ('login', 'register')";

const IDENTICAL_TEXT: &str = "
    SELECT b_at.user_id AS other, max(b.saved_at) AS at, count(*) AS extra
    FROM answers a JOIN attempts a_at ON a_at.id = a.attempt_id AND NOT a_at.is_preview
    JOIN answers b ON b.question_id = a.question_id AND b.attempt_id <> a.attempt_id
    JOIN attempts b_at ON b_at.id = b.attempt_id AND NOT b_at.is_preview AND b_at.user_id <> a_at.user_id
    WHERE a_at.user_id = $1 AND $2 IS NOT NULL AND length(trim(a.text_answer)) >= 20
      AND lower(regexp_replace(trim(a.text_answer), '\\s+', ' ', 'g')) = lower(regexp_replace(trim(b.text_answer), '\\s+', ' ', 'g'))
    GROUP BY b_at.user_id";

const MATCHING_MISTAKES: &str = "
    WITH pairs AS (
      SELECT b_at.user_id AS other, a_at.test_id, count(*) AS same_wrong, max(b.saved_at) AS at
      FROM answers a JOIN attempts a_at ON a_at.id = a.attempt_id AND NOT a_at.is_preview AND a_at.submitted_at IS NOT NULL
      JOIN answers b ON b.question_id = a.question_id AND b.attempt_id <> a.attempt_id
      JOIN attempts b_at ON b_at.id = b.attempt_id AND NOT b_at.is_preview AND b_at.submitted_at IS NOT NULL AND b_at.user_id <> a_at.user_id
      WHERE a_at.user_id = $1 AND $2 IS NOT NULL AND a.score <= 0 AND b.score <= 0
        AND cardinality(a.selected_option_ids) > 0
        AND (SELECT array_agg(x ORDER BY x) FROM unnest(a.selected_option_ids) x) = (SELECT array_agg(y ORDER BY y) FROM unnest(b.selected_option_ids) y)
      GROUP BY b_at.user_id, a_at.test_id)
    SELECT other, max(at) AS at, count(*) AS extra FROM pairs WHERE same_wrong >= 3 GROUP BY other";

const REGISTRATION_PATTERN: &str = "
    SELECT DISTINCT b.user_id AS other, b.used_at AS at, 0::bigint AS extra
    FROM invite_link_uses a JOIN invite_link_uses b
      ON b.link_id = a.link_id AND b.user_id <> a.user_id AND abs(extract(epoch FROM b.used_at - a.used_at)) <= 600 AND b.ip = a.ip
    WHERE a.user_id = $1 AND $2 IS NOT NULL";

fn describe_network(hit: &Hit) -> Option<(EvidenceKind, String)> {
    if hit.extra < 0 && -hit.extra > PUBLIC_NETWORK_USERS {
        return Some((EvidenceKind::SharedPublicNetwork, format!("общий IP-адрес, которым пользуются {} человек", -hit.extra)));
    }
    (hit.extra >= 2).then(|| (EvidenceKind::HomeNetwork, format!("общая домашняя сеть, {} разных дней", hit.extra)))
}

/// Пересчитывает связи пользователя и сохраняет их (решения проверяющих не трогаются).
///
/// cancel-safe: yes — каждая связь сохраняется отдельным идемпотентным UPSERT.
pub(crate) async fn recompute(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    let since = state.clock.now() - Duration::days(30);
    let mut found = Found::new();
    add(&mut found, hits(state, SHARED_MARKER, user_id, since).await?, |_| Some((EvidenceKind::SharedMarker, "один и тот же браузер (маркер)".to_owned())));
    add(&mut found, hits(state, SHARED_DEVICE, user_id, since).await?, |hit| {
        Some(if hit.extra <= RARE_DEVICE_USERS {
            (EvidenceKind::SharedRareDevice, "одно и то же устройство (отпечаток редкий)".to_owned())
        } else {
            (EvidenceKind::SharedCommonDevice, format!("одинаковый отпечаток устройства — распространённый, у {} пользователей", hit.extra))
        })
    });
    add(&mut found, hits(state, SHARED_NETWORK, user_id, since).await?, describe_network);
    add(&mut found, hits(state, SESSION_HANDOFF, user_id, since).await?, |_| Some((EvidenceKind::SessionHandoff, "передача сессии: вход сразу после другого аккаунта".to_owned())));
    add(&mut found, hits(state, IDENTICAL_TEXT, user_id, since).await?, |hit| Some((EvidenceKind::IdenticalFreeText, format!("идентичные свободные ответы: {}", hit.extra))));
    add(&mut found, hits(state, MATCHING_MISTAKES, user_id, since).await?, |hit| (hit.extra >= 2).then(|| (EvidenceKind::MatchingMistakes, format!("одинаковые ошибки в {} тестах", hit.extra))));
    add(&mut found, hits(state, REGISTRATION_PATTERN, user_id, since).await?, |_| Some((EvidenceKind::RegistrationPattern, "регистрация по одной ссылке из одной сети с разницей до 10 минут".to_owned())));
    add_typing(state, user_id, &mut found).await?;
    save(state, user_id, found).await
}

#[derive(FromRow)]
struct TypingRow {
    user_id: Uuid,
    typing: sqlx::types::Json<crate::antifraud::ingest::Typing>,
}

/// Признаки ритма: средние и разброс интервалов.
fn features(typing: &crate::antifraud::ingest::Typing) -> Option<[f64; 4]> {
    let stats = |samples: &[f32]| -> Option<(f64, f64)> {
        if samples.len() < 20 {
            return None;
        }
        let count = f64::from(u32::try_from(samples.len()).ok()?);
        let mean = samples.iter().map(|sample| f64::from(*sample)).sum::<f64>() / count;
        let variance = samples.iter().map(|sample| (f64::from(*sample) - mean).powi(2)).sum::<f64>() / count;
        Some((mean, variance.sqrt()))
    };
    let (dwell_mean, dwell_spread) = stats(&typing.dwell)?;
    let (flight_mean, flight_spread) = stats(&typing.flight)?;
    Some([dwell_mean, dwell_spread, flight_mean, flight_spread])
}

fn similar(left: &[f64; 4], right: &[f64; 4]) -> bool {
    left.iter().zip(right).all(|(one, two)| (one - two).abs() <= 0.07 * one.abs().max(two.abs()).max(1.0))
}

async fn add_typing(state: &AppState, user_id: Uuid, found: &mut Found) -> ApiResult<()> {
    let rows: Vec<TypingRow> = sqlx::query_as(
        "SELECT DISTINCT ON (user_id) user_id, typing FROM signals WHERE typing IS NOT NULL ORDER BY user_id, at DESC LIMIT 2000",
    )
    .fetch_all(&state.db)
    .await?;
    let Some(mine) = rows.iter().find(|row| row.user_id == user_id) else { return Ok(()) };
    let Some(my_features) = features(&mine.typing) else { return Ok(()) };
    for row in rows.iter().filter(|row| row.user_id != user_id && row.typing.input == mine.typing.input) {
        if features(&row.typing).is_some_and(|theirs| similar(&my_features, &theirs)) {
            found.entry(row.user_id).or_default().push(Evidence {
                kind: EvidenceKind::TypingSimilarity,
                detail: "похожий ритм набора текста".to_owned(),
                at: None,
            });
        }
    }
    Ok(())
}

async fn save(state: &AppState, user_id: Uuid, found: Found) -> ApiResult<()> {
    let table = LevelTable::default();
    let mut public_links = 0_i64;
    for (other, evidence) in found {
        let kinds: BTreeSet<EvidenceKind> = evidence.iter().map(|item| item.kind).collect();
        let Some(level) = table.combine(&kinds) else { continue };
        if kinds.iter().all(|kind| *kind == EvidenceKind::SharedPublicNetwork) {
            public_links = public_links.saturating_add(1);
            if public_links > PUBLIC_LINK_CAP {
                continue;
            }
        }
        upsert(state, (user_id, other), level, &evidence).await?;
    }
    Ok(())
}

pub(crate) const fn level_name(level: LinkLevel) -> &'static str {
    match level {
        LinkLevel::Low => "low",
        LinkLevel::Medium => "medium",
        LinkLevel::High => "high",
    }
}

async fn upsert(state: &AppState, (one, two): (Uuid, Uuid), level: LinkLevel, evidence: &[Evidence]) -> ApiResult<()> {
    let (user_a, user_b) = if one < two { (one, two) } else { (two, one) };
    let _saved = sqlx::query(
        "INSERT INTO identity_links (user_a, user_b, level, evidence, computed_at) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_a, user_b) DO UPDATE SET level = EXCLUDED.level, evidence = EXCLUDED.evidence, computed_at = EXCLUDED.computed_at",
    )
    .bind(user_a)
    .bind(user_b)
    .bind(level_name(level))
    .bind(sqlx::types::Json(evidence))
    .bind(state.clock.now())
    .execute(&state.db)
    .await?;
    Ok(())
}

//! Пересчёт связей пользователя по совпавшим сигналам (spec antifraud-signals, design D14).

use std::collections::{BTreeMap, BTreeSet};

use domain::antifraud::{EvidenceKind, LevelTable, LinkLevel, combine_levels, transitive_level};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use time::{Duration, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::antifraud::ingest::Typing;
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

/// Больше этого — сеть общего пользования.
const PUBLIC_NETWORK_USERS: i64 = 5;
/// Отпечаток устройства у стольких пользователей и меньше — «редкий».
const RARE_DEVICE_USERS: i64 = 3;
/// Не больше стольких связей по сети общего пользования на пользователя.
const PUBLIC_LINK_CAP: i64 = 50;
/// Не больше стольких аккаунтов-посредников при поиске транзитивных связей.
const BRIDGE_CAP: usize = 50;
/// Основания «общее устройство», из которых складываются пути графа устройств. Распространённый
/// отпечаток сюда не входит: он не означает одно и то же устройство (компьютерный класс).
const DEVICE_KINDS: [EvidenceKind; 2] =
    [EvidenceKind::SharedMarker, EvidenceKind::SharedRareDevice];

#[derive(FromRow)]
struct Hit {
    other: Uuid,
    at: Option<OffsetDateTime>,
    extra: i64,
    /// Дни совпадения по МСК (только у запроса общей сети).
    #[sqlx(default)]
    days: Option<String>,
}

/// Основания по каждому связанному пользователю; упорядочено по id, чтобы лимит связей
/// по сети общего пользования отсекал одни и те же связи при каждом пересчёте.
type Found = BTreeMap<Uuid, Vec<Evidence>>;

/// Вид и описание основания, если совпадение что-то значит.
type Described = Option<(EvidenceKind, String)>;

/// Толкование строки совпадения.
type Describe = fn(&Hit) -> Described;

fn add(found: &mut Found, hits: Vec<Hit>, describe: Describe) {
    for hit in hits {
        if let Some((kind, detail)) = describe(&hit) {
            found.entry(hit.other).or_default().push(Evidence { kind, detail, at: hit.at });
        }
    }
}

async fn hits(
    state: &AppState,
    sql: &'static str,
    user_id: Uuid,
    since: OffsetDateTime,
) -> ApiResult<Vec<Hit>> {
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

/// Аппаратное ядро отпечатка: признаки, которые не зависят от браузера. Совпадение ядра при
/// разных отпечатках — то же устройство в другом браузере (spec «Смена браузера на том же
/// устройстве»): у Chromium и Firefox различаются строка GPU, число найденных шрифтов и т. п.
const SIMILAR_DEVICE: &str = "
    WITH core AS (
      SELECT user_id, at, device_hash,
             concat_ws('|', device_components->>'screen', device_components->>'color_depth',
                       device_components->>'pixel_ratio', device_components->>'cores',
                       device_components->>'touch_points', device_components->>'timezone',
                       device_components->>'os') AS key
      FROM signals
      WHERE device_components ? 'screen' AND device_components ? 'cores' AND device_components ? 'os'),
    mine AS (SELECT DISTINCT key, device_hash FROM core WHERE user_id = $1)
    SELECT b.user_id AS other, max(b.at) AS at,
           (SELECT count(DISTINCT c.user_id) FROM core c WHERE c.key = m.key AND c.at > $2) AS extra
    FROM mine m JOIN core b ON b.key = m.key AND b.user_id <> $1 AND b.device_hash IS DISTINCT FROM m.device_hash
    GROUP BY b.user_id, m.key";

/// `max(c.users) <= 3`: IP, которым за 30 дней пользовались не больше трёх человек, —
/// «домашняя сеть».
const SHARED_NETWORK: &str = "
    WITH mine AS (SELECT DISTINCT ip FROM signals WHERE user_id = $1 AND ip IS NOT NULL AND NOT hosting),
         crowd AS (SELECT s.ip, count(DISTINCT s.user_id) AS users FROM signals s JOIN mine USING (ip) WHERE s.at > $2 GROUP BY s.ip)
    SELECT s.user_id AS other, max(s.at) AS at,
           CASE WHEN max(c.users) <= 3 THEN count(DISTINCT date_trunc('day', s.at)) ELSE -max(c.users) END AS extra,
           string_agg(DISTINCT to_char(s.at AT TIME ZONE 'Europe/Moscow', 'YYYY-MM-DD'), ', ') AS days
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

/// Ошибка — ответ с баллом ниже максимума вопроса. Максимум считается так же,
/// как `ScoredQuestion::max_score`: W × число верных в `multiple_each`, иначе W.
const MATCHING_MISTAKES: &str = "
    WITH pairs AS (
      SELECT b_at.user_id AS other, a_at.test_id, count(*) AS same_wrong, max(b.saved_at) AS at
      FROM answers a JOIN attempts a_at ON a_at.id = a.attempt_id AND NOT a_at.is_preview AND a_at.submitted_at IS NOT NULL
      JOIN questions q ON q.id = a.question_id
      CROSS JOIN LATERAL (SELECT CASE WHEN q.kind = 'multiple_each'
          THEN q.weight * (SELECT count(*) FROM options o WHERE o.question_id = q.id AND o.is_correct)
          ELSE q.weight END AS max_score) m
      JOIN answers b ON b.question_id = a.question_id AND b.attempt_id <> a.attempt_id
      JOIN attempts b_at ON b_at.id = b.attempt_id AND NOT b_at.is_preview AND b_at.submitted_at IS NOT NULL AND b_at.user_id <> a_at.user_id
      WHERE a_at.user_id = $1 AND $2 IS NOT NULL AND a.score < m.max_score AND b.score < m.max_score
        AND cardinality(a.selected_option_ids) > 0
        AND (SELECT array_agg(x ORDER BY x) FROM unnest(a.selected_option_ids) x) = (SELECT array_agg(y ORDER BY y) FROM unnest(b.selected_option_ids) y)
      GROUP BY b_at.user_id, a_at.test_id)
    SELECT other, max(at) AS at, count(*) AS extra FROM pairs WHERE same_wrong >= 3 GROUP BY other";

const REGISTRATION_PATTERN: &str = "
    SELECT DISTINCT b.user_id AS other, b.used_at AS at, 0::bigint AS extra
    FROM invite_link_uses a JOIN invite_link_uses b
      ON b.link_id = a.link_id AND b.user_id <> a.user_id AND abs(extract(epoch FROM b.used_at - a.used_at)) <= 600 AND b.ip = a.ip
    WHERE a.user_id = $1 AND $2 IS NOT NULL";

fn describe_network(hit: &Hit) -> Described {
    let users = hit.extra.saturating_neg();
    if hit.extra < 0 && users > PUBLIC_NETWORK_USERS {
        return Some((
            EvidenceKind::SharedPublicNetwork,
            format!("общий IP-адрес, которым пользуются {users} человек"),
        ));
    }
    (hit.extra >= 2).then(|| {
        let days = hit.days.as_deref().unwrap_or_default();
        (
            EvidenceKind::HomeNetwork,
            format!("общая домашняя сеть, {} разных дней: {days}", hit.extra),
        )
    })
}

/// Общий маркер браузера.
const DESCRIBE_MARKER: Describe =
    |_| Some((EvidenceKind::SharedMarker, "один и тот же браузер (маркер)".to_owned()));

/// Общий отпечаток устройства: редкий или распространённый.
const DESCRIBE_DEVICE: Describe = |hit| {
    Some(if hit.extra <= RARE_DEVICE_USERS {
        (EvidenceKind::SharedRareDevice, "одно и то же устройство (отпечаток редкий)".to_owned())
    } else {
        (
            EvidenceKind::SharedCommonDevice,
            format!(
                "одинаковый отпечаток устройства — распространённый, у {} пользователей",
                hit.extra
            ),
        )
    })
};

fn describe_mistakes(hit: &Hit) -> Described {
    (hit.extra >= 2).then(|| {
        (EvidenceKind::MatchingMistakes, format!("одинаковые ошибки в {} тестах", hit.extra))
    })
}

/// Совпадение аппаратного ядра при разных отпечатках.
const DESCRIBE_SAME_HARDWARE: Describe = |hit| {
    Some((
        EvidenceKind::SameHardware,
        format!(
            "похоже на тот же компьютер в другом браузере: совпали экран, процессор, ОС и часовой пояс \
             (такое сочетание у {} пользователей)",
            hit.extra
        ),
    ))
};

/// Добавляет совпадения аппаратного ядра тем, у кого ещё нет основания «общее устройство»:
/// это тот же факт, что и точный отпечаток, и вторым независимым сигналом он не считается.
fn add_similar_devices(found: &mut Found, hits: Vec<Hit>) {
    let has_device = |evidence: &[Evidence]| {
        evidence.iter().any(|piece| {
            matches!(piece.kind, EvidenceKind::SharedRareDevice | EvidenceKind::SharedCommonDevice)
        })
    };
    let fresh: Vec<Hit> = hits
        .into_iter()
        .filter(|hit| !found.get(&hit.other).is_some_and(|evidence| has_device(evidence)))
        .collect();
    add(found, fresh, DESCRIBE_SAME_HARDWARE);
}

/// Запрос совпадений и его толкование.
type Source = (&'static str, Describe);

/// Запросы рёбер графа устройств: общий браузер и общее устройство.
const DEVICE_SOURCES: [Source; 2] =
    [(SHARED_MARKER, DESCRIBE_MARKER), (SHARED_DEVICE, DESCRIBE_DEVICE)];

/// Запросы совпадений в порядке выполнения.
const SOURCES: [Source; 7] = [
    (SHARED_MARKER, DESCRIBE_MARKER),
    (SHARED_DEVICE, DESCRIBE_DEVICE),
    (SHARED_NETWORK, describe_network),
    (SESSION_HANDOFF, |_| {
        Some((
            EvidenceKind::SessionHandoff,
            "передача сессии: вход сразу после другого аккаунта".to_owned(),
        ))
    }),
    (IDENTICAL_TEXT, |hit| {
        Some((
            EvidenceKind::IdenticalFreeText,
            format!("идентичные свободные ответы: {}", hit.extra),
        ))
    }),
    (MATCHING_MISTAKES, describe_mistakes),
    (REGISTRATION_PATTERN, |_| {
        Some((
            EvidenceKind::RegistrationPattern,
            "регистрация по одной ссылке из одной сети с разницей до 10 минут".to_owned(),
        ))
    }),
];

/// Пересчитывает связи пользователя и сохраняет их (решения проверяющих не трогаются).
///
/// cancel-safe: yes — каждая связь сохраняется отдельным идемпотентным UPSERT.
pub(crate) async fn recompute(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    let since = state.clock.now().saturating_sub(Duration::days(30));
    let mut found = Found::new();
    for (sql, describe) in SOURCES {
        add(&mut found, hits(state, sql, user_id, since).await?, describe);
    }
    add_similar_devices(&mut found, hits(state, SIMILAR_DEVICE, user_id, since).await?);
    add_typing(state, user_id, &mut found).await?;
    let bridged = add_transitive(state, user_id, since, &mut found).await?;
    save(state, user_id, found, &bridged).await
}

/// Уровни транзитивных связей по связанному пользователю.
type Bridged = BTreeMap<Uuid, LinkLevel>;

/// Уровень ребра «общее устройство» до каждого соседа по графу устройств.
fn device_edges(found: &Found, table: LevelTable) -> Bridged {
    found
        .iter()
        .filter_map(|(other, evidence)| {
            evidence
                .iter()
                .filter(|piece| DEVICE_KINDS.contains(&piece.kind))
                .map(|piece| table.level_of(piece.kind))
                .max()
                .map(|level| (*other, level))
        })
        .collect()
}

/// Соседи пользователя по графу устройств: общий браузер или общее устройство.
///
/// cancel-safe: yes — только чтение.
async fn device_neighbours(
    state: &AppState,
    user_id: Uuid,
    since: OffsetDateTime,
) -> ApiResult<Found> {
    let mut found = Found::new();
    for (sql, describe) in DEVICE_SOURCES {
        add(&mut found, hits(state, sql, user_id, since).await?, describe);
    }
    Ok(found)
}

/// Транзитивные связи через общее устройство (spec «Граф устройств», design D14): аккаунт B
/// связан с пользователем, если оба делили браузер или редкое устройство с аккаунтом-посредником,
/// а прямого общего устройства у них нет. Уровень — самого слабого ребра пути, из нескольких
/// путей берётся лучший.
///
/// cancel-safe: yes — только чтение; найденное добавляется в `found`.
async fn add_transitive(
    state: &AppState,
    user_id: Uuid,
    since: OffsetDateTime,
    found: &mut Found,
) -> ApiResult<Bridged> {
    let table = LevelTable::default();
    let direct = device_edges(found, table);
    let mut bridged = Bridged::new();
    for (bridge, first_hop) in direct.iter().take(BRIDGE_CAP) {
        let second = device_edges(&device_neighbours(state, *bridge, since).await?, table);
        let beyond = second
            .into_iter()
            .filter(|(other, _)| *other != user_id && !direct.contains_key(other));
        extend_paths(&mut bridged, *first_hop, beyond);
    }
    for other in bridged.keys() {
        found.entry(*other).or_default().push(transitive_evidence());
    }
    Ok(bridged)
}

/// Добавляет пути «пользователь → посредник → другой»; у пути уровень слабейшего ребра.
fn extend_paths(
    bridged: &mut Bridged,
    first_hop: LinkLevel,
    second: impl Iterator<Item = (Uuid, LinkLevel)>,
) {
    for (other, second_hop) in second {
        if let Some(level) = transitive_level(&[first_hop, second_hop]) {
            let _best =
                bridged.entry(other).and_modify(|best| *best = (*best).max(level)).or_insert(level);
        }
    }
}

fn transitive_evidence() -> Evidence {
    Evidence {
        kind: EvidenceKind::TransitiveDevice,
        detail: "транзитивная связь: оба аккаунта пользовались общим браузером или устройством \
                 третьего аккаунта"
            .to_owned(),
        at: None,
    }
}

#[derive(FromRow)]
struct TypingRow {
    user_id: Uuid,
    typing: Json<Typing>,
}

/// Среднее и стандартное отклонение.
type MeanSpread = (f64, f64);

/// Признаки ритма: средние и разброс интервалов.
fn features(typing: &Typing) -> Option<[f64; 4]> {
    let stats = |samples: &[f32]| -> Option<MeanSpread> {
        if samples.len() < 20 {
            return None;
        }
        let count = f64::from(u32::try_from(samples.len()).ok()?);
        let mean = samples.iter().map(|sample| f64::from(*sample)).sum::<f64>() / count;
        let variance =
            samples.iter().map(|sample| (f64::from(*sample) - mean).powi(2)).sum::<f64>() / count;
        Some((mean, variance.sqrt()))
    };
    let (dwell_mean, dwell_spread) = stats(&typing.dwell)?;
    let (flight_mean, flight_spread) = stats(&typing.flight)?;
    Some([dwell_mean, dwell_spread, flight_mean, flight_spread])
}

fn similar(left: &[f64; 4], right: &[f64; 4]) -> bool {
    left.iter()
        .zip(right)
        .all(|(one, two)| (one - two).abs() <= 0.07 * one.abs().max(two.abs()).max(1.0))
}

/// cancel-safe: yes — только чтение.
async fn add_typing(state: &AppState, user_id: Uuid, found: &mut Found) -> ApiResult<()> {
    let rows: Vec<TypingRow> = sqlx::query_as(
        "SELECT DISTINCT ON (user_id) user_id, typing FROM signals WHERE typing IS NOT NULL ORDER BY user_id, at DESC LIMIT 2000",
    )
    .fetch_all(&state.db)
    .await?;
    push_typing(found, &rows, user_id);
    Ok(())
}

/// Добавляет основание «похожий ритм» для пользователей, чей набор похож на набор `user_id`.
fn push_typing(found: &mut Found, rows: &[TypingRow], user_id: Uuid) {
    let Some(mine) = rows.iter().find(|row| row.user_id == user_id) else { return };
    let Some(my_features) = features(&mine.typing) else { return };
    for row in
        rows.iter().filter(|row| row.user_id != user_id && row.typing.input == mine.typing.input)
    {
        if features(&row.typing).is_some_and(|theirs| similar(&my_features, &theirs)) {
            found.entry(row.user_id).or_default().push(Evidence {
                kind: EvidenceKind::TypingSimilarity,
                detail: "похожий ритм набора текста".to_owned(),
                at: None,
            });
        }
    }
}

/// Уровни видов совпадений с одним пользователем; транзитивный — по пути из `bridged`.
fn levels_of(
    table: LevelTable,
    kinds: &BTreeSet<EvidenceKind>,
    path: Option<LinkLevel>,
) -> BTreeMap<EvidenceKind, LinkLevel> {
    kinds
        .iter()
        .map(|kind| {
            let level = match (kind, path) {
                (EvidenceKind::TransitiveDevice, Some(path_level)) => path_level,
                _ => table.level_of(*kind),
            };
            (*kind, level)
        })
        .collect()
}

async fn save(state: &AppState, user_id: Uuid, found: Found, bridged: &Bridged) -> ApiResult<()> {
    let table = LevelTable::default();
    let mut public_links = 0_i64;
    for (other, evidence) in found {
        let kinds: BTreeSet<EvidenceKind> = evidence.iter().map(|piece| piece.kind).collect();
        let levels = levels_of(table, &kinds, bridged.get(&other).copied());
        let Some(level) = combine_levels(&levels) else { continue };
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

async fn upsert(
    state: &AppState,
    (one, two): (Uuid, Uuid),
    level: LinkLevel,
    evidence: &[Evidence],
) -> ApiResult<()> {
    let (user_a, user_b) = if one < two { (one, two) } else { (two, one) };
    let _saved = sqlx::query(
        "INSERT INTO identity_links (user_a, user_b, level, evidence, computed_at) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_a, user_b) DO UPDATE SET level = EXCLUDED.level, evidence = EXCLUDED.evidence, computed_at = EXCLUDED.computed_at",
    )
    .bind(user_a)
    .bind(user_b)
    .bind(level_name(level))
    .bind(Json(evidence))
    .bind(state.clock.now())
    .execute(&state.db)
    .await?;
    Ok(())
}

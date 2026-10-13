//! Связи для проверяющих: список, кластеры, решения (spec antifraud-signals, «Просмотр и решение по связям»).

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::{self, CourseRole};
use crate::antifraud::links::Evidence;
use crate::audit::AuditEntry;
use crate::auth::session::CurrentUser;
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::state::AppState;

/// Участник связи.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct LinkedUser {
    pub id: Uuid,
    pub display_name: String,
    pub email: String,
}

/// Связь двух аккаунтов.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct LinkView {
    pub user_a: LinkedUser,
    pub user_b: LinkedUser,
    pub level: String,
    pub evidence: Vec<Evidence>,
    pub decision: Option<String>,
    pub comment: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub decided_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub computed_at: OffsetDateTime,
}

/// Связи участников курса и кластеры.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CourseLinks {
    pub links: Vec<LinkView>,
    /// Группы аккаунтов, связанных на уровне от среднего (без отклонённых связей).
    pub clusters: Vec<Vec<Uuid>>,
}

#[derive(FromRow)]
struct LinkRow {
    user_a: Uuid,
    a_name: String,
    a_email: String,
    user_b: Uuid,
    b_name: String,
    b_email: String,
    level: String,
    evidence: sqlx::types::Json<Vec<Evidence>>,
    decision: Option<String>,
    comment: String,
    decided_at: Option<OffsetDateTime>,
    computed_at: OffsetDateTime,
}

const COURSE_LINKS: &str = "
    WITH participants AS (
        SELECT m.user_id FROM stream_courses sc JOIN stream_members m ON m.stream_id = sc.stream_id WHERE sc.course_id = $1
        UNION SELECT a.user_id FROM attempts a JOIN tests t ON t.id = a.test_id WHERE t.course_id = $1 AND NOT a.is_preview)
    SELECT l.user_a, ua.display_name AS a_name, ua.email AS a_email, l.user_b, ub.display_name AS b_name, ub.email AS b_email,
           l.level, l.evidence, l.decision, l.comment, l.decided_at, l.computed_at
    FROM identity_links l JOIN users ua ON ua.id = l.user_a JOIN users ub ON ub.id = l.user_b
    WHERE l.user_a IN (SELECT user_id FROM participants) OR l.user_b IN (SELECT user_id FROM participants)
    ORDER BY CASE l.level WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END, l.computed_at DESC";

/// Связи участников курса; просмотр записывается в аудит-лог.
#[utoipa::path(get, path = "/api/courses/{course_id}/antifraud", tag = "antifraud", params(("course_id" = Uuid, Path)),
    responses((status = 200, body = CourseLinks)))]
pub(crate) async fn course_links(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(course_id): Path<Uuid>,
) -> ApiResult<Json<CourseLinks>> {
    let _role = access::require(access::course_role(&state.db, &user, course_id).await?, CourseRole::can_view)?;
    let rows: Vec<LinkRow> = sqlx::query_as(COURSE_LINKS).bind(course_id).fetch_all(&state.db).await?;
    AuditEntry::new("antifraud.view", "course").by(Some(user.id), Some(ip)).on(course_id).in_course(course_id).record(&state.db).await?;
    let clusters = clusters(&rows);
    Ok(Json(CourseLinks { links: rows.into_iter().map(view).collect(), clusters }))
}

fn view(row: LinkRow) -> LinkView {
    LinkView {
        user_a: LinkedUser { id: row.user_a, display_name: row.a_name, email: row.a_email },
        user_b: LinkedUser { id: row.user_b, display_name: row.b_name, email: row.b_email },
        level: row.level,
        evidence: row.evidence.0,
        decision: row.decision,
        comment: row.comment,
        decided_at: row.decided_at,
        computed_at: row.computed_at,
    }
}

/// Union-find по связям от среднего уровня, кроме отклонённых.
fn clusters(rows: &[LinkRow]) -> Vec<Vec<Uuid>> {
    let mut parent: HashMap<Uuid, Uuid> = HashMap::new();
    fn root(parent: &mut HashMap<Uuid, Uuid>, node: Uuid) -> Uuid {
        let mut current = node;
        while let Some(next) = parent.get(&current).copied().filter(|next| *next != current) {
            current = next;
        }
        let _compressed = parent.insert(node, current);
        current
    }
    for row in rows.iter().filter(|row| row.level != "low" && row.decision.as_deref() != Some("rejected")) {
        let _a = parent.entry(row.user_a).or_insert(row.user_a);
        let _b = parent.entry(row.user_b).or_insert(row.user_b);
        let (one, two) = (root(&mut parent, row.user_a), root(&mut parent, row.user_b));
        let _merged = parent.insert(one, two);
    }
    let nodes: Vec<Uuid> = parent.keys().copied().collect();
    let mut groups: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for node in nodes {
        let top = root(&mut parent, node);
        groups.entry(top).or_default().push(node);
    }
    groups.into_values().filter(|group| group.len() > 1).map(|mut group| { group.sort_unstable(); group }).collect()
}

/// Решение проверяющего.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct Decision {
    pub course_id: Uuid,
    pub user_a: Uuid,
    pub user_b: Uuid,
    /// `confirmed` или `rejected`.
    pub decision: String,
    #[serde(default)]
    pub comment: String,
}

/// Отмечает связь как подтверждённую или отклонённую.
#[utoipa::path(post, path = "/api/antifraud/decisions", tag = "antifraud", request_body = Decision, responses((status = 204)))]
pub(crate) async fn decide(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Json(request): Json<Decision>,
) -> ApiResult<StatusCode> {
    let _role = access::require(access::course_role(&state.db, &user, request.course_id).await?, CourseRole::can_view)?;
    if !matches!(request.decision.as_str(), "confirmed" | "rejected") {
        return Err(AppError::BadRequest("Решение: confirmed или rejected".to_owned()));
    }
    let (user_a, user_b) = if request.user_a < request.user_b { (request.user_a, request.user_b) } else { (request.user_b, request.user_a) };
    let mut tx = state.db.begin().await?;
    let updated = sqlx::query(
        "UPDATE identity_links SET decision = $3, comment = $4, decided_by = $5, decided_at = $6 WHERE user_a = $1 AND user_b = $2",
    )
    .bind(user_a)
    .bind(user_b)
    .bind(&request.decision)
    .bind(request.comment.trim())
    .bind(user.id)
    .bind(state.clock.now())
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    let details = serde_json::json!({ "user_a": user_a, "user_b": user_b, "decision": request.decision, "comment": request.comment });
    AuditEntry::new("antifraud.decision", "identity_link").by(Some(user.id), Some(ip)).in_course(request.course_id).with(details).record(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

//! Связи для проверяющих: список, кластеры, решения (spec antifraud-signals, «Просмотр и решение по связям»).

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::types::Json as SqlJson;
use sqlx::{FromRow, PgConnection};
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
    evidence: SqlJson<Vec<Evidence>>,
    decision: Option<String>,
    comment: String,
    decided_at: Option<OffsetDateTime>,
    computed_at: OffsetDateTime,
}

const PARTICIPANTS: &str = "participants AS (
        SELECT m.user_id FROM stream_courses sc JOIN stream_members m ON m.stream_id = sc.stream_id WHERE sc.course_id = $1
        UNION SELECT a.user_id FROM attempts a JOIN tests t ON t.id = a.test_id WHERE t.course_id = $1 AND NOT a.is_preview)";

/// Данные пользователя вне курса не раскрываются: только факт связи.
const COURSE_LINKS: &str = "
    WITH participants AS (
        SELECT m.user_id FROM stream_courses sc JOIN stream_members m ON m.stream_id = sc.stream_id WHERE sc.course_id = $1
        UNION SELECT a.user_id FROM attempts a JOIN tests t ON t.id = a.test_id WHERE t.course_id = $1 AND NOT a.is_preview)
    SELECT l.user_a,
           CASE WHEN l.user_a IN (SELECT user_id FROM participants) THEN ua.display_name ELSE 'Пользователь вне курса' END AS a_name,
           CASE WHEN l.user_a IN (SELECT user_id FROM participants) THEN ua.email ELSE '' END AS a_email,
           l.user_b,
           CASE WHEN l.user_b IN (SELECT user_id FROM participants) THEN ub.display_name ELSE 'Пользователь вне курса' END AS b_name,
           CASE WHEN l.user_b IN (SELECT user_id FROM participants) THEN ub.email ELSE '' END AS b_email,
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
    let _role = access::require_on(&state.db, &user, course_id, CourseRole::can_view).await?;
    let rows: Vec<LinkRow> =
        sqlx::query_as(COURSE_LINKS).bind(course_id).fetch_all(&state.db).await?;
    AuditEntry::new("antifraud.view", "course")
        .by(Some(user.id), Some(ip))
        .on(course_id)
        .in_course(course_id)
        .record(&state.db)
        .await?;
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

/// Родитель узла в union-find.
type Parents = HashMap<Uuid, Uuid>;

/// Узлы по корню своего множества.
type Groups = HashMap<Uuid, Vec<Uuid>>;

/// Union-find по связям от среднего уровня, кроме отклонённых.
fn clusters(rows: &[LinkRow]) -> Vec<Vec<Uuid>> {
    let mut parent = union_links(rows);
    let nodes: Vec<Uuid> = parent.keys().copied().collect();
    let mut groups = Groups::new();
    for node in nodes {
        let top = root(&mut parent, node);
        groups.entry(top).or_default().push(node);
    }
    groups
        .into_values()
        .filter(|group| group.len() > 1)
        .map(|mut group| {
            group.sort_unstable();
            group
        })
        .collect()
}

/// Объединяет участников значимых связей.
fn union_links(rows: &[LinkRow]) -> Parents {
    let mut parent = Parents::new();
    for row in
        rows.iter().filter(|row| row.level != "low" && row.decision.as_deref() != Some("rejected"))
    {
        let _a = parent.entry(row.user_a).or_insert(row.user_a);
        let _b = parent.entry(row.user_b).or_insert(row.user_b);
        let (one, two) = (root(&mut parent, row.user_a), root(&mut parent, row.user_b));
        let _merged = parent.insert(one, two);
    }
    parent
}

/// Корень множества узла со сжатием пути.
fn root(parent: &mut Parents, node: Uuid) -> Uuid {
    let mut current = node;
    while let Some(next) = parent.get(&current).copied().filter(|next| *next != current) {
        current = next;
    }
    let _compressed = parent.insert(node, current);
    current
}

/// Решение проверяющего.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct Decision {
    pub course_id: Uuid,
    pub user_a: Uuid,
    pub user_b: Uuid,
    /// `confirmed` или `rejected`.
    #[serde(rename = "decision")]
    pub verdict: String,
    #[serde(default)]
    pub comment: String,
}

impl Decision {
    fn validate(&self) -> ApiResult<()> {
        if matches!(self.verdict.as_str(), "confirmed" | "rejected") {
            Ok(())
        } else {
            Err(AppError::BadRequest("Решение: confirmed или rejected".to_owned()))
        }
    }

    /// Пара в порядке хранения: `user_a < user_b`.
    fn ordered_pair(&self) -> (Uuid, Uuid) {
        if self.user_a < self.user_b {
            (self.user_a, self.user_b)
        } else {
            (self.user_b, self.user_a)
        }
    }
}

/// Отмечает связь как подтверждённую или отклонённую.
#[utoipa::path(post, path = "/api/antifraud/decisions", tag = "antifraud", request_body = Decision, responses((status = 204)))]
pub(crate) async fn decide(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Json(request): Json<Decision>,
) -> ApiResult<StatusCode> {
    let _role =
        access::require_on(&state.db, &user, request.course_id, CourseRole::can_view).await?;
    request.validate()?;
    let mut tx = state.db.begin().await?;
    let (user_a, user_b) = update_decision(&mut tx, &request, user.id, state.clock.now()).await?;
    let details = serde_json::json!({ "user_a": user_a, "user_b": user_b, "decision": request.verdict, "comment": request.comment });
    AuditEntry::new("antifraud.decision", "identity_link")
        .by(Some(user.id), Some(ip))
        .in_course(request.course_id)
        .with(details)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// UPDATE решения, ограниченный связями участников курса `$1`.
fn decision_statement() -> String {
    format!(
        "WITH {PARTICIPANTS}
         UPDATE identity_links SET decision = $4, comment = $5, decided_by = $6, decided_at = $7
         WHERE user_a = $2 AND user_b = $3
           AND (user_a IN (SELECT user_id FROM participants) OR user_b IN (SELECT user_id FROM participants))"
    )
}

/// Записывает решение по связи участника курса; возвращает пару в порядке хранения.
///
/// cancel-safe: yes — в транзакции вызывающего.
async fn update_decision(
    tx: &mut PgConnection,
    request: &Decision,
    decided_by: Uuid,
    now: OffsetDateTime,
) -> ApiResult<(Uuid, Uuid)> {
    let (user_a, user_b) = request.ordered_pair();
    let updated = sqlx::query(sqlx::AssertSqlSafe(decision_statement()))
        .bind(request.course_id)
        .bind(user_a)
        .bind(user_b)
        .bind(&request.verdict)
        .bind(request.comment.trim())
        .bind(decided_by)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok((user_a, user_b))
}

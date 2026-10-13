//! Журнал значимых действий (spec `audit-log`). Только INSERT.

use std::net::IpAddr;

use serde_json::Value as Json;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Запись аудит-лога.
#[derive(Debug, Clone)]
pub(crate) struct AuditEntry {
    pub actor: Option<Uuid>,
    pub action: &'static str,
    pub object_type: &'static str,
    pub object_id: Option<Uuid>,
    pub course_id: Option<Uuid>,
    pub details: Json,
    pub ip: Option<IpAddr>,
}

impl AuditEntry {
    /// Запись без курса и деталей.
    pub(crate) fn new(action: &'static str, object_type: &'static str) -> Self {
        Self {
            actor: None,
            action,
            object_type,
            object_id: None,
            course_id: None,
            details: Json::Object(serde_json::Map::new()),
            ip: None,
        }
    }

    /// Исполнитель и его IP.
    pub(crate) const fn by(mut self, actor: Option<Uuid>, ip: Option<IpAddr>) -> Self {
        self.actor = actor;
        self.ip = ip;
        self
    }

    /// Объект действия.
    pub(crate) const fn on(mut self, object_id: Uuid) -> Self {
        self.object_id = Some(object_id);
        self
    }

    /// Курс, к которому относится действие (для журнала методиста).
    pub(crate) const fn in_course(mut self, course_id: Uuid) -> Self {
        self.course_id = Some(course_id);
        self
    }

    /// Подробности.
    pub(crate) fn with(mut self, details: Json) -> Self {
        self.details = details;
        self
    }

    /// Сохраняет запись.
    ///
    /// cancel-safe: yes — один INSERT.
    pub(crate) async fn record<'conn>(self, executor: impl PgExecutor<'conn>) -> Result<(), sqlx::Error> {
        let _done = sqlx::query(
            "INSERT INTO audit_log (actor_id, action, object_type, object_id, course_id, details, ip)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.actor)
        .bind(self.action)
        .bind(self.object_type)
        .bind(self.object_id)
        .bind(self.course_id)
        .bind(self.details)
        .bind(self.ip)
        .execute(executor)
        .await?;
        Ok(())
    }
}

use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use crate::access::{self, CourseRole};
use crate::auth::session::CurrentUser;
use crate::error::{ApiResult, AppError};
use crate::state::AppState;

/// Фильтры журнала.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct AuditQuery {
    pub course_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    pub action: Option<String>,
    pub object_id: Option<Uuid>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub from: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub to: Option<OffsetDateTime>,
}

/// Запись журнала для просмотра.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct AuditRecord {
    pub id: i64,
    pub actor_id: Option<Uuid>,
    pub actor_name: Option<String>,
    pub action: String,
    pub object_type: String,
    pub object_id: Option<Uuid>,
    pub course_id: Option<Uuid>,
    #[schema(value_type = Object)]
    pub details: Json,
    pub ip: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
}

/// Аудит-лог: весь — суперпользователю, по своему курсу — методисту-владельцу.
#[utoipa::path(get, path = "/api/audit", tag = "audit", params(AuditQuery), responses((status = 200, body = [AuditRecord])))]
pub(crate) async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<AuditQuery>,
) -> ApiResult<axum::Json<Vec<AuditRecord>>> {
    if !user.roles.superuser {
        let course_id = query.course_id.ok_or(AppError::Forbidden)?;
        let _role = access::require(access::course_role(&state.db, &user, course_id).await?, CourseRole::can_edit)?;
    }
    let records = sqlx::query_as(
        "SELECT l.id, l.actor_id, u.display_name AS actor_name, l.action, l.object_type, l.object_id, l.course_id,
                l.details, host(l.ip) AS ip, l.at
         FROM audit_log l LEFT JOIN users u ON u.id = l.actor_id
         WHERE ($1::uuid IS NULL OR l.course_id = $1) AND ($2::uuid IS NULL OR l.actor_id = $2)
           AND ($3::text IS NULL OR l.action = $3) AND ($4::uuid IS NULL OR l.object_id = $4)
           AND ($5::timestamptz IS NULL OR l.at >= $5) AND ($6::timestamptz IS NULL OR l.at <= $6)
         ORDER BY l.at DESC, l.id DESC LIMIT 500",
    )
    .bind(query.course_id)
    .bind(query.actor_id)
    .bind(query.action.as_deref())
    .bind(query.object_id)
    .bind(query.from)
    .bind(query.to)
    .fetch_all(&state.db)
    .await?;
    Ok(axum::Json(records))
}

/// Маршруты журнала.
pub(crate) fn routes() -> utoipa_axum::router::OpenApiRouter<AppState> {
    utoipa_axum::router::OpenApiRouter::new().routes(utoipa_axum::routes!(list))
}

#[cfg(test)]
mod tests;

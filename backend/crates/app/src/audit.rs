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

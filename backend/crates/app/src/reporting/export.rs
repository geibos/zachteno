//! Выгрузка в Excel и в CSV для Excel с русской локалью
//! (spec results-reporting, «Экспорт в Excel», «Экспорт в CSV»).

mod sheets;
mod xlsx;

use std::iter::once;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, header};
use axum::response::Response;
use time::{OffsetDateTime, UtcOffset};
use uuid::Uuid;

use crate::access::{self, CourseRole};
use crate::audit::AuditEntry;
use crate::auth::session::CurrentUser;
use crate::clock::MSK_OFFSET_HOURS;
use crate::error::{ApiResult, AppError};
use crate::http::extract::ClientIp;
use crate::reporting::data::ResultsQuery;
use crate::reporting::tables;
use crate::state::AppState;
use sheets::{Datum, Sheet};

#[cfg(test)]
mod tests;

/// UTF-8 BOM: без него Excel открывает файл не в той кодировке.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Экранирует ячейку, которую табличный редактор принял бы за формулу.
pub(crate) fn neutralize(cell: &str) -> String {
    if cell.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{cell}")
    } else {
        cell.to_owned()
    }
}

/// CSV из первого листа: разделитель `;`, BOM в начале, дроби с запятой.
pub(crate) fn to_csv(sheet: &Sheet) -> ApiResult<Vec<u8>> {
    let mut writer = csv::WriterBuilder::new().delimiter(b';').from_writer(BOM.to_vec());
    let header = sheet.header.iter().map(|title| neutralize(title));
    let rows = sheet.rows.iter().map(|row| row.iter().map(csv_cell).collect::<Vec<_>>());
    for record in once(header.collect()).chain(rows) {
        writer.write_record(&record).map_err(|error| AppError::Internal(error.into()))?;
    }
    writer.into_inner().map_err(|error| AppError::Internal(anyhow::anyhow!("{error}")))
}

/// Ячейка CSV: текст экранируется, числа сервер формирует сам и формулой они не бывают.
fn csv_cell(datum: &Datum) -> String {
    match datum {
        Datum::Text(text) => neutralize(text),
        Datum::Number(number) | Datum::Percent(number) => {
            number.normalize().to_string().replace('.', ",")
        }
        Datum::Moment(moment) => msk(*moment),
        Datum::Empty => String::new(),
    }
}

/// Момент в московском времени.
fn msk_local(moment: OffsetDateTime) -> OffsetDateTime {
    moment.to_offset(UtcOffset::from_hms(MSK_OFFSET_HOURS, 0, 0).unwrap_or(UtcOffset::UTC))
}

fn msk(moment: OffsetDateTime) -> String {
    let local = msk_local(moment);
    format!(
        "{:02}.{:02}.{} {:02}:{:02}",
        local.day(),
        u8::from(local.month()),
        local.year(),
        local.hour(),
        local.minute()
    )
}

/// Формат выгрузки.
#[derive(Debug, Clone, Copy)]
enum FileKind {
    Csv,
    Xlsx,
}

impl FileKind {
    /// CSV получает только первый лист, XLSX — все.
    fn write(self, sheets: &[Sheet]) -> ApiResult<Vec<u8>> {
        match self {
            Self::Csv => sheets.first().map_or_else(|| Ok(BOM.to_vec()), to_csv),
            Self::Xlsx => xlsx::to_xlsx(sheets).map_err(AppError::Internal),
        }
    }

    const fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
        }
    }

    const fn content_type(self) -> &'static str {
        match self {
            Self::Csv => "text/csv; charset=utf-8",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    const fn audit_action(self) -> &'static str {
        match self {
            Self::Csv => "export.csv",
            Self::Xlsx => "export.xlsx",
        }
    }

    fn download(self, bytes: Vec<u8>, stem: &str) -> ApiResult<Response> {
        let mut response = Response::new(Body::from(bytes));
        let headers = response.headers_mut();
        let _type =
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(self.content_type()));
        let disposition = format!("attachment; filename=\"{stem}.{}\"", self.extension());
        let _disposition = headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition)
                .map_err(|error| AppError::Internal(error.into()))?,
        );
        Ok(response)
    }
}

/// Таблица теста с теми же фильтрами в выбранном формате.
async fn test_export(
    state: &AppState,
    (user, ip): (&CurrentUser, ClientIp),
    (test_id, kind): (Uuid, FileKind),
    query: &ResultsQuery,
) -> ApiResult<Response> {
    let report = tables::load_visible(state, user, test_id, query).await?;
    let bytes = kind.write(&sheets::test_sheets(&tables::table(&report)))?;
    let details = serde_json::json!({ "kind": "test", "stream_id": query.stream_id });
    AuditEntry::new(kind.audit_action(), "test")
        .by(Some(user.id), Some(ip.0))
        .on(test_id)
        .in_course(report.test.course_id)
        .with(details)
        .record(&state.db)
        .await?;
    kind.download(bytes, &format!("results-{test_id}"))
}

/// Сводная таблица курса в выбранном формате.
async fn course_export(
    state: &AppState,
    (user, ip): (&CurrentUser, ClientIp),
    (course_id, kind): (Uuid, FileKind),
    query: &ResultsQuery,
) -> ApiResult<Response> {
    let _role = access::require_on(&state.db, user, course_id, CourseRole::can_view).await?;
    let summary = tables::build_summary(state, course_id, query).await?;
    let bytes = kind.write(&[sheets::summary_sheet(&summary)])?;
    AuditEntry::new(kind.audit_action(), "course")
        .by(Some(user.id), Some(ip.0))
        .on(course_id)
        .in_course(course_id)
        .record(&state.db)
        .await?;
    kind.download(bytes, &format!("course-{course_id}"))
}

/// Таблица теста в CSV с теми же фильтрами.
#[utoipa::path(get, path = "/api/tests/{test_id}/results.csv", tag = "reports", params(("test_id" = Uuid, Path), ResultsQuery),
    responses((status = 200, content_type = "text/csv")))]
pub(crate) async fn test_csv(
    State(state): State<AppState>,
    (user, ip): (CurrentUser, ClientIp),
    Path(test_id): Path<Uuid>,
    Query(query): Query<ResultsQuery>,
) -> ApiResult<Response> {
    test_export(&state, (&user, ip), (test_id, FileKind::Csv), &query).await
}

/// Таблица теста в Excel с теми же фильтрами: листы «Результаты», «Ответы», «Вопросы».
#[utoipa::path(get, path = "/api/tests/{test_id}/results.xlsx", tag = "reports", params(("test_id" = Uuid, Path), ResultsQuery),
    responses((status = 200, content_type = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")))]
pub(crate) async fn test_xlsx(
    State(state): State<AppState>,
    (user, ip): (CurrentUser, ClientIp),
    Path(test_id): Path<Uuid>,
    Query(query): Query<ResultsQuery>,
) -> ApiResult<Response> {
    test_export(&state, (&user, ip), (test_id, FileKind::Xlsx), &query).await
}

/// Сводная таблица курса в CSV.
#[utoipa::path(get, path = "/api/courses/{course_id}/summary.csv", tag = "reports", params(("course_id" = Uuid, Path), ResultsQuery),
    responses((status = 200, content_type = "text/csv")))]
pub(crate) async fn course_csv(
    State(state): State<AppState>,
    (user, ip): (CurrentUser, ClientIp),
    Path(course_id): Path<Uuid>,
    Query(query): Query<ResultsQuery>,
) -> ApiResult<Response> {
    course_export(&state, (&user, ip), (course_id, FileKind::Csv), &query).await
}

/// Сводная таблица курса в Excel.
#[utoipa::path(get, path = "/api/courses/{course_id}/summary.xlsx", tag = "reports", params(("course_id" = Uuid, Path), ResultsQuery),
    responses((status = 200, content_type = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")))]
pub(crate) async fn course_xlsx(
    State(state): State<AppState>,
    (user, ip): (CurrentUser, ClientIp),
    Path(course_id): Path<Uuid>,
    Query(query): Query<ResultsQuery>,
) -> ApiResult<Response> {
    course_export(&state, (&user, ip), (course_id, FileKind::Xlsx), &query).await
}

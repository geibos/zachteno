//! Выгрузка в CSV для Excel с русской локалью (spec results-reporting, «Экспорт в CSV»).

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
use crate::reporting::data::{Cell, ResultsQuery};
use crate::reporting::tables::{self, CourseSummaryTable, ResultsTable, TableRow};
use crate::state::AppState;

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

/// CSV из строк: разделитель `;`, BOM в начале.
pub(crate) fn to_csv(records: &[Vec<String>]) -> ApiResult<Vec<u8>> {
    let mut writer = csv::WriterBuilder::new().delimiter(b';').from_writer(BOM.to_vec());
    for record in records {
        writer
            .write_record(record.iter().map(|cell| neutralize(cell)))
            .map_err(|error| AppError::Internal(error.into()))?;
    }
    writer.into_inner().map_err(|error| AppError::Internal(anyhow::anyhow!("{error}")))
}

/// Постоянные столбцы выгрузки теста.
const TEST_HEADER: [&str; 11] = [
    "Студент",
    "Email",
    "Поток",
    "Попытка",
    "Начало (МСК)",
    "Сдача (МСК)",
    "Длительность, мин",
    "Как сдана",
    "Балл",
    "Максимум",
    "Итог",
];

fn test_records(table: &ResultsTable) -> Vec<Vec<String>> {
    let mut header = TEST_HEADER.into_iter().map(str::to_owned).collect::<Vec<_>>();
    header.extend(table.columns.iter().map(|column| format!("В{}", column.number)));
    once(header).chain(table.rows.iter().map(test_record)).collect()
}

fn test_record(row: &TableRow) -> Vec<String> {
    let mut cells = vec![
        row.display_name.clone(),
        row.email.clone(),
        row.stream_name.clone().unwrap_or_default(),
        row.attempt_number.to_string(),
        msk(row.started_at),
        msk(row.submitted_at),
        row.duration_seconds.checked_div(60).unwrap_or_default().to_string(),
        row.submit_reason.clone().unwrap_or_default(),
        row.score.to_string(),
        row.max_score.to_string(),
        row.final_score.map(|score| score.to_string()).unwrap_or_default(),
    ];
    cells.extend(row.cells.iter().map(answer_text));
    cells
}

/// Ответ в ячейке: текст, балл с выбранными вариантами или «—», если вопроса не было.
fn answer_text(cell: &Cell) -> String {
    match (&cell.text_answer, cell.in_attempt) {
        (_, false) => "—".to_owned(),
        (Some(text), true) => text.clone(),
        (None, true) => format!(
            "{} [{}]",
            cell.score.map(|score| score.to_string()).unwrap_or_default(),
            cell.selected.join(", ")
        ),
    }
}

fn summary_records(summary: &CourseSummaryTable) -> Vec<Vec<String>> {
    let mut header = vec!["Студент".to_owned(), "Email".to_owned()];
    header.extend(summary.tests.iter().map(|test| test.title.clone()));
    let rows = summary.rows.iter().map(|row| {
        let mut cells = vec![row.display_name.clone(), row.email.clone()];
        cells.extend(row.cells.iter().map(|cell| match (cell.final_score, cell.max_score) {
            (Some(score), Some(max)) => format!("{score} / {max}"),
            (None, Some(_)) => "скрыт".to_owned(),
            (_, None) => "нет попыток".to_owned(),
        }));
        cells
    });
    once(header).chain(rows).collect()
}

fn msk(moment: OffsetDateTime) -> String {
    let local =
        moment.to_offset(UtcOffset::from_hms(MSK_OFFSET_HOURS, 0, 0).unwrap_or(UtcOffset::UTC));
    format!(
        "{:02}.{:02}.{} {:02}:{:02}",
        local.day(),
        u8::from(local.month()),
        local.year(),
        local.hour(),
        local.minute()
    )
}

fn download(bytes: Vec<u8>, filename: &str) -> ApiResult<Response> {
    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    let _type =
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8"));
    let disposition = format!("attachment; filename=\"{filename}\"");
    let _disposition = headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|error| AppError::Internal(error.into()))?,
    );
    Ok(response)
}

/// Таблица теста в CSV с теми же фильтрами.
#[utoipa::path(get, path = "/api/tests/{test_id}/results.csv", tag = "reports", params(("test_id" = Uuid, Path), ResultsQuery),
    responses((status = 200, content_type = "text/csv")))]
pub(crate) async fn test_csv(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(test_id): Path<Uuid>,
    Query(query): Query<ResultsQuery>,
) -> ApiResult<Response> {
    let report = tables::load_visible(&state, &user, test_id, &query).await?;
    let bytes = to_csv(&test_records(&tables::table(&report)))?;
    let details = serde_json::json!({ "kind": "test", "stream_id": query.stream_id });
    AuditEntry::new("export.csv", "test")
        .by(Some(user.id), Some(ip))
        .on(test_id)
        .in_course(report.test.course_id)
        .with(details)
        .record(&state.db)
        .await?;
    download(bytes, &format!("results-{test_id}.csv"))
}

/// Сводная таблица курса в CSV.
#[utoipa::path(get, path = "/api/courses/{course_id}/summary.csv", tag = "reports", params(("course_id" = Uuid, Path), ResultsQuery),
    responses((status = 200, content_type = "text/csv")))]
pub(crate) async fn course_csv(
    State(state): State<AppState>,
    (user, ClientIp(ip)): (CurrentUser, ClientIp),
    Path(course_id): Path<Uuid>,
    Query(query): Query<ResultsQuery>,
) -> ApiResult<Response> {
    let _role = access::require_on(&state.db, &user, course_id, CourseRole::can_view).await?;
    let summary = tables::build_summary(&state, course_id, &query).await?;
    let bytes = to_csv(&summary_records(&summary))?;
    AuditEntry::new("export.csv", "course")
        .by(Some(user.id), Some(ip))
        .on(course_id)
        .in_course(course_id)
        .record(&state.db)
        .await?;
    download(bytes, &format!("course-{course_id}.csv"))
}

//! Таблица результатов теста и сводная таблица курса (spec results-reporting).

use axum::Json;
use axum::extract::{Path, Query, State};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::{self, CourseRole};
use crate::auth::session::CurrentUser;
use crate::authoring::model;
use crate::error::ApiResult;
use crate::reporting::data::{self, Cell, Results, ResultsQuery};
use crate::state::AppState;

/// Столбец-вопрос.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ColumnHeader {
    pub question_id: Uuid,
    pub number: usize,
    pub body: String,
    pub free_text: bool,
    #[schema(value_type = String)]
    pub weight: Decimal,
}

/// Строка таблицы.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TableRow {
    pub attempt_id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub stream_name: Option<String>,
    pub removed: bool,
    pub attempt_number: usize,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub submitted_at: OffsetDateTime,
    pub duration_seconds: i64,
    pub submit_reason: Option<String>,
    #[schema(value_type = String)]
    pub score: Decimal,
    #[schema(value_type = String)]
    pub max_score: Decimal,
    #[schema(value_type = Option<String>)]
    pub final_score: Option<Decimal>,
    /// Есть ли значимые сигналы антифрода.
    pub antifraud: bool,
    pub cells: Vec<Cell>,
}

/// Таблица теста.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ResultsTable {
    pub test_id: Uuid,
    pub title: String,
    pub columns: Vec<ColumnHeader>,
    pub rows: Vec<TableRow>,
}

/// Строит таблицу из загруженных результатов.
pub(crate) fn table(results: &Results) -> ResultsTable {
    ResultsTable {
        test_id: results.test.id,
        title: results.test.title.clone(),
        columns: (1_usize..)
            .zip(&results.questions)
            .map(|(number, question)| ColumnHeader {
                question_id: question.id,
                number,
                body: question.body.clone(),
                free_text: !question.kind.is_scored(),
                weight: question.weight,
            })
            .collect(),
        rows: results
            .rows
            .iter()
            .map(|row| TableRow {
                attempt_id: row.attempt.attempt_id,
                user_id: row.attempt.user_id,
                display_name: row.attempt.display_name.clone(),
                email: row.attempt.email.clone(),
                stream_name: row.attempt.stream_name.clone(),
                removed: row.attempt.removed,
                attempt_number: row.number,
                started_at: row.attempt.started_at,
                submitted_at: row.attempt.submitted_at,
                duration_seconds: (row.attempt.submitted_at - row.attempt.started_at).whole_seconds(),
                submit_reason: row.attempt.submit_reason.clone(),
                score: row.attempt.score.normalize(),
                max_score: row.attempt.max_score.normalize(),
                final_score: row.final_score,
                antifraud: row.attempt.has_links,
                cells: results.cells(row),
            })
            .collect(),
    }
}

/// Результаты теста для преподавателя и методиста.
#[utoipa::path(get, path = "/api/tests/{test_id}/results", tag = "reports", params(("test_id" = Uuid, Path), ResultsQuery),
    responses((status = 200, body = ResultsTable)))]
pub(crate) async fn test_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(test_id): Path<Uuid>,
    Query(query): Query<ResultsQuery>,
) -> ApiResult<Json<ResultsTable>> {
    let results = load_visible(&state, &user, test_id, &query).await?;
    Ok(Json(table(&results)))
}

/// Загружает результаты, проверив право видеть курс.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn load_visible(state: &AppState, user: &CurrentUser, test_id: Uuid, query: &ResultsQuery) -> ApiResult<Results> {
    let test = model::load_test(&state.db, test_id).await?;
    let _role = access::require(access::course_role(&state.db, user, test.course_id).await?, CourseRole::can_view)?;
    data::load(state, test, query).await
}

/// Итог студента по тесту.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct SummaryCell {
    pub test_id: Uuid,
    #[schema(value_type = Option<String>)]
    pub final_score: Option<Decimal>,
    #[schema(value_type = Option<String>)]
    pub max_score: Option<Decimal>,
    pub attempts: i64,
}

/// Строка сводной таблицы.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SummaryRow {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub cells: Vec<SummaryCell>,
}

/// Сводная таблица курса.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CourseSummary {
    pub tests: Vec<SummaryTest>,
    pub rows: Vec<SummaryRow>,
}

/// Тест-столбец сводной таблицы.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct SummaryTest {
    pub test_id: Uuid,
    pub title: String,
}

#[derive(FromRow)]
struct Student {
    user_id: Uuid,
    display_name: String,
    email: String,
}

/// Студенты × тесты: итоговые баллы (пусто — «нет попыток»).
#[utoipa::path(get, path = "/api/courses/{course_id}/summary", tag = "reports", params(("course_id" = Uuid, Path), ResultsQuery),
    responses((status = 200, body = CourseSummary)))]
pub(crate) async fn course_summary(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(course_id): Path<Uuid>,
    Query(query): Query<ResultsQuery>,
) -> ApiResult<Json<CourseSummary>> {
    let _role = access::require(access::course_role(&state.db, &user, course_id).await?, CourseRole::can_view)?;
    Ok(Json(build_summary(&state, course_id, &query).await?))
}

/// Собирает сводную таблицу курса.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn build_summary(state: &AppState, course_id: Uuid, query: &ResultsQuery) -> ApiResult<CourseSummary> {
    let tests: Vec<SummaryTest> = sqlx::query_as("SELECT id AS test_id, title FROM tests WHERE course_id = $1 AND status = 'published' ORDER BY position")
        .bind(course_id)
        .fetch_all(&state.db)
        .await?;
    let students: Vec<Student> = sqlx::query_as(
        "SELECT DISTINCT u.id AS user_id, u.display_name, u.email FROM stream_courses sc
         JOIN stream_members m ON m.stream_id = sc.stream_id JOIN users u ON u.id = m.user_id
         WHERE sc.course_id = $1 AND ($2::uuid IS NULL OR sc.stream_id = $2) ORDER BY u.display_name",
    )
    .bind(course_id)
    .bind(query.stream_id)
    .fetch_all(&state.db)
    .await?;
    let mut finals = std::collections::HashMap::new();
    for test in &tests {
        let row = model::load_test(&state.db, test.test_id).await?;
        let results = data::load(state, row, query).await?;
        for result in results.rows {
            let _previous = finals.insert((result.attempt.user_id, test.test_id), (result.final_score, result.attempt.max_score, result.number));
        }
    }
    let rows = students
        .into_iter()
        .map(|student| SummaryRow {
            cells: tests
                .iter()
                .map(|test| {
                    let found = finals.get(&(student.user_id, test.test_id));
                    SummaryCell {
                        test_id: test.test_id,
                        final_score: found.and_then(|cell| cell.0),
                        max_score: found.map(|cell| cell.1.normalize()),
                        attempts: found.map_or(0, |cell| i64::try_from(cell.2).unwrap_or(0)),
                    }
                })
                .collect(),
            user_id: student.user_id,
            display_name: student.display_name,
            email: student.email,
        })
        .collect();
    Ok(CourseSummary { tests, rows })
}

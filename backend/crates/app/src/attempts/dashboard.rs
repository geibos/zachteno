//! Панель студента (spec test-taking, «Панель студента»).

use axum::Json;
use axum::extract::State;
use domain::scoring::{self, FinalScorePolicy};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::attempts::review::{self, ReviewAvailability};
use crate::attempts::runner::{AttemptRow, ReadMode, own_attempt};
use crate::auth::session::CurrentUser;
use crate::authoring::model;
use crate::error::ApiResult;
use crate::state::AppState;

#[derive(FromRow)]
struct DashRow {
    test_id: Uuid,
    title: String,
    description: String,
    course_title: String,
    time_limit_minutes: Option<i32>,
    attempts_limit: Option<i32>,
    question_count: i64,
    free_navigation: bool,
    is_open: bool,
    opens_at: Option<OffsetDateTime>,
    closes_at: Option<OffsetDateTime>,
    final_policy: String,
    attempts_used: i64,
    active_attempt_id: Option<Uuid>,
    last_submitted_id: Option<Uuid>,
    scores: Vec<Decimal>,
}

/// Тест на панели.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DashboardTest {
    pub test_id: Uuid,
    pub title: String,
    pub description: String,
    pub course_title: String,
    pub time_limit_minutes: Option<i32>,
    pub attempts_limit: Option<i32>,
    pub attempts_used: i64,
    pub question_count: i64,
    pub free_navigation: bool,
    pub is_open: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub opens_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub closes_at: Option<OffsetDateTime>,
    pub active_attempt_id: Option<Uuid>,
    pub last_submitted_attempt_id: Option<Uuid>,
    pub review: Option<ReviewAvailability>,
    /// Итоговый балл — если результаты уже можно смотреть.
    #[schema(value_type = Option<String>)]
    pub final_score: Option<Decimal>,
}

impl DashboardTest {
    fn can_start(&self) -> bool {
        self.is_open
            && (self.active_attempt_id.is_some()
                || self.attempts_limit.is_none_or(|limit| self.attempts_used < i64::from(limit)))
    }
}

/// Тесты студента по группам.
#[derive(Debug, Default, Serialize, ToSchema)]
pub(crate) struct Dashboard {
    pub available: Vec<DashboardTest>,
    pub scheduled: Vec<DashboardTest>,
    pub completed: Vec<DashboardTest>,
    /// Окно для потока закрыто и не запланировано, сданных попыток нет: тест виден, начать нельзя.
    pub closed: Vec<DashboardTest>,
    /// Названия потоков, в которых студент состоит.
    pub streams: Vec<String>,
}

const DASHBOARD_QUERY: &str = "WITH mine AS (
            SELECT t.id AS test_id, t.title, t.description, c.title AS course_title, t.time_limit_minutes, t.attempts_limit,
                   t.free_navigation, t.final_policy,
                   coalesce(t.questions_per_attempt, (SELECT count(*) FROM questions q WHERE q.test_id = t.id))::bigint AS question_count,
                   bool_or(coalesce(w.is_open, false)) AS is_open,
                   min(w.open_at) FILTER (WHERE NOT coalesce(w.is_open, false)) AS opens_at,
                   min(w.close_at) FILTER (WHERE coalesce(w.is_open, false)) AS closes_at
            FROM stream_members m
            JOIN stream_courses sc ON sc.stream_id = m.stream_id
            JOIN courses c ON c.id = sc.course_id AND c.archived_at IS NULL
            JOIN tests t ON t.course_id = c.id AND t.status = 'published'
            LEFT JOIN test_windows w ON w.test_id = t.id AND w.stream_id = m.stream_id
            WHERE m.user_id = $1 AND m.removed_at IS NULL
            GROUP BY t.id, c.title)
         SELECT mine.*,
            (SELECT count(*) FROM attempts a WHERE a.test_id = mine.test_id AND a.user_id = $1 AND NOT a.is_preview) AS attempts_used,
            (SELECT a.id FROM attempts a WHERE a.test_id = mine.test_id AND a.user_id = $1 AND NOT a.is_preview AND a.submitted_at IS NULL) AS active_attempt_id,
            (SELECT a.id FROM attempts a WHERE a.test_id = mine.test_id AND a.user_id = $1 AND NOT a.is_preview AND a.submitted_at IS NOT NULL
             ORDER BY a.submitted_at DESC LIMIT 1) AS last_submitted_id,
            coalesce((SELECT array_agg(a.score ORDER BY a.submitted_at) FROM attempts a WHERE a.test_id = mine.test_id AND a.user_id = $1
                      AND NOT a.is_preview AND a.submitted_at IS NOT NULL), '{}') AS scores
         FROM mine ORDER BY mine.course_title, mine.title";

/// Доступные, запланированные, пройденные и пока закрытые тесты студента, его потоки.
#[utoipa::path(get, path = "/api/dashboard", tag = "attempts", responses((status = 200, body = Dashboard)))]
pub(crate) async fn dashboard(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Dashboard>> {
    let rows: Vec<DashRow> =
        sqlx::query_as(DASHBOARD_QUERY).bind(user.id).fetch_all(&state.db).await?;
    let mut tests = Vec::new();
    for row in rows {
        tests.push(enrich(&state, &user, row).await?);
    }
    let streams = sqlx::query_scalar(
        "SELECT s.name FROM stream_members m JOIN streams s ON s.id = m.stream_id
         WHERE m.user_id = $1 AND m.removed_at IS NULL ORDER BY s.name",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(Dashboard { streams, ..group(tests, state.clock.now()) }))
}

/// Итог последней сданной попытки: доступность разбора и итоговый балл.
#[derive(Default)]
struct LastResult {
    review: Option<ReviewAvailability>,
    final_score: Option<Decimal>,
}

/// cancel-safe: yes — только чтение.
async fn enrich(state: &AppState, user: &CurrentUser, row: DashRow) -> ApiResult<DashboardTest> {
    let last = match row.last_submitted_id {
        Some(attempt_id) => last_result(state, user, &row, attempt_id).await?,
        None => LastResult::default(),
    };
    Ok(into_test(row, last))
}

/// cancel-safe: yes — только чтение.
async fn last_result(
    state: &AppState,
    user: &CurrentUser,
    row: &DashRow,
    attempt_id: Uuid,
) -> ApiResult<LastResult> {
    let mut connection = state.db.acquire().await?;
    let attempt: AttemptRow =
        own_attempt(&mut connection, user, attempt_id, ReadMode::Plain).await?;
    let test = model::load_test(&mut *connection, row.test_id).await?;
    let availability =
        review::availability(&mut connection, &test, &attempt, state.clock.now()).await?;
    let policy = FinalScorePolicy::parse(&row.final_policy).unwrap_or(FinalScorePolicy::Best);
    let score = scoring::final_score(policy, &row.scores)
        .filter(|_| availability.available)
        .map(|score| score.normalize());
    Ok(LastResult { review: Some(availability), final_score: score })
}

fn into_test(row: DashRow, last: LastResult) -> DashboardTest {
    DashboardTest {
        test_id: row.test_id,
        title: row.title,
        description: row.description,
        course_title: row.course_title,
        time_limit_minutes: row.time_limit_minutes,
        attempts_limit: row.attempts_limit,
        attempts_used: row.attempts_used,
        question_count: row.question_count,
        free_navigation: row.free_navigation,
        is_open: row.is_open,
        opens_at: row.opens_at,
        closes_at: row.closes_at,
        active_attempt_id: row.active_attempt_id,
        last_submitted_attempt_id: row.last_submitted_id,
        review: last.review,
        final_score: last.final_score,
    }
}

fn group(tests: Vec<DashboardTest>, now: OffsetDateTime) -> Dashboard {
    let mut dashboard = Dashboard::default();
    for test in tests {
        let start = test.can_start();
        let planned = !test.is_open && test.opens_at.is_some_and(|opens| opens > now);
        let done = test.last_submitted_attempt_id.is_some();
        match (start, planned, done) {
            (true, _, true) => {
                dashboard
                    .completed
                    .push(DashboardTest { review: test.review.clone(), ..clone_shallow(&test) });
                dashboard.available.push(test);
            }
            (true, _, false) => dashboard.available.push(test),
            (false, true, _) => dashboard.scheduled.push(test),
            (false, false, true) => dashboard.completed.push(test),
            (false, false, false) => dashboard.closed.push(test),
        }
    }
    dashboard
}

fn clone_shallow(test: &DashboardTest) -> DashboardTest {
    DashboardTest {
        test_id: test.test_id,
        title: test.title.clone(),
        description: test.description.clone(),
        course_title: test.course_title.clone(),
        time_limit_minutes: test.time_limit_minutes,
        attempts_limit: test.attempts_limit,
        attempts_used: test.attempts_used,
        question_count: test.question_count,
        free_navigation: test.free_navigation,
        is_open: test.is_open,
        opens_at: test.opens_at,
        closes_at: test.closes_at,
        active_attempt_id: test.active_attempt_id,
        last_submitted_attempt_id: test.last_submitted_attempt_id,
        review: None,
        final_score: test.final_score,
    }
}

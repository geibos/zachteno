//! Начало попытки и предпросмотр (spec test-taking, «Начало попытки»; course-authoring, «Предпросмотр»).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use domain::attempt_rules::Timing;
use domain::layout::{self, PlacedQuestion, Settings, SourceOption, SourceQuestion};
use rand::Rng as _;
use serde::Serialize;
use sqlx::PgConnection;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::{self, CourseRole};
use crate::attempts::lifecycle::log_event;
use crate::auth::session::CurrentUser;
use crate::authoring::model::{self, QuestionDetail, TestRow, TestStatus};
use crate::error::{ApiResult, AppError};
use crate::state::AppState;

/// Начатая (или продолженная) попытка.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct Started {
    pub attempt_id: Uuid,
    /// Продолжена ли уже идущая попытка.
    pub resumed: bool,
}

/// Начинает попытку или продолжает незавершённую.
#[utoipa::path(post, path = "/api/tests/{test_id}/attempts", tag = "attempts", operation_id = "start_attempt", params(("test_id" = Uuid, Path)),
    responses((status = 201, body = Started), (status = 200, body = Started), (status = 409)))]
pub(crate) async fn start(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(test_id): Path<Uuid>,
) -> ApiResult<(StatusCode, Json<Started>)> {
    let test = model::load_test(&state.db, test_id).await?;
    if test.status() != TestStatus::Published {
        return Err(AppError::NotFound);
    }
    let stream_id = open_stream(&state, &user, &test).await?;
    let mut tx = state.db.begin().await?;
    lock_participant(&mut tx, test_id, user.id).await?;
    if let Some(attempt_id) = active_attempt(&mut tx, test_id, user.id).await? {
        tx.commit().await?;
        return Ok((StatusCode::OK, Json(Started { attempt_id, resumed: true })));
    }
    ensure_attempts_left(&mut tx, &test, user.id).await?;
    let attempt_id = create(&mut tx, &state, &test, (user.id, Some(stream_id), false)).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(Started { attempt_id, resumed: false })))
}

/// Поток студента, для которого окно теста открыто.
async fn open_stream(state: &AppState, user: &CurrentUser, test: &TestRow) -> ApiResult<Uuid> {
    let stream: Option<Uuid> = sqlx::query_scalar(
        "SELECT w.stream_id FROM test_windows w
         JOIN stream_members m ON m.stream_id = w.stream_id AND m.user_id = $2 AND m.removed_at IS NULL
         JOIN stream_courses sc ON sc.stream_id = w.stream_id AND sc.course_id = $3
         JOIN streams s ON s.id = w.stream_id
         WHERE w.test_id = $1 AND w.is_open ORDER BY s.name LIMIT 1",
    )
    .bind(test.id)
    .bind(user.id)
    .bind(test.course_id)
    .fetch_optional(&state.db)
    .await?;
    if let Some(stream_id) = stream {
        return Ok(stream_id);
    }
    match access::course_role(&state.db, user, test.course_id).await? {
        Some(_) => Err(AppError::Conflict("Тест сейчас закрыт для вашего потока".to_owned())),
        None => Err(AppError::NotFound),
    }
}

/// Сериализует старт попыток одного участника по одному тесту (две вкладки — одна попытка).
async fn lock_participant(tx: &mut PgConnection, test_id: Uuid, user_id: Uuid) -> ApiResult<()> {
    let _locked = sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text || ':' || $2::text, 0))")
        .bind(test_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

async fn active_attempt(tx: &mut PgConnection, test_id: Uuid, user_id: Uuid) -> ApiResult<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM attempts WHERE test_id = $1 AND user_id = $2 AND submitted_at IS NULL AND NOT is_preview",
    )
    .bind(test_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?)
}

async fn ensure_attempts_left(tx: &mut PgConnection, test: &TestRow, user_id: Uuid) -> ApiResult<()> {
    let Some(limit) = test.attempts_limit else { return Ok(()) };
    let used: i64 = sqlx::query_scalar("SELECT count(*) FROM attempts WHERE test_id = $1 AND user_id = $2 AND NOT is_preview")
        .bind(test.id)
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
    if used >= i64::from(limit) {
        return Err(AppError::Conflict("Все попытки использованы".to_owned()));
    }
    Ok(())
}

/// Создаёт попытку: раскладка по случайному seed, срок по лимиту времени.
///
/// cancel-safe: yes — в транзакции вызывающего.
async fn create(
    tx: &mut PgConnection,
    state: &AppState,
    test: &TestRow,
    (user_id, stream_id, preview): (Uuid, Option<Uuid>, bool),
) -> ApiResult<Uuid> {
    let questions = model::load_questions_in(tx, test.id).await?;
    let placed = place(test, &questions, rand::rng().next_u64());
    let scored: Vec<_> = placed.iter().filter_map(|slot| questions.iter().find(|question| question.id == slot.question_id)).map(QuestionDetail::scored).collect();
    let now = state.clock.now();
    let timing = Timing { started_at: now, limit_minutes: test.time_limit_minutes.and_then(|minutes| u32::try_from(minutes).ok()) };
    let attempt_id = insert(tx, (test.id, user_id, stream_id, preview), (&placed, domain::scoring::max_score(&scored)), (now, timing.deadline())).await?;
    log_event(tx, attempt_id, ("start", None), (serde_json::json!({ "questions": placed.len() }), now)).await?;
    Ok(attempt_id)
}

fn place(test: &TestRow, questions: &[QuestionDetail], seed: u64) -> Vec<PlacedQuestion> {
    let source: Vec<SourceQuestion> = questions
        .iter()
        .map(|question| SourceQuestion {
            question_id: question.id,
            options: question.options.iter().map(|option| SourceOption { option_id: option.id, pinned: option.pinned }).collect(),
        })
        .collect();
    let settings = Settings {
        questions_per_attempt: test.questions_per_attempt.and_then(|count| usize::try_from(count).ok()),
        shuffle_questions: test.shuffle_questions,
        shuffle_options: test.shuffle_options,
    };
    layout::build(&source, settings, seed)
}

async fn insert(
    tx: &mut PgConnection,
    (test_id, user_id, stream_id, preview): (Uuid, Uuid, Option<Uuid>, bool),
    (placed, max_score): (&[PlacedQuestion], rust_decimal::Decimal),
    (now, deadline): (OffsetDateTime, Option<OffsetDateTime>),
) -> ApiResult<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO attempts (test_id, user_id, stream_id, is_preview, started_at, deadline_at, layout, max_score)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(test_id)
    .bind(user_id)
    .bind(stream_id)
    .bind(preview)
    .bind(now)
    .bind(deadline)
    .bind(sqlx::types::Json(placed))
    .bind(max_score)
    .fetch_one(&mut *tx)
    .await?)
}

/// Предпросмотр теста глазами студента: попытка без записи в результаты и без блокировки.
#[utoipa::path(post, path = "/api/tests/{test_id}/preview", tag = "attempts", params(("test_id" = Uuid, Path)),
    responses((status = 201, body = Started)))]
pub(crate) async fn preview(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(test_id): Path<Uuid>,
) -> ApiResult<(StatusCode, Json<Started>)> {
    let test = model::load_test(&state.db, test_id).await?;
    let _role = access::require(access::course_role(&state.db, &user, test.course_id).await?, CourseRole::can_view)?;
    let mut tx = state.db.begin().await?;
    let attempt_id = create(&mut tx, &state, &test, (user.id, None, true)).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(Started { attempt_id, resumed: false })))
}

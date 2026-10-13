//! Модель контента в том виде, в каком её видят редактор и прохождение.

use std::collections::HashMap;

use domain::scoring::{FinalScorePolicy, QuestionKind, ScoredOption, ScoredQuestion};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgExecutor};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ApiResult, AppError};

/// Статус теста.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TestStatus {
    Draft,
    Published,
    Archived,
}

impl TestStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }

    pub(crate) fn parse(name: &str) -> Self {
        match name {
            "published" => Self::Published,
            "archived" => Self::Archived,
            _ => Self::Draft,
        }
    }
}

/// Политика показа результатов (без даты — она отдельным полем).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReviewMode {
    Never,
    AfterSubmit,
    AfterClose,
}

impl ReviewMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::AfterSubmit => "after_submit",
            Self::AfterClose => "after_close",
        }
    }

    pub(crate) fn parse(name: &str) -> Self {
        match name {
            "never" => Self::Never,
            "after_close" => Self::AfterClose,
            _ => Self::AfterSubmit,
        }
    }

    /// Политика домена с датой «не раньше».
    pub(crate) const fn policy(self, not_before: Option<OffsetDateTime>) -> domain::review::Policy {
        match self {
            Self::Never => domain::review::Policy::Never,
            Self::AfterSubmit => domain::review::Policy::AfterSubmit,
            Self::AfterClose => domain::review::Policy::AfterClose { not_before },
        }
    }
}

/// Настройки теста (spec course-authoring, «Настройки теста»).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct TestSettings {
    /// Лимит попыток; `null` — без ограничения.
    pub attempts_limit: Option<i32>,
    /// Лимит времени в минутах; `null` — без лимита.
    pub time_limit_minutes: Option<i32>,
    /// Сколько вопросов в попытке; `null` — все.
    pub questions_per_attempt: Option<i32>,
    /// Возврат и пропуск вопросов (один флаг).
    pub free_navigation: bool,
    pub shuffle_questions: bool,
    pub shuffle_options: bool,
    pub review_policy: ReviewMode,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub review_not_before: Option<OffsetDateTime>,
    pub final_policy: FinalScorePolicy,
}

impl Default for TestSettings {
    fn default() -> Self {
        Self {
            attempts_limit: Some(1),
            time_limit_minutes: None,
            questions_per_attempt: None,
            free_navigation: false,
            shuffle_questions: false,
            shuffle_options: false,
            review_policy: ReviewMode::AfterSubmit,
            review_not_before: None,
            final_policy: FinalScorePolicy::Best,
        }
    }
}

impl TestSettings {
    /// Совпадают ли настройки, кроме политики показа (её можно менять у заблокированного теста).
    pub(crate) fn same_except_review(&self, other: &Self) -> bool {
        let strip = |settings: &Self| Self {
            review_policy: ReviewMode::Never,
            review_not_before: None,
            ..settings.clone()
        };
        strip(self) == strip(other)
    }

    /// Проверка диапазонов.
    pub(crate) fn validate(&self) -> ApiResult<()> {
        let positive = |field: Option<i32>| field.is_none_or(|count| count >= 1);
        if positive(self.attempts_limit) && positive(self.time_limit_minutes) && positive(self.questions_per_attempt) {
            Ok(())
        } else {
            Err(AppError::BadRequest("Лимиты должны быть не меньше 1".to_owned()))
        }
    }
}

/// Строка теста из базы.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct TestRow {
    pub id: Uuid,
    pub course_id: Uuid,
    pub title: String,
    pub description: String,
    pub status: String,
    pub position: i32,
    pub attempts_limit: Option<i32>,
    pub time_limit_minutes: Option<i32>,
    pub questions_per_attempt: Option<i32>,
    pub free_navigation: bool,
    pub shuffle_questions: bool,
    pub shuffle_options: bool,
    pub review_policy: String,
    pub review_not_before: Option<OffsetDateTime>,
    pub final_policy: String,
    pub locked_at: Option<OffsetDateTime>,
}

/// Колонки `TestRow` для SELECT.
pub(crate) const TEST_COLUMNS: &str = "id, course_id, title, description, status, position, attempts_limit,
    time_limit_minutes, questions_per_attempt, free_navigation, shuffle_questions, shuffle_options,
    review_policy, review_not_before, final_policy, locked_at";

impl TestRow {
    pub(crate) fn settings(&self) -> TestSettings {
        TestSettings {
            attempts_limit: self.attempts_limit,
            time_limit_minutes: self.time_limit_minutes,
            questions_per_attempt: self.questions_per_attempt,
            free_navigation: self.free_navigation,
            shuffle_questions: self.shuffle_questions,
            shuffle_options: self.shuffle_options,
            review_policy: ReviewMode::parse(&self.review_policy),
            review_not_before: self.review_not_before,
            final_policy: FinalScorePolicy::parse(&self.final_policy).unwrap_or(FinalScorePolicy::Best),
        }
    }

    pub(crate) fn status(&self) -> TestStatus {
        TestStatus::parse(&self.status)
    }
}

/// Загружает тест.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn load_test<'conn>(executor: impl PgExecutor<'conn>, test_id: Uuid) -> ApiResult<TestRow> {
    let query = format!("SELECT {TEST_COLUMNS} FROM tests WHERE id = $1");
    sqlx::query_as(sqlx::AssertSqlSafe(query)).bind(test_id).fetch_optional(executor).await?.ok_or(AppError::NotFound)
}

/// Картинка, прикреплённая к вопросу или варианту.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct ImageRef {
    pub id: Uuid,
    pub url: String,
    pub alt: String,
}

impl ImageRef {
    pub(crate) fn from_parts(image_id: Option<Uuid>, alt: &str) -> Option<Self> {
        image_id.map(|id| Self { id, url: format!("/api/files/{id}"), alt: alt.to_owned() })
    }
}

/// Вариант ответа для редактора (с правильностью и штрафом).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct OptionDetail {
    pub id: Uuid,
    pub body: String,
    pub image: Option<ImageRef>,
    pub is_correct: bool,
    #[schema(value_type = String)]
    pub penalty: Decimal,
    pub pinned: bool,
}

/// Вопрос для редактора.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct QuestionDetail {
    pub id: Uuid,
    pub position: i32,
    pub kind: QuestionKind,
    pub body: String,
    #[schema(value_type = String)]
    pub weight: Decimal,
    pub image: Option<ImageRef>,
    pub options: Vec<OptionDetail>,
}

impl QuestionDetail {
    /// Вопрос в виде, понятном оцениванию.
    pub(crate) fn scored(&self) -> ScoredQuestion {
        ScoredQuestion {
            kind: self.kind,
            weight: self.weight,
            options: self
                .options
                .iter()
                .map(|option| ScoredOption { option_id: option.id, is_correct: option.is_correct, penalty: option.penalty })
                .collect(),
        }
    }
}

#[derive(FromRow)]
struct QuestionRow {
    id: Uuid,
    position: i32,
    kind: String,
    body: String,
    weight: Decimal,
    image_id: Option<Uuid>,
    image_alt: String,
}

#[derive(FromRow)]
struct OptionRow {
    id: Uuid,
    question_id: Uuid,
    body: String,
    image_id: Option<Uuid>,
    image_alt: String,
    is_correct: bool,
    penalty: Decimal,
    pinned: bool,
}

/// Все вопросы теста с вариантами в порядке редактора.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn load_questions(db: &sqlx::PgPool, test_id: Uuid) -> ApiResult<Vec<QuestionDetail>> {
    let questions: Vec<QuestionRow> = sqlx::query_as(
        "SELECT id, position, kind, body, weight, image_id, image_alt FROM questions WHERE test_id = $1 ORDER BY position, id",
    )
    .bind(test_id)
    .fetch_all(db)
    .await?;
    let options: Vec<OptionRow> = sqlx::query_as(
        "SELECT o.id, o.question_id, o.body, o.image_id, o.image_alt, o.is_correct, o.penalty, o.pinned
         FROM options o JOIN questions q ON q.id = o.question_id WHERE q.test_id = $1 ORDER BY o.position, o.id",
    )
    .bind(test_id)
    .fetch_all(db)
    .await?;
    Ok(assemble(questions, options))
}

fn assemble(questions: Vec<QuestionRow>, options: Vec<OptionRow>) -> Vec<QuestionDetail> {
    let mut grouped: HashMap<Uuid, Vec<OptionDetail>> = HashMap::new();
    for option in options {
        grouped.entry(option.question_id).or_default().push(OptionDetail {
            id: option.id,
            image: ImageRef::from_parts(option.image_id, &option.image_alt),
            body: option.body,
            is_correct: option.is_correct,
            penalty: option.penalty.normalize(),
            pinned: option.pinned,
        });
    }
    questions
        .into_iter()
        .map(|question| QuestionDetail {
            options: grouped.remove(&question.id).unwrap_or_default(),
            kind: QuestionKind::parse(&question.kind).unwrap_or(QuestionKind::FreeText),
            image: ImageRef::from_parts(question.image_id, &question.image_alt),
            id: question.id,
            position: question.position,
            body: question.body,
            weight: question.weight.normalize(),
        })
        .collect()
}

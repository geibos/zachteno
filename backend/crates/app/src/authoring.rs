//! Курсы, тесты, вопросы, картинки (spec `course-authoring`).

mod attachments;
mod cloning;
pub(crate) mod courses;
pub(crate) mod model;
mod questions;
pub(crate) mod quizzes;
mod teachers;

#[cfg(test)]
mod tests;

use axum::extract::DefaultBodyLimit;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt as _};
use utoipa_axum::routes;

use crate::blobs::MAX_UPLOAD_BYTES;
use crate::state::AppState;

/// Маршруты редактора.
pub(crate) fn routes() -> OpenApiRouter<AppState> {
    question_routes(test_routes(course_routes(OpenApiRouter::new())))
}

/// Курсы и их преподаватели.
fn course_routes(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(courses::list_courses, courses::create_course))
        .routes(routes!(courses::get_course, courses::update_course))
        .routes(routes!(cloning::clone_course))
        .routes(routes!(teachers::list_teachers, teachers::assign_teacher))
        .routes(routes!(teachers::remove_teacher))
}

/// Тесты: создание, правка, публикация, архив, копирование.
fn test_routes(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(quizzes::create_test))
        .routes(routes!(quizzes::get_test, quizzes::update_test))
        .routes(routes!(quizzes::publish_test))
        .routes(routes!(quizzes::archive_test))
        .routes(routes!(cloning::clone_test))
}

/// Вопросы и картинки к ним.
fn question_routes(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(questions::create_question))
        .routes(routes!(questions::update_question, questions::delete_question))
        .routes(routes!(questions::reorder_questions))
        .routes(
            routes!(attachments::upload).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES + 64 * 1024)),
        )
        .routes(routes!(attachments::serve))
}

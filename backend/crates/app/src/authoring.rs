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

use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt as _};
use utoipa_axum::routes;

use crate::state::AppState;

/// Маршруты редактора.
pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(courses::list_courses, courses::create_course))
        .routes(routes!(courses::get_course, courses::update_course))
        .routes(routes!(cloning::clone_course))
        .routes(routes!(teachers::list_teachers, teachers::assign_teacher))
        .routes(routes!(teachers::remove_teacher))
        .routes(routes!(quizzes::create_test))
        .routes(routes!(quizzes::get_test, quizzes::update_test))
        .routes(routes!(quizzes::publish_test))
        .routes(routes!(quizzes::archive_test))
        .routes(routes!(cloning::clone_test))
        .routes(routes!(questions::create_question))
        .routes(routes!(questions::update_question, questions::delete_question))
        .routes(routes!(questions::reorder_questions))
        .routes(routes!(attachments::upload).layer(axum::extract::DefaultBodyLimit::max(crate::blobs::MAX_UPLOAD_BYTES + 64 * 1024)))
        .routes(routes!(attachments::serve))
}

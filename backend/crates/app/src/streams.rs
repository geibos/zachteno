//! Потоки, приглашения, окна прохождения (spec `streams-enrollment`).

pub(crate) mod access;
pub(crate) mod invites;
mod management;
pub(crate) mod windows;

#[cfg(test)]
mod tests;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

/// Маршруты потоков.
pub(crate) fn routes() -> OpenApiRouter<AppState> {
    access_routes(management_routes(OpenApiRouter::new()))
}

/// Потоки, их курсы и участники.
fn management_routes(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(management::list_streams, management::create_stream))
        .routes(routes!(management::get_stream))
        .routes(routes!(management::assign_course))
        .routes(routes!(management::unassign_course))
        .routes(routes!(management::remove_member))
}

/// Приглашения и окна тестов.
fn access_routes(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(invites::create_invite, invites::list_invites))
        .routes(routes!(invites::invite_uses))
        .routes(routes!(invites::invite_info))
        .routes(routes!(invites::join))
        .routes(routes!(windows::list_windows))
        .routes(routes!(windows::change_window))
}

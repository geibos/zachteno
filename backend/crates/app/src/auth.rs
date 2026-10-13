//! Учётные записи и вход (spec `identity-auth`).

pub(crate) mod account;
mod login;
mod oauth;
pub(crate) mod onetime;
pub(crate) mod password;
mod profile;
mod recovery;
pub(crate) mod register;
pub(crate) mod session;
#[cfg(test)]
mod tests;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

/// Маршруты входа, регистрации и профиля.
pub(crate) fn routes() -> OpenApiRouter<AppState> {
    profile_routes(account_routes(OpenApiRouter::new()))
}

/// Регистрация, вход, подтверждение email и восстановление пароля.
fn account_routes(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(register::register))
        .routes(routes!(login::login))
        .routes(routes!(login::logout))
        .routes(routes!(login::me))
        .routes(routes!(recovery::verify_email))
        .routes(routes!(recovery::resend_verification))
        .routes(routes!(recovery::forgot_password))
        .routes(routes!(recovery::reset_password))
}

/// Профиль, сессии и внешние входы.
fn profile_routes(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(profile::change_password))
        .routes(routes!(profile::list_sessions))
        .routes(routes!(profile::revoke_session))
        .routes(routes!(profile::list_identities))
        .routes(routes!(profile::unlink_identity))
        .routes(routes!(oauth::start))
        .routes(routes!(oauth::callback))
}

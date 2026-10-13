//! Учётные записи и вход (spec `identity-auth`).

pub(crate) mod account;
mod login;
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
    OpenApiRouter::new()
        .routes(routes!(register::register))
        .routes(routes!(login::login))
        .routes(routes!(login::logout))
        .routes(routes!(login::me))
        .routes(routes!(recovery::verify_email))
        .routes(routes!(recovery::resend_verification))
        .routes(routes!(recovery::forgot_password))
        .routes(routes!(recovery::reset_password))
        .routes(routes!(profile::change_password))
        .routes(routes!(profile::list_sessions))
        .routes(routes!(profile::revoke_session))
}

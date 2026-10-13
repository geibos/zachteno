//! Экстракторы: IP клиента с учётом доверенных прокси и текущий пользователь.

use std::future::ready;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts, Path};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use ipnet::IpNet;
use uuid::Uuid;

use crate::auth::session::{self, CurrentUser};
use crate::error::AppError;
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Путь с двумя идентификаторами, например `/api/courses/{course_id}/teachers/{user_id}`.
pub(crate) type PathPair = Path<(Uuid, Uuid)>;

/// IP клиента: `X-Forwarded-For` учитывается только от доверенного прокси.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = AppError;

    /// cancel-safe: yes — готовое значение, без ожидания.
    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map_or(IpAddr::V4(Ipv4Addr::LOCALHOST), |connect| connect.0.ip());
        ready(Ok(Self(resolve_client_ip(peer, &parts.headers, &state.config.http.trusted_proxies))))
    }
}

/// Идёт по `X-Forwarded-For` справа налево и берёт первый адрес не из доверенных.
pub(crate) fn resolve_client_ip(peer: IpAddr, headers: &HeaderMap, trusted: &[IpNet]) -> IpAddr {
    let is_trusted = |address: &IpAddr| trusted.iter().any(|net| net.contains(address));
    if !is_trusted(&peer) {
        return peer;
    }
    headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|header_value| header_value.to_str().ok())
        .flat_map(|list| list.split(',').map(str::trim).collect::<Vec<_>>())
        .filter_map(|part| part.parse::<IpAddr>().ok())
        .rev()
        .find(|address| !is_trusted(address))
        .unwrap_or(peer)
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        session::authenticate(state, &parts.headers).await?.ok_or(AppError::Unauthorized)
    }
}

/// Пользователь, если он вошёл.
#[derive(Debug, Clone)]
pub(crate) struct MaybeUser(pub Option<CurrentUser>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(session::authenticate(state, &parts.headers).await?))
    }
}

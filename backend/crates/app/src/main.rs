//! «Зачтено» — HTTP-сервер и служебные команды сервиса тестирования студентов.

mod access;
mod admin;
mod audit;
mod auth;
mod cli;
mod clock;
mod config;
mod database;
mod error;
mod http;
mod jobs;
mod mail;
mod ops;
mod secret;
mod state;
#[cfg(test)]
mod testing;
mod tokens;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::run().await
}

//! «Зачтено» — HTTP-сервер и служебные команды сервиса тестирования студентов.

mod access;
mod admin;
mod antifraud;
mod attempts;
mod audit;
mod blobs;
mod auth;
mod authoring;
mod cli;
mod clock;
mod config;
mod database;
mod error;
mod http;
mod jobs;
mod mail;
mod ops;
mod reporting;
mod secret;
mod seed;
mod state;
mod streams;
mod sweeper;
#[cfg(test)]
mod testing;
mod tokens;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::run().await
}

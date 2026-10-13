//! Команды бинаря: сервер, миграции, первый суперпользователь, OpenAPI.

use std::net::SocketAddr;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;
use crate::{database, http, mail};

/// Сервис тестирования студентов «Зачтено».
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Применить миграции и запустить HTTP-сервер.
    Serve,
    /// Только применить миграции.
    Migrate,
    /// Вывести документ OpenAPI (для генерации типов фронтенда).
    Openapi,
    /// Создать суперпользователя (или выдать роль существующему) и вывести ссылку для пароля.
    CreateSuperuser {
        /// Email учётной записи.
        #[arg(long)]
        email: String,
        /// Отображаемое имя для новой учётной записи.
        #[arg(long, default_value = "Администратор")]
        name: String,
    },
}

pub(crate) async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Command::Openapi) {
        return print_openapi();
    }
    init_tracing();
    let config = Config::from_env()?;
    match cli.command {
        Command::Serve => serve(config).await,
        Command::Migrate => database::migrate(&config).await,
        Command::Openapi => print_openapi(),
        Command::CreateSuperuser { email, name } => create_superuser(config, &email, &name).await,
    }
}

fn print_openapi() -> anyhow::Result<()> {
    use std::io::Write as _;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{}", http::openapi_json()?)?;
    Ok(())
}

async fn create_superuser(config: Config, email: &str, name: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    database::migrate(&config).await?;
    let db = database::connect(&config).await?;
    let state = AppState::new(config, db)?;
    let link = crate::admin::bootstrap_superuser(&state, email, name).await?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "Суперпользователь: {email}")?;
    match link {
        Some(url) => writeln!(stdout, "Задайте пароль по ссылке (действует 24 часа):\n{url}")?,
        None => writeln!(stdout, "У учётной записи уже есть пароль — роль выдана.")?,
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().json().with_env_filter(filter).with_current_span(false).init();
}

/// cancel-safe: yes — завершается по Ctrl+C/SIGTERM с корректной остановкой.
async fn serve(config: Config) -> anyhow::Result<()> {
    database::migrate(&config).await?;
    let db = database::connect(&config).await?;
    let bind = config.http.bind;
    let state = AppState::new(config, db)?;
    spawn_workers(&state);
    let listener = TcpListener::bind(bind).await.with_context(|| format!("bind {bind}"))?;
    tracing::info!(%bind, "сервер запущен");
    axum::serve(listener, http::app(state).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn spawn_workers(state: &AppState) {
    let mail_state = state.clone();
    drop(tokio::spawn(async move {
        if let Err(error) = mail::run_worker(mail_state).await {
            tracing::error!(%error, "воркер писем остановлен");
        }
    }));
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "не удалось подписаться на Ctrl+C");
        }
    };
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => drop(signal.recv().await),
            Err(error) => tracing::error!(%error, "не удалось подписаться на SIGTERM"),
        }
    };
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

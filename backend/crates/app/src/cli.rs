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
    /// Проверка живости для Docker healthcheck (без окружения).
    Healthcheck {
        /// Адрес проверки.
        #[arg(long, default_value = "http://127.0.0.1:8080/health/live")]
        url: String,
    },
    /// Загрузить демонстрационный курс ПДД из каталога `etspring/pdd_russia`.
    SeedPdd {
        /// Каталог с клоном набора.
        #[arg(long)]
        dir: std::path::PathBuf,
        /// Email владельца курса (методист или суперпользователь).
        #[arg(long)]
        owner: String,
    },
}

pub(crate) async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Command::Openapi) {
        return print_openapi();
    }
    if let Command::Healthcheck { url } = &cli.command {
        let response = reqwest::Client::new().get(url).timeout(std::time::Duration::from_secs(3)).send().await?;
        anyhow::ensure!(response.status().is_success(), "статус {}", response.status());
        return Ok(());
    }
    init_tracing();
    let config = Config::from_env()?;
    match cli.command {
        Command::Serve => serve(config).await,
        Command::Migrate => database::migrate(&config).await,
        Command::Openapi => print_openapi(),
        Command::Healthcheck { .. } => Ok(()),
        Command::CreateSuperuser { email, name } => create_superuser(config, &email, &name).await,
        Command::SeedPdd { dir, owner } => seed_pdd(config, &dir, &owner).await,
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

async fn seed_pdd(config: Config, dir: &std::path::Path, owner: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    database::migrate(&config).await?;
    let db = database::connect(&config).await?;
    let state = AppState::new(config, db)?;
    let owner_row = crate::auth::account::find_for_login(&state.db, owner).await?.with_context(|| format!("нет пользователя {owner}"))?;
    let seeded = crate::seed::seed_pdd(&state, dir, owner_row.id).await?;
    let mut stdout = std::io::stdout().lock();
    if seeded.created {
        writeln!(stdout, "Курс создан: {} вопросов, {} картинок, id {}", seeded.questions, seeded.images, seeded.course_id)?;
    } else {
        writeln!(stdout, "Курс уже есть (id {}, {} вопросов) — ничего не делаю", seeded.course_id, seeded.questions)?;
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
    let metrics_bind = config.http.metrics_bind;
    let state = AppState::new(config, db)?;
    spawn_workers(&state);
    let metrics_listener = TcpListener::bind(metrics_bind).await.with_context(|| format!("bind {metrics_bind}"))?;
    let metrics_app = crate::ops::metrics_router(state.clone());
    drop(tokio::spawn(async move {
        if let Err(error) = axum::serve(metrics_listener, metrics_app).await {
            tracing::error!(%error, "сервер метрик остановлен");
        }
    }));
    let listener = TcpListener::bind(bind).await.with_context(|| format!("bind {bind}"))?;
    tracing::info!(%bind, "сервер запущен");
    axum::serve(listener, http::app(state).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn spawn_workers(state: &AppState) {
    drop(tokio::spawn(crate::sweeper::run(state.clone())));
    drop(tokio::spawn(crate::antifraud::run_worker(state.clone())));
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

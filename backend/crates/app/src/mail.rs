//! Письма: шаблоны и отправка через SMTP (Postbox) или в каталог (design D6).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context as _;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport as _, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};
use tokio::fs::{create_dir_all, write};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

use crate::config::MailConfig;
use crate::jobs::{self, JobKind};
use crate::state::AppState;

/// Не чаще одного письма в секунду (квота Postbox по умолчанию).
const SEND_INTERVAL: Duration = Duration::from_secs(1);

/// Письмо в очереди.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "template", rename_all = "snake_case")]
pub(crate) enum Email {
    /// Подтверждение адреса.
    #[serde(rename = "verify_email")]
    VerifyAddress { to: String, name: String, token: String },
    /// Восстановление пароля.
    ResetPassword { to: String, name: String, token: String },
}

impl Email {
    fn recipient(&self) -> &str {
        match self {
            Self::VerifyAddress { to, .. } | Self::ResetPassword { to, .. } => to,
        }
    }

    /// Тема и текст письма.
    pub(crate) fn render(&self, public_url: &str) -> (String, String) {
        match self {
            Self::VerifyAddress { name, token, .. } => (
                "Подтвердите адрес почты — Зачтено".to_owned(),
                format!(
                    "Здравствуйте, {name}!\n\nПодтвердите адрес почты по ссылке:\n{public_url}/verify-email?token={token}\n\n\
                     Ссылка действует 24 часа. Если вы не регистрировались в «Зачтено», просто проигнорируйте письмо.\n"
                ),
            ),
            Self::ResetPassword { name, token, .. } => (
                "Восстановление пароля — Зачтено".to_owned(),
                format!(
                    "Здравствуйте, {name}!\n\nЗадать новый пароль можно по ссылке:\n{public_url}/reset-password?token={token}\n\n\
                     Ссылка действует 1 час. Если вы не запрашивали восстановление, проигнорируйте письмо.\n"
                ),
            ),
        }
    }
}

/// Ставит письмо в очередь (в транзакции вызывающего).
///
/// cancel-safe: yes — один INSERT.
pub(crate) async fn queue<'conn>(
    executor: impl sqlx::PgExecutor<'conn>,
    email: &Email,
    now: time::OffsetDateTime,
) -> Result<(), sqlx::Error> {
    jobs::enqueue(executor, JobKind::Email, email, now).await
}

/// Способ доставки.
enum Transport {
    Smtp { mailer: AsyncSmtpTransport<Tokio1Executor>, from: Mailbox },
    Directory { path: PathBuf, from: Mailbox },
}

impl Transport {
    fn from_config(config: &MailConfig) -> anyhow::Result<Self> {
        match config {
            MailConfig::Smtp { host, port, username, password, from } => {
                let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?
                    .port(*port)
                    .credentials(Credentials::new(username.clone(), password.expose().clone()))
                    .build();
                Ok(Self::Smtp { mailer, from: from.parse().context("MAIL_FROM")? })
            }
            MailConfig::Directory { path, from } => Ok(Self::Directory {
                path: PathBuf::from(path),
                from: from.parse().context("MAIL_FROM")?,
            }),
        }
    }

    /// Адрес отправителя.
    const fn sender(&self) -> &Mailbox {
        match self {
            Self::Smtp { from, .. } | Self::Directory { from, .. } => from,
        }
    }

    /// cancel-safe: NO — письмо может уйти, а отметка о выполнении — нет; повторная отправка допустима.
    async fn send(&self, message: Message) -> anyhow::Result<()> {
        match self {
            Self::Smtp { mailer, .. } => {
                let _response = mailer.send(message).await?;
            }
            Self::Directory { path, .. } => {
                create_dir_all(path).await?;
                let file = path.join(format!("{}.eml", Uuid::new_v4()));
                write(file, message.formatted()).await?;
            }
        }
        Ok(())
    }
}

/// Воркер писем: берёт задачи по одной и отправляет не чаще раза в секунду.
///
/// cancel-safe: NO — см. `Transport::send`; запускается отдельной задачей и живёт до остановки процесса.
pub(crate) async fn run_worker(state: AppState) -> anyhow::Result<()> {
    let transport = Transport::from_config(&state.config.mail)?;
    work(&state, &transport).await
}

/// Бесконечный цикл воркера писем.
///
/// cancel-safe: NO — см. `Transport::send`.
async fn work(state: &AppState, transport: &Transport) -> ! {
    loop {
        step(state, transport).await;
    }
}

/// Одна итерация: письмо и пауза, ожидание новой задачи или пауза после ошибки очереди.
///
/// cancel-safe: NO — см. `Transport::send`.
async fn step(state: &AppState, transport: &Transport) {
    match jobs::claim(state, Some(JobKind::Email)).await {
        Ok(Some(job)) => {
            deliver(state, transport, &job).await;
            sleep(SEND_INTERVAL).await;
        }
        Ok(None) => {
            let _woken = timeout(jobs::IDLE_POLL, state.jobs_wake.notified()).await;
        }
        Err(error) => queue_unavailable(&error).await,
    }
}

/// Пишет ошибку очереди и выжидает перед следующей попыткой.
///
/// cancel-safe: yes — только пауза.
async fn queue_unavailable(error: &sqlx::Error) {
    tracing::error!(%error, "очередь писем недоступна");
    sleep(jobs::IDLE_POLL).await;
}

/// cancel-safe: NO — см. `Transport::send`.
async fn deliver(state: &AppState, transport: &Transport, job: &jobs::Claimed) {
    let recorded = match send_job(state, transport, job).await {
        Ok(()) => jobs::complete(state, job.id).await,
        Err(error) => fail_job(state, job, &error).await,
    };
    if let Err(error) = recorded {
        report_unrecorded(job, &error);
    }
}

/// cancel-safe: NO — см. `Transport::send`.
async fn send_job(
    state: &AppState,
    transport: &Transport,
    job: &jobs::Claimed,
) -> anyhow::Result<()> {
    transport.send(build(state, transport, job)?).await
}

/// cancel-safe: yes — один UPDATE.
async fn fail_job(
    state: &AppState,
    job: &jobs::Claimed,
    error: &anyhow::Error,
) -> Result<(), sqlx::Error> {
    tracing::warn!(job = job.id, %error, "письмо не отправлено");
    jobs::fail(state, job, &error.to_string()).await
}

fn report_unrecorded(job: &jobs::Claimed, error: &sqlx::Error) {
    tracing::error!(job = job.id, %error, "не удалось обновить задачу письма");
}

fn build(state: &AppState, transport: &Transport, job: &jobs::Claimed) -> anyhow::Result<Message> {
    let email: Email = job.payload()?;
    let (subject, body) = email.render(&state.config.http.public_url);
    Ok(Message::builder()
        .from(transport.sender().clone())
        .to(email.recipient().parse().context("адрес получателя")?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body)?)
}

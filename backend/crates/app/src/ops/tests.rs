use std::sync::Arc;

use axum::extract::State;
use axum::http::{Method, StatusCode};
use object_store::RetryConfig;
use object_store::aws::AmazonS3Builder;
use sqlx::PgPool;

use crate::blobs::Blobs;
use crate::testing::{self, Client};

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn metrics_expose_backup_timestamps(db: PgPool) {
    let state = testing::state(db);
    let _status = sqlx::query("INSERT INTO ops_status (name, last_success_at) VALUES ('backup', to_timestamp(1800000000)), ('restore_check', NULL)")
        .execute(&state.db).await.unwrap();
    let (_, text) = super::metrics(State(state)).await;
    assert!(
        text.contains("zachteno_backup_last_success_timestamp 1800000000"),
        "время бэкапа: {text}"
    );
    assert!(
        text.contains("zachteno_restore_check_last_success_timestamp 0"),
        "проверки ещё не было"
    );
}

/// S3 по адресу, где никто не слушает; без повторов, чтобы отказ был сразу.
fn unreachable_storage() -> Blobs {
    let store = AmazonS3Builder::new()
        .with_endpoint("http://127.0.0.1:9")
        .with_region("garage")
        .with_bucket_name("attachments")
        .with_access_key_id("key")
        .with_secret_access_key("secret")
        .with_allow_http(true)
        .with_retry(RetryConfig { max_retries: 0, ..RetryConfig::default() })
        .build()
        .unwrap();
    Arc::new(store)
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn ready_when_database_and_storage_answer(db: PgPool) {
    let state = testing::state(db);
    let client = Client::new(&state);
    assert_eq!(client.get_raw("/health/ready").await, StatusCode::OK, "всё доступно");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn database_down_fails_ready_but_not_live(db: PgPool) {
    let state = testing::state(db);
    state.db.close().await;
    let client = Client::new(&state);
    assert_eq!(
        client.get_raw("/health/ready").await,
        StatusCode::SERVICE_UNAVAILABLE,
        "без базы — не готов"
    );
    assert_eq!(client.get_raw("/health/live").await, StatusCode::OK, "но жив");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn storage_down_fails_ready_but_not_live(db: PgPool) {
    let mut state = testing::state(db);
    state.blobs = unreachable_storage();
    let client = Client::new(&state);
    assert_eq!(
        client.get_raw("/health/ready").await,
        StatusCode::SERVICE_UNAVAILABLE,
        "без хранилища — не готов"
    );
    assert_eq!(client.get_raw("/health/live").await, StatusCode::OK, "но жив");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn every_response_carries_request_id(db: PgPool) {
    let state = testing::state(db);
    let mut client = Client::new(&state);
    let generated = client.raw(Method::GET, "/health/live").await;
    let request_id = generated.headers().get("x-request-id").and_then(|id| id.to_str().ok());
    assert!(
        request_id.is_some_and(|id| id.parse::<uuid::Uuid>().is_ok()),
        "сгенерирован UUID: {request_id:?}"
    );
    let api = client.raw(Method::GET, "/api/auth/me").await;
    assert!(api.headers().contains_key("x-request-id"), "и на ответах API");
}

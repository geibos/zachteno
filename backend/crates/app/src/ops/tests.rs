use axum::extract::State;
use sqlx::PgPool;

use crate::testing;

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

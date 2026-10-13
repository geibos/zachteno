use sqlx::PgPool;

use super::ensure_applied;

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn fully_migrated_schema_passes_check(db: PgPool) {
    ensure_applied(&db).await.unwrap();
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn missing_migration_stops_server_with_hint(db: PgPool) {
    let latest = super::MIGRATOR.iter().map(|migration| migration.version).max().unwrap();
    let _forgotten = sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
        .bind(latest)
        .execute(&db)
        .await
        .unwrap();
    let error = ensure_applied(&db).await.unwrap_err().to_string();
    assert!(error.contains(&latest.to_string()), "называет миграцию: {error}");
    assert!(error.contains("migrate"), "подсказывает команду: {error}");
}

#[sqlx::test(migrations = false)]
async fn empty_database_is_reported(db: PgPool) {
    let error = ensure_applied(&db).await.unwrap_err().to_string();
    assert!(error.contains("схема базы не создана"), "пустая база: {error}");
}

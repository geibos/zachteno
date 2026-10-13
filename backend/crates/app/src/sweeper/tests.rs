use sqlx::PgPool;

use super::{LOCK_KEY, run_once};
use crate::testing;

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn second_sweeper_skips_while_lock_is_held(db: PgPool) {
    let state = testing::state(db);
    let mut other = state.db.acquire().await.unwrap();
    let held: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(LOCK_KEY)
        .fetch_one(&mut *other)
        .await
        .unwrap();
    assert!(held, "блокировку взял «другой экземпляр»");
    assert!(!run_once(&state).await.unwrap(), "второй свипер не работает");
    let _released = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(LOCK_KEY)
        .execute(&mut *other)
        .await
        .unwrap();
    assert!(run_once(&state).await.unwrap(), "после освобождения — работает");
}

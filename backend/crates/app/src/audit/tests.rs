use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::testing::{self, Classroom};

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn methodist_sees_only_own_course_log(db: PgPool) {
    let state = testing::state(db);
    let mut room = Classroom::new(&state, 1, json!({})).await;
    let (_, own) = room.methodist.get(&format!("/api/audit?course_id={}", room.course_id)).await;
    let actions: Vec<&str> =
        own.as_array().unwrap().iter().filter_map(|record| record["action"].as_str()).collect();
    assert!(actions.contains(&"test.publish"), "публикация в журнале курса: {actions:?}");
    assert!(
        own.as_array().unwrap().iter().all(|record| record["course_id"] == room.course_id.as_str()),
        "только свой курс"
    );
    assert_eq!(
        room.methodist.get("/api/audit").await.0,
        StatusCode::FORBIDDEN,
        "весь журнал — только суперпользователю"
    );
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn audit_log_is_append_only_for_app_role(db: PgPool) {
    let state = testing::state(db);
    let _user = testing::register(&state, "a@example.com").await;
    let mut tx = state.db.begin().await.unwrap();
    let _role = sqlx::query("SET LOCAL ROLE zachteno_app").execute(&mut *tx).await.unwrap();
    let denied = sqlx::query("UPDATE audit_log SET action = 'x'").execute(&mut *tx).await;
    assert!(
        denied.is_err_and(|error| error.to_string().contains("permission denied")),
        "UPDATE запрещён роли приложения"
    );
}

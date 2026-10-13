use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::testing::{self, Client, PASSWORD};

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn superuser_grants_methodist_and_it_is_audited(db: PgPool) {
    let state = testing::state(db);
    let (mut admin, admin_id) = testing::register(&state, "admin@example.com").await;
    testing::grant(&state, admin_id, "superuser").await;
    let (mut target, target_id) = testing::register(&state, "metod@example.com").await;
    let path = format!("/api/admin/users/{target_id}/roles");
    assert_eq!(admin.post(&path, json!({ "role": "methodist", "granted": true })).await.0, StatusCode::NO_CONTENT, "выдача");
    let (_, me) = target.get("/api/auth/me").await;
    assert_eq!(me["user"]["roles"]["methodist"], true, "права со следующего запроса");
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'role.grant' AND object_id = $1")
        .bind(target_id).fetch_one(&state.db).await.unwrap();
    assert_eq!(audited, 1, "в аудит-логе");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn last_superuser_cannot_drop_own_role(db: PgPool) {
    let state = testing::state(db);
    let (mut admin, admin_id) = testing::register(&state, "admin@example.com").await;
    testing::grant(&state, admin_id, "superuser").await;
    let path = format!("/api/admin/users/{admin_id}/roles");
    let (status, _) = admin.post(&path, json!({ "role": "superuser", "granted": false })).await;
    assert_eq!(status, StatusCode::CONFLICT, "последний суперпользователь");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn blocking_ends_sessions_and_login(db: PgPool) {
    let state = testing::state(db);
    let (mut admin, admin_id) = testing::register(&state, "admin@example.com").await;
    testing::grant(&state, admin_id, "superuser").await;
    let (mut victim, victim_id) = testing::register(&state, "student@example.com").await;
    let path = format!("/api/admin/users/{victim_id}/block");
    assert_eq!(admin.post(&path, json!({ "blocked": true })).await.0, StatusCode::NO_CONTENT, "блокировка");
    assert!(victim.get("/api/auth/me").await.1["user"].is_null(), "сессии завершены");
    let login = json!({ "email": "student@example.com", "password": PASSWORD });
    assert_eq!(Client::new(&state).post("/api/auth/login", login).await.0, StatusCode::FORBIDDEN, "вход запрещён");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn regular_user_has_no_admin_access(db: PgPool) {
    let state = testing::state(db);
    let (mut user, _user_id) = testing::register(&state, "student@example.com").await;
    assert_eq!(user.get("/api/admin/users").await.0, StatusCode::FORBIDDEN, "нет прав");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn registration_by_trusted_link_marks_account_and_logs_use(db: PgPool) {
    let state = testing::state(db);
    let (mut admin, admin_id) = testing::register(&state, "admin@example.com").await;
    testing::grant(&state, admin_id, "superuser").await;
    let (status, link) = admin.post("/api/admin/trusted-links", json!({ "label": "методисты" })).await;
    assert_eq!(status, StatusCode::CREATED, "ссылка создана");
    let token = link["url"].as_str().unwrap().split("trusted=").nth(1).unwrap().to_owned();
    let mut visitor = Client::new(&state);
    let body = json!({ "email": "new@example.com", "display_name": "Новый", "password": PASSWORD, "trusted_token": token });
    let (status, user) = visitor.post("/api/auth/register", body).await;
    assert_eq!(status, StatusCode::CREATED, "регистрация: {user}");
    assert_eq!(user["trusted_registration"], true, "отметка доверенной регистрации");
    let link_id = link["id"].as_str().unwrap();
    let (_, uses) = admin.get(&format!("/api/admin/trusted-links/{link_id}/uses")).await;
    assert_eq!(uses.as_array().map(Vec::len), Some(1), "использование в журнале");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn bootstrap_creates_superuser_with_password_link(db: PgPool) {
    let state = testing::state(db);
    let link = crate::admin::bootstrap_superuser(&state, "root@example.com", "Админ").await.unwrap().unwrap();
    let token = link.split("token=").nth(1).unwrap();
    let mut visitor = Client::new(&state);
    let body = json!({ "token": token, "password": "пароль суперпользователя 1" });
    assert_eq!(visitor.post("/api/auth/password/reset", body).await.0, StatusCode::NO_CONTENT, "пароль задан");
    let login = json!({ "email": "root@example.com", "password": "пароль суперпользователя 1" });
    let (status, user) = visitor.post("/api/auth/login", login).await;
    assert_eq!(status, StatusCode::OK, "вход");
    assert_eq!(user["roles"]["superuser"], true, "роль суперпользователя");
}

#[sqlx::test(migrator = "crate::database::MIGRATOR")]
async fn bootstrap_does_not_hand_role_to_squatter(db: PgPool) {
    let state = testing::state(db);
    let (mut squatter, _id) = testing::register(&state, "root@example.com").await;
    let link = crate::admin::bootstrap_superuser(&state, "root@example.com", "Админ").await.unwrap();
    assert!(link.is_some(), "неподтверждённая учётка получает только ссылку на новый пароль");
    assert!(squatter.get("/api/auth/me").await.1["user"].is_null(), "сессии захватчика завершены");
    let login = json!({ "email": "root@example.com", "password": PASSWORD });
    assert_eq!(Client::new(&state).post("/api/auth/login", login).await.0, StatusCode::UNAUTHORIZED, "старый пароль сброшен");
}

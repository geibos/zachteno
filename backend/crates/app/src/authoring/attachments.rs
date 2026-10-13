//! Картинки к вопросам и вариантам: загрузка и раздача с проверкой прав (spec course-authoring, access-control).

use axum::Json;
use axum::body::Body;
use std::time::Instant;

use axum::extract::multipart::MultipartError;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use serde::Serialize;
use sqlx::FromRow;
use tokio::task::spawn_blocking;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access;
use crate::auth::session::CurrentUser;
use crate::blobs::{self, Processed};
use crate::error::{ApiResult, AppError};
use crate::state::AppState;

/// Загруженная картинка.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct Uploaded {
    pub id: Uuid,
    pub url: String,
    pub width: u32,
    pub height: u32,
}

/// Загружает картинку (поле `file`, до 10 МБ): проверка формата, перекодирование без метаданных.
#[utoipa::path(post, path = "/api/attachments", tag = "tests", request_body(content_type = "multipart/form-data"),
    responses((status = 201, body = Uploaded), (status = 400)))]
pub(crate) async fn upload(
    State(state): State<AppState>,
    user: CurrentUser,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<Uploaded>)> {
    access::require_methodist(&user)?;
    state
        .uploads
        .check(&user.id, Instant::now())
        .map_err(|wait| AppError::TooManyRequests { retry_after_secs: wait.as_secs().max(1) })?;
    let upload = first_file(&mut multipart).await?;
    let processed = process(&state, upload.to_vec()).await?;
    let id = store(&state, &user, processed.clone()).await?;
    Ok((
        StatusCode::CREATED,
        Json(Uploaded {
            id,
            url: format!("/api/files/{id}"),
            width: processed.width,
            height: processed.height,
        }),
    ))
}

/// Читает только первое поле `file`; остальные поля не буферизуются.
async fn first_file(multipart: &mut Multipart) -> ApiResult<bytes::Bytes> {
    let malformed = |error: MultipartError| AppError::BadRequest(error.body_text());
    while let Some(field) = multipart.next_field().await.map_err(malformed)? {
        if field.name() == Some("file") {
            return field.bytes().await.map_err(|error| too_large(&error));
        }
    }
    Err(AppError::BadRequest("Нет файла в поле file".to_owned()))
}

/// Поле `file` не дочитано — обычно сработал лимит размера тела; причина уходит в журнал.
fn too_large(error: &MultipartError) -> AppError {
    tracing::info!(%error, "поле file не дочитано");
    AppError::BadRequest("Файл больше 10 МБ".to_owned())
}

async fn process(state: &AppState, upload: Vec<u8>) -> ApiResult<Processed> {
    let _permit =
        state.image_permits.acquire().await.map_err(|error| AppError::Internal(error.into()))?;
    spawn_blocking(move || blobs::process(&upload))
        .await
        .map_err(|error| AppError::Internal(error.into()))?
        .map_err(|rejected| AppError::BadRequest(rejected.to_string()))
}

async fn store(state: &AppState, user: &CurrentUser, processed: Processed) -> ApiResult<Uuid> {
    let key = format!("img/{}", Uuid::new_v4());
    let size = i64::try_from(processed.bytes.len()).unwrap_or(i64::MAX);
    blobs::put(&state.blobs, &key, processed.bytes).await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO attachments (storage_key, content_type, byte_size, width, height, uploaded_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(&key)
    .bind(processed.content_type)
    .bind(size)
    .bind(i32::try_from(processed.width).unwrap_or(i32::MAX))
    .bind(i32::try_from(processed.height).unwrap_or(i32::MAX))
    .bind(user.id)
    .fetch_one(&state.db)
    .await?)
}

#[derive(FromRow)]
struct StoredFile {
    storage_key: String,
    content_type: String,
}

/// Файл, если пользователь его загрузил, методист или видит тест с ним.
const VISIBLE_FILE: &str = "SELECT a.storage_key, a.content_type FROM attachments a WHERE a.id = $1 AND (
            a.uploaded_by = $2 OR $3 OR EXISTS (
              SELECT 1 FROM questions q JOIN tests t ON t.id = q.test_id
              WHERE (q.image_id = a.id OR EXISTS (SELECT 1 FROM options o WHERE o.question_id = q.id AND o.image_id = a.id))
                AND (EXISTS (SELECT 1 FROM attempts at WHERE at.test_id = t.id AND at.user_id = $2)
                  OR EXISTS (SELECT 1 FROM course_teachers ct WHERE ct.course_id = t.course_id AND ct.user_id = $2))))";

/// Отдаёт картинку тем, кто видит курс с ней или проходит тест, где она используется.
#[utoipa::path(get, path = "/api/files/{attachment_id}", tag = "tests", params(("attachment_id" = Uuid, Path)),
    responses((status = 200, content_type = "image/*"), (status = 404)))]
pub(crate) async fn serve(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(attachment_id): Path<Uuid>,
) -> ApiResult<Response> {
    let file: StoredFile = sqlx::query_as(VISIBLE_FILE)
        .bind(attachment_id)
        .bind(user.id)
        .bind(user.roles.methodist || user.roles.superuser)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let bytes = blobs::get(&state.blobs, &file.storage_key).await?;
    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    let _type = headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.content_type)
            .map_err(|error| AppError::Internal(error.into()))?,
    );
    let _cache =
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
    Ok(response)
}

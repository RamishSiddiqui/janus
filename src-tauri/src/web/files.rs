//! Stored files for the browser (issue #94, phase 3): avatars, portraits, scene
//! images and chat attachments are read through `GET /api/files/{path}`, and
//! images are uploaded through `POST /api/upload/*`.
//!
//! The desktop reads these straight off disk by path, which a browser can't.
//! Reads go through the same `ensure_file_access` check the commands use, so
//! a path that exists but belongs to someone else is "not found", and only
//! media extensions are ever served.

use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::util::{ActiveUser, ApiError};
use super::WebState;
use crate::auth::access;
use crate::error::MythicError;

/// Largest image the browser may upload (a phone photo fits).
pub const MAX_UPLOAD_BYTES: usize = 12 * 1024 * 1024;

fn mime_for(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        _ => return None,
    })
}

fn data_dir(st: &WebState) -> Result<&std::path::Path, ApiError> {
    st.data_dir.as_deref().ok_or_else(|| {
        ApiError(MythicError::NotFound(
            "Files aren't available here.".to_string(),
        ))
    })
}

/// `GET /api/files/{*path}`
pub async fn get_file(
    State(st): State<WebState>,
    ActiveUser(authed): ActiveUser,
    Path(path): Path<String>,
) -> Result<Response, ApiError> {
    let dir = data_dir(&st)?;
    let not_found = || ApiError(MythicError::NotFound(format!("File not found: {path}")));
    let mime = mime_for(&path).ok_or_else(not_found)?;
    let actor = access::resolve_web_actor(&st.db, &authed.user.id).await?;
    access::ensure_file_access(&st.db, &actor, &path).await?;
    let full = crate::error::resolve_within(dir, &path)?;
    let bytes = tokio::fs::read(&full).await.map_err(|_| not_found())?;
    let mut resp = bytes.into_response();
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    // Files are per-account, so they must not be cached by a shared proxy.
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    Ok(resp)
}

/// The `ext` value of a query string such as `ext=png`.
fn ext_param(query: Option<String>) -> String {
    query
        .unwrap_or_default()
        .split('&')
        .find_map(|kv| kv.strip_prefix("ext="))
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// `POST /api/upload/attachment?ext=png`: the raw image bytes as the body.
pub async fn upload_attachment(
    State(st): State<WebState>,
    ActiveUser(_authed): ActiveUser,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Json<crate::models::conversation::MessageAttachment>, ApiError> {
    let dir = data_dir(&st)?;
    if body.is_empty() {
        return Err(ApiError(MythicError::Validation(
            "The image is empty.".to_string(),
        )));
    }
    let ext = ext_param(query);
    let saved = crate::commands::chat::attachments::write_attachment_in(dir, &body, &ext).await?;
    Ok(Json(saved))
}

/// `POST /api/upload/portrait/{character_id}`: the raw image bytes as the body.
pub async fn upload_portrait(
    State(st): State<WebState>,
    ActiveUser(authed): ActiveUser,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<crate::models::character::Character>, ApiError> {
    let dir = data_dir(&st)?;
    if body.is_empty() {
        return Err(ApiError(MythicError::Validation(
            "The image is empty.".to_string(),
        )));
    }
    let actor = access::resolve_web_actor(&st.db, &authed.user.id).await?;
    access::ensure_character(&st.db, &actor, &character_id).await?;
    let updated =
        crate::commands::characters::store_portrait(&st.db, dir, &character_id, &body).await?;
    Ok(Json(updated))
}

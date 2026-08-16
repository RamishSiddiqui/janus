//! Message attachment upload + resolution — copying a user-picked or
//! clipboard-pasted image into `app_data_dir/attachments/`, and resolving a
//! stored message's attachments back into raw bytes for a provider call.

use tauri::Manager;
use tracing::warn;

use crate::error::MythicError;

fn mime_type_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// Writes attachment bytes into `app_data_dir/attachments/{uuid}.{ext}` and
/// returns the resulting `MessageAttachment` — the shared tail end of both
/// `upload_message_attachment` (source is a file on disk) and
/// `upload_message_attachment_bytes` (source is raw clipboard-paste bytes,
/// which have no file/extension to read from).
async fn write_attachment(
    app: &tauri::AppHandle,
    bytes: &[u8],
    ext: &str,
) -> Result<crate::models::conversation::MessageAttachment, MythicError> {
    let mime_type = mime_type_for_extension(ext).ok_or_else(|| {
        MythicError::Validation(format!(
            "Unsupported attachment type '.{}'. Supported: png, jpg, jpeg, webp, gif", ext
        ))
    })?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    let attachments_dir = app_data_dir.join("attachments");
    tokio::fs::create_dir_all(&attachments_dir).await?;
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    tokio::fs::write(attachments_dir.join(&filename), bytes).await?;

    Ok(crate::models::conversation::MessageAttachment {
        relative_path: format!("attachments/{}", filename),
        mime_type: mime_type.to_string(),
    })
}

/// Copies a user-picked image file (an absolute path from the frontend's
/// file dialog) into `app_data_dir/attachments/`, so it can be attached to
/// a chat message and later resolved via `crate::error::resolve_within`.
///
/// Unlike `upload_character_avatar`, this preserves the source file's real
/// extension instead of hardcoding `.png` — the extension is how
/// `mime_type_for_extension` (and, on replay, `load_message_images`) knows
/// what MIME type to hand the provider.
#[tauri::command]
#[specta::specta]
pub async fn upload_message_attachment(
    app: tauri::AppHandle,
    file_path: String,
) -> Result<crate::models::conversation::MessageAttachment, MythicError> {
    let source = std::path::PathBuf::from(&file_path);
    if !source.exists() {
        return Err(MythicError::NotFound(format!("File not found: {}", file_path)));
    }
    let ext = source.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let bytes = tokio::fs::read(&source).await?;
    write_attachment(&app, &bytes, &ext).await
}

/// Same as `upload_message_attachment`, but for an image pasted directly
/// from the clipboard (e.g. a screenshot) — there's no source file on disk
/// to read, just raw bytes and the clipboard blob's MIME type from the
/// frontend's `paste` event.
#[tauri::command]
#[specta::specta]
pub async fn upload_message_attachment_bytes(
    app: tauri::AppHandle,
    bytes: Vec<u8>,
    extension: String,
) -> Result<crate::models::conversation::MessageAttachment, MythicError> {
    if bytes.is_empty() {
        return Err(MythicError::Validation("Pasted image is empty".to_string()));
    }
    write_attachment(&app, &bytes, &extension.to_lowercase()).await
}

/// Resolves a user message's stored attachments (from its `metadata` JSON,
/// see `MessageAttachment`) into raw `(bytes, mime_type)` pairs ready to
/// hand to `RigProvider::generate`/`generate_stream`. Used both for a
/// freshly-sent message (`send_message`) and for replaying an already-
/// stored message's attachments on regenerate/retry (`retry_failed_message`).
///
/// Silently skips any entry that fails to resolve or read — an attachment
/// whose file got cleaned up shouldn't break generation, it should just be
/// dropped from what the model sees.
pub(crate) async fn load_message_images(
    app_data_dir: &std::path::Path,
    metadata: Option<&serde_json::Value>,
) -> Vec<(Vec<u8>, String)> {
    let Some(attachments) = metadata
        .and_then(|m| m.get("attachments"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    let mut images = Vec::new();
    for entry in attachments {
        let (Some(relative_path), Some(mime_type)) = (
            entry.get("relativePath").and_then(|v| v.as_str()),
            entry.get("mimeType").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        match crate::error::resolve_within(app_data_dir, relative_path) {
            Ok(resolved) => match tokio::fs::read(&resolved).await {
                Ok(bytes) => images.push((bytes, mime_type.to_string())),
                Err(e) => warn!("[load_message_images] Failed to read attachment {}: {}", relative_path, e),
            },
            Err(e) => warn!("[load_message_images] Failed to resolve attachment {}: {}", relative_path, e),
        }
    }
    images
}

//! Backend log access for Settings > Logging — reads back the file the
//! dual stdout+file tracing subscriber (see `lib.rs::run`) has been writing
//! to all along, so a packaged build (no visible console) is no longer a
//! black box when something goes wrong.

use tauri::{AppHandle, Manager};

use crate::error::MythicError;

/// Max lines ever returned by [`get_backend_logs`], regardless of the
/// requested `lines` — a runaway request (or a log file that's grown huge
/// over a long session) shouldn't be able to stall the UI or blow up memory
/// reading the whole thing into a string.
const MAX_LINES: usize = 5000;

fn log_file_path(app: &AppHandle) -> Result<std::path::PathBuf, MythicError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data directory: {}", e)))?;
    Ok(app_data_dir.join("logs").join("janus.log"))
}

/// Returns the last `lines` (default 1000, capped at `MAX_LINES`) lines of
/// the persisted backend log file, oldest first within that window. Empty
/// string (not an error) if the file doesn't exist yet — e.g. right after a
/// fresh install before anything's been logged.
#[tauri::command]
#[specta::specta]
pub async fn get_backend_logs(app: AppHandle, lines: Option<u32>) -> Result<String, MythicError> {
    let path = log_file_path(&app)?;
    if !path.exists() {
        return Ok(String::new());
    }

    let requested = lines.unwrap_or(1000) as usize;
    let take = requested.min(MAX_LINES);

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| MythicError::Config(format!("Failed to read log file: {}", e)))?;

    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(take);
    Ok(all_lines[start..].join("\n"))
}

/// Absolute path to the backend log file — shown in the Logging tab so the
/// user can locate it directly (e.g. to attach to a bug report) without
/// going through Export.
#[tauri::command]
#[specta::specta]
pub async fn get_backend_log_path(app: AppHandle) -> Result<String, MythicError> {
    Ok(log_file_path(&app)?.to_string_lossy().to_string())
}

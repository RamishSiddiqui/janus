//! Backend log access for Settings > Logging — reads back the file the
//! dual stdout+file tracing subscriber (see `lib.rs::run`) has been writing
//! to all along, so a packaged build (no visible console) is no longer a
//! black box when something goes wrong.

use std::io::SeekFrom;

use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::error::MythicError;

/// Max lines ever returned by [`get_backend_logs`], regardless of the
/// requested `lines` — a runaway request (or a log file that's grown huge
/// over a long session) shouldn't be able to stall the UI or blow up memory
/// reading the whole thing into a string.
const MAX_LINES: usize = 5000;

/// Starting (and doubling) read window for [`get_backend_logs_page`].
const PAGE_CHUNK_BYTES: u64 = 64 * 1024;
/// Hard ceiling on a single page's read window — protects against a log
/// file with pathologically long lines forcing an unbounded read.
const PAGE_MAX_CHUNK_BYTES: u64 = 4 * 1024 * 1024;

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

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LogPage {
    /// Lines within this page, oldest first — same ordering as
    /// [`get_backend_logs`].
    pub lines: Vec<String>,
    /// Byte offset to pass back as `cursor` to fetch the page immediately
    /// before this one (older lines). `None` once the start of the file has
    /// been reached — there is nothing older to load. `u32` (not `u64`,
    /// which specta's TypeScript export forbids as a bigint-precision risk)
    /// caps a single log file at ~4GB, far beyond anything this feature
    /// needs to handle.
    pub next_cursor: Option<u32>,
}

/// Returns one page of backend log lines read backward from `cursor` (a byte
/// offset into the log file; omit to start at the end — the newest lines).
/// Used by the Logging tab's viewer to load the file incrementally as the
/// user scrolls up, rather than reading it in full up front: only a bounded
/// window at the tail is read per call, growing just enough to cover the
/// requested `limit` lines.
#[tauri::command]
#[specta::specta]
pub async fn get_backend_logs_page(
    app: AppHandle,
    cursor: Option<u32>,
    limit: Option<u32>,
) -> Result<LogPage, MythicError> {
    let path = log_file_path(&app)?;
    let mut file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(_) => {
            return Ok(LogPage {
                lines: vec![],
                next_cursor: None,
            })
        }
    };
    let file_len = file
        .metadata()
        .await
        .map_err(|e| MythicError::Config(format!("Failed to stat log file: {e}")))?
        .len();

    let end = cursor.map(|c| c as u64).unwrap_or(file_len).min(file_len);
    if end == 0 {
        return Ok(LogPage {
            lines: vec![],
            next_cursor: None,
        });
    }

    let limit = (limit.unwrap_or(200) as usize).clamp(1, 1000);

    let mut window: u64 = PAGE_CHUNK_BYTES.min(end);
    let (start, buf) = loop {
        let new_start = end - window;
        file.seek(SeekFrom::Start(new_start))
            .await
            .map_err(|e| MythicError::Config(format!("Failed to seek log file: {e}")))?;
        let mut chunk = vec![0u8; window as usize];
        file.read_exact(&mut chunk)
            .await
            .map_err(|e| MythicError::Config(format!("Failed to read log file: {e}")))?;
        let newline_count = chunk.iter().filter(|&&b| b == b'\n').count();
        if newline_count > limit || new_start == 0 || window >= PAGE_MAX_CHUNK_BYTES {
            break (new_start, chunk);
        }
        window = (window * 2).min(end).min(PAGE_MAX_CHUNK_BYTES);
    };

    let text = String::from_utf8_lossy(&buf);
    let mut parts: Vec<&str> = text.split('\n').collect();

    // The window's first element is a partial line unless we've read all
    // the way back to byte 0 of the file — drop it (the previous page, or a
    // future call with a smaller cursor, owns the bytes before it).
    let mut prefix_bytes = 0usize;
    if start > 0 {
        if let Some(first) = parts.first().copied() {
            prefix_bytes = first.len() + 1; // +1 for the '\n' split on
            parts.remove(0);
        }
    }
    // A trailing '\n' right at `end` (only possible when `end == file_len`)
    // produces one trailing empty element — drop it, it's not a line.
    if parts.last().map(|s| s.is_empty()).unwrap_or(false) {
        parts.pop();
    }

    let take_from = parts.len().saturating_sub(limit);
    let lines: Vec<String> = parts[take_from..].iter().map(|s| s.to_string()).collect();

    let consumed_bytes: usize = parts[..take_from].iter().map(|s| s.len() + 1).sum();
    let next_start = start + prefix_bytes as u64 + consumed_bytes as u64;
    let next_cursor = if next_start > 0 {
        u32::try_from(next_start).ok()
    } else {
        None
    };

    Ok(LogPage { lines, next_cursor })
}

/// Absolute path to the backend log file — shown in the Logging tab so the
/// user can locate it directly (e.g. to attach to a bug report) without
/// going through Export.
#[tauri::command]
#[specta::specta]
pub async fn get_backend_log_path(app: AppHandle) -> Result<String, MythicError> {
    Ok(log_file_path(&app)?.to_string_lossy().to_string())
}

//! Settings > Data — manual export/import of the entire datastore, backed by
//! `db::backup`. See that module's doc comment for why this exists (it's
//! both a real user-facing safety feature and the migration path for future
//! `surrealdb` major-version bumps).

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;

use crate::db::backup;
use crate::error::MythicError;
use crate::AppState;

fn app_data_dir(app: &AppHandle) -> Result<std::path::PathBuf, MythicError> {
    app.path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data directory: {}", e)))
}

/// Exports the entire datastore to a new timestamped file under
/// `<app_data_dir>/backups/` and returns its absolute path, so the frontend
/// can show it to the user (and offer to reveal it in the file manager).
#[tauri::command]
#[specta::specta]
pub async fn export_data_backup(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<String, MythicError> {
    let data_dir = app_data_dir(&app)?;
    let dest = data_dir
        .join(backup::BACKUP_DIR_NAME)
        .join(backup::timestamped_backup_filename());

    let state = state.read().await;
    backup::export_to_file(&state.db, &dest).await?;
    drop(state);

    backup::prune_old_manual_backups(&data_dir).await?;

    Ok(dest.to_string_lossy().to_string())
}

/// Imports a `.surql` backup file (produced by [`export_data_backup`] or the
/// automatic rolling backup) into the current datastore. Existing records
/// with the same ID are left as-is by SurrealDB's import (it replays
/// `CREATE`/`UPSERT`-shaped statements, which don't overwrite silently) —
/// this is meant for restoring into a fresh/empty datastore, not merging
/// into one with unrelated existing data.
#[tauri::command]
#[specta::specta]
pub async fn import_data_backup(
    state: State<'_, Arc<RwLock<AppState>>>,
    file_path: String,
) -> Result<(), MythicError> {
    let source = std::path::PathBuf::from(&file_path);
    let state = state.read().await;
    backup::import_from_file(&state.db, &source).await
}

/// Lists available backup files (both manual exports and the automatic
/// rolling backup) under `<app_data_dir>/backups/`, newest first.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupFileInfo {
    pub path: String,
    pub filename: String,
    pub is_auto: bool,
    /// RFC 3339 timestamp of the file's last modification — a formatted
    /// string, not a raw epoch integer, since specta forbids exporting
    /// BigInt-style types (i64/u64) to TypeScript (precision-loss risk).
    pub modified_at: String,
    /// File size in bytes, as a string for the same BigInt-export reason —
    /// a backup file can plausibly exceed u32's ~4GB ceiling.
    pub size_bytes: String,
}

#[tauri::command]
#[specta::specta]
pub async fn list_data_backups(app: AppHandle) -> Result<Vec<BackupFileInfo>, MythicError> {
    let dir = app_data_dir(&app)?.join(backup::BACKUP_DIR_NAME);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut entries = tokio::fs::read_dir(&dir).await?;
    let mut backups = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("surql") {
            continue;
        }
        let filename = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata().await?;
        let modified_sort_key = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let modified_at = meta
            .modified()
            .ok()
            .map(chrono::DateTime::<chrono::Utc>::from)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        backups.push((
            modified_sort_key,
            BackupFileInfo {
                path: path.to_string_lossy().to_string(),
                is_auto: filename == "janus_auto_backup.surql",
                filename,
                modified_at,
                size_bytes: meta.len().to_string(),
            },
        ));
    }

    backups.sort_by_key(|(sort_key, _)| std::cmp::Reverse(*sort_key));
    Ok(backups.into_iter().map(|(_, info)| info).collect())
}

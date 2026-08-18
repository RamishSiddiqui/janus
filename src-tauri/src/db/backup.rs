//! Portable, version-agnostic data backup/restore.
//!
//! Walks the entire datastore via SurrealDB's own native export/import
//! (`.surql` script format) rather than a hand-rolled per-table dump — this
//! is the exact mechanism the `surreal export`/`surreal import` CLI commands
//! use, just called directly from the embedded Rust SDK with no external
//! binary required.
//!
//! Two purposes:
//! 1. A real user-facing data-safety feature (Settings > Data > Export/Import) —
//!    valuable on its own, independent of anything else below.
//! 2. The migration path for future `surrealdb` major-version bumps. A newer
//!    storage engine cannot open an older on-disk RocksDB store directly (key
//!    encoding changes across majors), so an in-place crate bump risks the
//!    app being unable to read a user's existing data at all. The fix:
//!    export to this portable `.surql` format *before* the bump (using the
//!    old engine, which can still read the old store), then import into a
//!    freshly-initialized datastore on the new engine after.
//!
//! The export uses SurrealDB's v3-compatibility mode (`.v3(true)`), which
//! translates renamed/removed syntax during export (e.g. `SEARCH ANALYZER`
//! -> `FULLTEXT ANALYZER`, `MTREE` -> `HNSW`) — the same translation the
//! `surreal v2 export --v3` CLI flag performs. This keeps one backup file
//! importable both into a same-version datastore (routine backup/restore)
//! and into a future major-version datastore (migration), without needing
//! two separate export paths.

use std::path::{Path, PathBuf};

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::info;

use crate::error::MythicError;

/// Subdirectory (under app_data_dir) where backups are written.
pub const BACKUP_DIR_NAME: &str = "backups";

/// Filename for the automatic rolling backup, rewritten on every startup —
/// distinct from user-triggered manual exports, which get a timestamp in
/// their name so multiple can coexist.
const AUTO_BACKUP_FILENAME: &str = "janus_auto_backup.surql";

/// Number of dated manual-export files to keep before pruning the oldest —
/// applies only to files matching the manual-export naming pattern
/// (`janus_backup_*.surql`), never to `AUTO_BACKUP_FILENAME`.
const MAX_MANUAL_BACKUPS: usize = 10;

/// Exports the entire datastore to a single `.surql` file at `dest`,
/// creating parent directories as needed.
pub async fn export_to_file(db: &Surreal<Db>, dest: &Path) -> Result<(), MythicError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    db.export(dest)
        .with_config()
        .v3(true)
        .await
        .map_err(|e| MythicError::DatabaseOp(format!("export failed: {e}")))?;
    info!("[backup] Exported datastore to {:?}", dest);
    Ok(())
}

/// Imports a `.surql` backup file into `db`. Safe to call against a
/// datastore that already has its schema defined (`schema::define_schema`'s
/// `IF NOT EXISTS` definitions no-op against matching ones the import
/// brings) or against a freshly-created empty one (the import's own
/// `DEFINE`/`CREATE` statements establish everything from scratch).
pub async fn import_from_file(db: &Surreal<Db>, src: &Path) -> Result<(), MythicError> {
    if !src.exists() {
        return Err(MythicError::NotFound(format!(
            "Backup file not found: {}",
            src.display()
        )));
    }
    db.import(src)
        .await
        .map_err(|e| MythicError::DatabaseOp(format!("import failed: {e}")))?;
    info!("[backup] Imported datastore from {:?}", src);
    Ok(())
}

/// Path to the rolling automatic backup file.
pub fn auto_backup_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir
        .join(BACKUP_DIR_NAME)
        .join(AUTO_BACKUP_FILENAME)
}

/// Writes (overwrites) the rolling automatic backup. Called once per app
/// startup, after the schema/migrations/seed sequence completes — cheap
/// insurance so a recent backup exists on disk without any user action,
/// independent of whether they've ever used the manual Export button.
///
/// Failure here is logged, not propagated — an auto-backup that couldn't be
/// written (e.g. disk full) must never block the app from starting.
pub async fn run_auto_backup(db: &Surreal<Db>, app_data_dir: &Path) {
    let dest = auto_backup_path(app_data_dir);
    if let Err(e) = export_to_file(db, &dest).await {
        tracing::warn!("[backup] Auto-backup failed (non-fatal): {}", e);
    }
}

/// Builds a timestamped filename for a manual export, e.g.
/// `janus_backup_2026-08-18_143022.surql`.
pub fn timestamped_backup_filename() -> String {
    let now = chrono::Local::now();
    format!("janus_backup_{}.surql", now.format("%Y-%m-%d_%H%M%S"))
}

/// Deletes the oldest manual-export files beyond [`MAX_MANUAL_BACKUPS`],
/// keeping the most recent ones. Never touches [`AUTO_BACKUP_FILENAME`].
pub async fn prune_old_manual_backups(app_data_dir: &Path) -> Result<(), MythicError> {
    let dir = app_data_dir.join(BACKUP_DIR_NAME);
    if !dir.exists() {
        return Ok(());
    }

    let mut entries = tokio::fs::read_dir(&dir).await?;
    let mut manual_backups: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("janus_backup_") && name.ends_with(".surql") {
            if let Ok(meta) = entry.metadata().await {
                if let Ok(modified) = meta.modified() {
                    manual_backups.push((modified, path));
                }
            }
        }
    }

    if manual_backups.len() <= MAX_MANUAL_BACKUPS {
        return Ok(());
    }

    manual_backups.sort_by_key(|(modified, _)| *modified);
    let excess = manual_backups.len() - MAX_MANUAL_BACKUPS;
    for (_, path) in manual_backups.into_iter().take(excess) {
        if let Err(e) = tokio::fs::remove_file(&path).await {
            tracing::warn!("[backup] Failed to prune old backup {:?}: {}", path, e);
        }
    }

    Ok(())
}

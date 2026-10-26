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
//! Note: the `.v3(true)` v3-compatibility export mode mentioned in earlier
//! design notes only exists on surrealdb 2.x's SDK (it's the transitional
//! flag for exporting *out of* a 2.x datastore in a 3.x-importable format,
//! matching the `surreal v2 export --v3` CLI flag). Now that this app itself
//! runs on surrealdb 3.x, `.export()` is already native 3.x format with no
//! compatibility toggle needed or available.

use std::path::{Path, PathBuf};

use surrealdb::engine::local::{Db, Mem};
use surrealdb::opt::capabilities::Capabilities;
use surrealdb::opt::Config;
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
        .await
        .map_err(|e| MythicError::DatabaseOp(format!("export failed: {e}")))?;
    info!("[backup] Exported datastore to {:?}", dest);
    Ok(())
}

/// Rows copied per statement when restoring.
const COPY_BATCH: usize = 200;

/// How long the throwaway store may spend on any one statement of the file.
const SANDBOX_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// A throwaway in-memory store with every capability switched off (no
/// functions, no network, no scripting), so nothing in a backup file can reach
/// beyond it.
async fn open_sandbox() -> Result<Surreal<Db>, MythicError> {
    let config = Config::new()
        .capabilities(Capabilities::none())
        .query_timeout(Some(SANDBOX_QUERY_TIMEOUT));
    let sandbox = Surreal::new::<Mem>(config)
        .await
        .map_err(|e| MythicError::DatabaseOp(format!("could not open the import sandbox: {e}")))?;
    sandbox
        .use_ns("backup")
        .use_db("backup")
        .await
        .map_err(|e| MythicError::DatabaseOp(format!("could not open the import sandbox: {e}")))?;
    Ok(sandbox)
}

/// The app's own tables, read from the live store's schema: `(name, is a
/// relation table)`. A backup can only fill these, whatever else it names.
async fn app_tables(db: &Surreal<Db>) -> Result<Vec<(String, bool)>, MythicError> {
    let mut result = db.query("INFO FOR DB").await?;
    let info: Option<surrealdb::types::Value> = result.take(0)?;
    let info: Option<serde_json::Value> = crate::db::value_bridge::from_value_opt(info)?;
    let tables = info
        .as_ref()
        .and_then(|i| i.get("tables"))
        .and_then(|t| t.as_object())
        .ok_or_else(|| MythicError::DatabaseOp("could not read the table list".to_string()))?;
    Ok(tables
        .iter()
        // Names go into query text below, so only plain ones are used.
        .filter(|(name, _)| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .map(|(name, def)| {
            let relation = def.as_str().is_some_and(|d| d.contains("TYPE RELATION"));
            (name.clone(), relation)
        })
        .collect())
}

/// Copies one table's rows from the sandbox into the live store as values, not
/// as query text. Rows whose id already exists are left as they are.
async fn copy_table(
    sandbox: &Surreal<Db>,
    db: &Surreal<Db>,
    table: &str,
    relation: bool,
) -> Result<usize, MythicError> {
    let insert = if relation {
        format!("INSERT RELATION IGNORE INTO {table} $rows")
    } else {
        format!("INSERT IGNORE INTO {table} $rows")
    };
    let select = format!("SELECT * FROM {table} LIMIT {COPY_BATCH} START $start");
    let mut copied = 0usize;
    loop {
        let mut page = sandbox.query(&select).bind(("start", copied)).await?;
        let rows: Vec<surrealdb::types::Value> = page.take(0)?;
        if rows.is_empty() {
            return Ok(copied);
        }
        let n = rows.len();
        db.query(&insert)
            .bind(("rows", rows))
            .await?
            .check()
            .map_err(|e| MythicError::DatabaseOp(format!("restoring {table} failed: {e}")))?;
        copied += n;
        if n < COPY_BATCH {
            return Ok(copied);
        }
    }
}

/// Restores a `.surql` backup file into `db`.
///
/// The file is never run against the live store. It is loaded into a
/// throwaway in-memory store with every capability off, and only the rows of
/// the app's own tables are then copied across, as plain values. So a
/// hand-edited or malicious file can't run its own statements against your
/// data, can't define anything in the live schema (the live schema from
/// `schema::define_schema` stays the only one, as before), and a message
/// whose text happens to contain a line starting with `DEFINE` is kept intact.
/// Rows whose id already exists are left as they are, so this is meant for
/// restoring into a fresh store, not for merging.
pub async fn import_from_file(db: &Surreal<Db>, src: &Path) -> Result<(), MythicError> {
    if !src.exists() {
        return Err(MythicError::NotFound(format!(
            "Backup file not found: {}",
            src.display()
        )));
    }

    let sandbox = open_sandbox().await?;
    sandbox
        .import(src)
        .await
        .map_err(|e| MythicError::DatabaseOp(format!("import failed: {e}")))?;

    let mut total = 0usize;
    for (table, relation) in app_tables(db).await? {
        total += copy_table(&sandbox, db, &table, relation).await?;
    }
    info!("[backup] Imported {total} rows from {:?}", src);
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

use sqlx::{Pool, Sqlite, sqlite::SqlitePoolOptions};
use std::path::Path;
use tracing::info;

use crate::error::MythicError;

/// Initializes the SQLite database connection pool and runs migrations.
///
/// The database file is created in the Tauri app data directory,
/// ensuring platform-compliant storage on all operating systems.
pub async fn init_database(db_path: &Path) -> Result<Pool<Sqlite>, MythicError> {
    // Ensure the parent directory exists
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    info!("Initializing database at: {}", db_url);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    // Enable WAL mode for better concurrent read performance
    sqlx::query("PRAGMA journal_mode=WAL;")
        .execute(&pool)
        .await?;

    // Enable foreign keys
    sqlx::query("PRAGMA foreign_keys=ON;")
        .execute(&pool)
        .await?;

    // Run embedded migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await?;

    info!("Database initialized successfully");
    Ok(pool)
}

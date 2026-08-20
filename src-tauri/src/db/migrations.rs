//! Versioned migrations for schema changes that plain idempotent DDL can't
//! express safely — renaming or dropping a field, changing a field's type,
//! or backfilling existing rows when a field's meaning changes.
//!
//! `schema.rs`'s `define_schema()` remains the right place for purely
//! additive changes (`DEFINE TABLE/FIELD IF NOT EXISTS` is naturally
//! idempotent and safe to re-run on every boot). This module exists for the
//! changes that *aren't* safe to express that way — anything that must run
//! exactly once, in order, against data that may already exist.
//!
//! To add a migration: append a new `Migration` to `MIGRATIONS` with the
//! next version number. Never edit or reorder an already-shipped migration —
//! once a version has run on any installed database, its behavior is frozen;
//! ship a new migration to adjust it instead.

use futures::future::BoxFuture;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::info;

use crate::error::MythicError;

type MigrationRun = for<'a> fn(&'a Surreal<Db>) -> BoxFuture<'a, Result<(), MythicError>>;

struct Migration {
    version: i64,
    description: &'static str,
    run: MigrationRun,
}

/// Registered migrations, in ascending version order. Empty today — nothing
/// shipped so far has needed anything beyond additive DDL. Example shape for
/// the next one that does:
///
/// ```ignore
/// Migration {
///     version: 1,
///     description: "rename memories.foo to memories.bar",
///     run: |db| async move {
///         db.query("UPDATE memories SET bar = foo").await?;
///         db.query("REMOVE FIELD foo ON memories").await?;
///         Ok(())
///     }.boxed(),
/// },
/// ```
const MIGRATIONS: &[Migration] = &[];

/// Runs any migrations not yet recorded as applied, in version order.
/// Safe to call on every startup — already-applied migrations are skipped.
pub async fn run_pending(db: &Surreal<Db>) -> Result<(), MythicError> {
    db.query(
        "
        DEFINE TABLE IF NOT EXISTS _migrations SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS version     ON _migrations TYPE int;
        DEFINE FIELD IF NOT EXISTS description ON _migrations TYPE string;
        DEFINE FIELD IF NOT EXISTS applied_at  ON _migrations TYPE datetime DEFAULT time::now();
        DEFINE INDEX IF NOT EXISTS idx_migrations_version ON _migrations FIELDS version UNIQUE;
        ",
    )
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:_migrations: {}", e)))?;

    let mut result = db.query("SELECT VALUE version FROM _migrations").await?;
    let applied: Vec<i64> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;
    let applied: std::collections::HashSet<i64> = applied.into_iter().collect();

    for m in MIGRATIONS {
        if applied.contains(&m.version) {
            continue;
        }

        info!("  migration {}: {}", m.version, m.description);
        (m.run)(db).await?;

        db.query("CREATE _migrations SET version = $v, description = $d")
            .bind(("v", m.version))
            .bind(("d", m.description.to_string()))
            .await?;
    }

    Ok(())
}

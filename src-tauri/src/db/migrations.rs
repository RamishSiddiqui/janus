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

/// Registered migrations, in ascending version order.
const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "backfill characters.origin/portrait_status/profile_reviewed for rows that predate those fields",
    run: |db| {
        use futures::FutureExt;
        async move {
            // `DEFINE FIELD ... DEFAULT` only applies on INSERT/CREATE — it
            // never backfills rows written before the field existed. Those
            // rows store a genuine NONE for `origin` (a `TYPE string`
            // field), which fails SurrealDB's schema coercion on *any*
            // future UPDATE to the row (not just one touching `origin`
            // itself) — first surfaced by `CharacterRepo::set_voice` erroring
            // on a seeded demo character with "Expected `string` but found
            // `NONE`" for a field the query never even set. Backfill to the
            // same defaults `schema.rs`'s DEFAULT clauses and the Rust
            // model's serde defaults already assume for pre-existing rows.
            //
            // Must set all three fields in ONE statement, not three separate
            // ones — SurrealDB validates a row's *entire* schema on every
            // write, not just the touched fields, so a first statement that
            // fixes only `origin` still leaves `portrait_status`/
            // `profile_reviewed` as NONE, and the very next statement's
            // write to the same row fails the same coercion check all over
            // again (confirmed the hard way: this crashed the app on
            // startup on the first attempt). Each field only overwrites
            // itself when it's actually NONE (`IF x = NONE THEN default ELSE
            // x END`) rather than blanket-setting all three — a row missing
            // only `portrait_status` must keep whatever `origin` it already
            // has (e.g. a real 'npc'), not get reset to 'gallery'.
            db.query(
                "
                UPDATE characters SET
                    origin = IF origin = NONE THEN 'gallery' ELSE origin END,
                    portrait_status = IF portrait_status = NONE THEN 'approved' ELSE portrait_status END,
                    profile_reviewed = IF profile_reviewed = NONE THEN true ELSE profile_reviewed END
                WHERE origin = NONE OR portrait_status = NONE OR profile_reviewed = NONE;
                ",
            )
            .await?
            .check()
            .map_err(|e| MythicError::DatabaseOp(format!("migration 1: {}", e)))?;
            Ok(())
        }
        .boxed()
    },
}];

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

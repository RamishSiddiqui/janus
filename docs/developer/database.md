# Database guide

Janus uses embedded **SurrealDB 3.x** with the RocksDB engine, in `<app data>/mythic_surreal` (namespace `mythic`, database `mythic`). Every table, field and index is listed in the {doc}`/reference/database`.

## Layout of the code

| Path | Role |
|---|---|
| `db/mod.rs` | `init_database`: open, define schema, run migrations, seed defaults. |
| `db/schema.rs` | `define_schema()`: idempotent `DEFINE ... IF NOT EXISTS` statements, run on every start. |
| `db/migrations.rs` | Versioned one-time migrations. |
| `db/*.rs` | One `*Repo` per table holding all queries for it. |
| `db/value_bridge.rs` | Converts between serde structs and SurrealDB values. |
| `db/backup.rs` | Export and import (`.surql`). |
| `db/seed.rs` | Default image preset (always) and demo characters (debug builds only). |

Commands never write queries inline. They call repos.

## Conventions

- **Records are `table:id`** with string keys (a UUID or a fixed slug). `RecordId` is turned into a plain string for the frontend by `models::serialize_thing`, and back with `type::record('table', $id)` in queries.
- **Soft delete:** characters, personas and conversations have `deleted_at`. `trash_*` sets it, `restore_*` clears it, `list_trash` reads it, and `delete_*` removes the row for good. Queries for live data filter `deleted_at = NONE`.
- **Cascades** are SurrealDB `DEFINE EVENT`s (`cascade_character_delete`, `cascade_conversation_delete`, `cascade_memory_delete`, `cascade_image_preset_delete`, `cascade_persona_delete`) that remove dependent rows. `enforce_copy_direction` guards memory links.
- **Full-text** search uses `msg_analyzer` (class tokenizer, lowercase) over message and memory content.
- **Vectors:** `message_embeddings` and memory embeddings store a vector; the HNSW index is created at runtime by `EmbeddingRepo::ensure_vector_index` once the dimension is known.
- **Ownership:** seven root tables carry `owner_id` (see {doc}`accounts`).

## Reading and writing

SurrealDB 3's `.take()` requires its own value trait, so every call site pins its type to `surrealdb::types::Value` and converts with the helpers in `value_bridge.rs`: `from_value`, `from_value_opt`, `from_value_vec` for reads and `to_surreal_value` for writes (a small JSON-to-value mapping). Use them instead of fighting the trait bound.

Wrap writes that can collide in `error::retry_on_conflict`: RocksDB reports retryable transaction conflicts, most often on cascading deletes done in quick succession.

## SurrealDB behaviours that have bitten this project

Read these before changing the schema.

1. **`DEFINE FIELD IF NOT EXISTS` never re-applies.** Changing an existing field's type, assertion or default in `schema.rs` does nothing for existing databases. Use a migration with `DEFINE FIELD OVERWRITE`. (A plain `DEFINE FIELD` errors with "already exists".) Example: migration 2 widened `provider_type`'s assertion to allow `'tts'`.
2. **`DEFAULT` only applies on create.** Rows written before a field existed hold `NONE` in it.
3. **`NONE` in a non-optional field breaks every later write to that row.** SurrealDB validates the whole row on any `UPDATE`, so a row missing *any* required field fails even when the query never touches it. When you add a field to existing tables, either make it `option<...>` or backfill it in a migration, and backfill **every** defaulted field of the row in one statement (see `migrations::backfill_defaults`).
4. **`NULL` is rejected for `option<...>` fields.** Omit the field instead of sending null.
5. **Insert errors are silent unless you check them.** Call `.check()` on query responses whose result you do not read, as a failed seed insert once went unnoticed this way.

## Migrations

Add a `Migration { version, description, run }` to `MIGRATIONS` in `migrations.rs` with the next version. Never edit or reorder a migration that has shipped. They run once, in order, and are recorded in `_migrations` (unique on `version`). They run on every start but skip applied versions. Use a migration for renames, drops, type changes and backfills; use plain `schema.rs` additions for new tables, fields and indexes.

Current migrations are listed at the end of the {doc}`/reference/database`.

A migration that fails prevents startup (the error is logged and the app exits), so test it against a database that contains legacy-shaped rows (`tests/migrations_backfill.rs` shows how).

## Testing against a real database

Integration tests open a real embedded database in a temporary folder with `init_database`. See {doc}`testing`.

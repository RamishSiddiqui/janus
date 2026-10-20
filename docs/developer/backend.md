# Backend guide

The backend is a single Rust crate, `janus` (library name `janus_lib`), in `src-tauri/`.

## Startup

`lib.rs::run()`:

1. In debug builds, export the TypeScript bindings (`src/lib/services/bindings.ts`).
2. Build the Tauri app with the dialog, fs, opener and shell plugins.
3. In `setup`:
   - resolve the application data directory;
   - start the rolling file logger (`logs/janus.log`) and stdout logging. This must happen before the database is opened, since that is the first thing that logs;
   - call `db::init_database` (open RocksDB, define the schema, run migrations, seed defaults);
   - write the rolling automatic backup in a background task;
   - copy bundled demo avatars into the data directory if missing;
   - build one shared `reqwest::Client` (connect timeout only, no overall timeout, because streams can run for minutes);
   - choose the desktop's account (the admin, if any account exists);
   - assemble `AppState`, register it as managed state, start the event router (`events::start`), and bring browser access back up if it was left on.

## `AppState`

One `Arc<RwLock<AppState>>` is managed by Tauri and passed to commands as `State<'_, Arc<RwLock<AppState>>>`.

| Field | Purpose |
|---|---|
| `db` | The embedded SurrealDB handle. |
| `http_client` | Shared HTTP client with connection pooling. |
| `active_generations` | Running chat generations by conversation id, so Stop can abort and keep partial text. |
| `active_scene_generations` | Cancel flags for image/video jobs. |
| `tts_engine` | The lazily loaded Kokoro engine. |
| `resource_monitor` | A long-lived `sysinfo::System` (CPU is a delta between refreshes). |
| `desktop_user` | The account the desktop window acts as. |
| `desktop_admin` | The admin's id while the desktop acts as someone else. |
| `web_server` | The running browser server, if any. |
| `event_bus` | Broadcast channel feeding browser event streams. |

## Commands

A command is a function with `#[tauri::command]` and `#[specta::specta]`:

```rust
/// One sentence describing what it does. This becomes the reference entry.
#[tauri::command]
#[specta::specta]
pub async fn rename_thing(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    name: String,
) -> Result<Thing, MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_thing(&db, &actor, &id).await?;
    validate_required_string("name", &name, 200)?;
    ThingRepo::rename(&db, &id, &name).await
}
```

Rules the codebase follows:

- **Commands are thin.** Validate, check access, call a repo or helper, shape the result. Non-trivial logic belongs in a repo or a module such as `context/`.
- **Start with `acting()`.** It returns the database and the `Actor` (see {doc}`accounts`). Then filter by `actor.owner()` or call an `ensure_*` check on every id from the client.
- **Document every command** with a `///` comment. The command reference is generated from it, and the docs check fails if a command has none (see {doc}`docs`).
- **Register it twice in `lib.rs`:** once in `specta_builder()`'s `collect_commands![...]` (so a TypeScript binding is generated) and once in `tauri::generate_handler![...]` (so Tauri dispatches it). Forgetting one compiles but breaks that side silently. The reference generator checks that both lists match.
- **Return `MythicError`.** Pick the variant that matches the failure.
- **Arguments** arrive as camelCase JSON (`file_path` is `filePath`).
- **Emit events with `emit_scoped`**, not `app.emit` (see {doc}`events`).

To make a command callable from a browser, add it to a family table in `web/rpc_*.rs` (see {doc}`browser-server`).

## Errors

`MythicError` (`error.rs`) is the single error type:

| Variant | Use |
|---|---|
| `Database`, `DatabaseOp` | SurrealDB failures and failed operations with context. |
| `Http`, `Io`, `Serialization`, `Image` | Wrapped library errors. |
| `Provider` | A third-party AI service failed or answered badly. |
| `Validation` | Bad input from the user. |
| `Unauthorized` | Bad credentials, locked account, expired session, or an action the account may not take. |
| `NotFound` | Missing, **or not yours**. Ownership checks return this so they never reveal that someone else's row exists. |
| `Config` | Setup problems (for example a port that cannot be bound). |
| `Cancelled` | The user stopped the work. |

It serializes to `{ error, message }`, which is what the frontend and the browser both receive. Over HTTP, `web::util::ApiError` maps variants to status codes and hides internal details.

Helpers in `error.rs`: `retry_on_conflict` (retries a retryable RocksDB transaction conflict), `validate_string_length`, `validate_required_string`, `resolve_within` (safe path join that rejects `..` and absolute paths), and char-boundary truncation helpers.

## Models

`models/*` holds the structs that cross layers. Rows use `RecordId` serialized to a string for the frontend (`serialize_thing`). Types that cross to TypeScript derive `specta::Type`. Free-form JSON (a character's card data, message metadata) is stored as flexible objects.

## Concurrency and cancellation

- One chat generation per conversation at a time; the frontend refuses to send while one is streaming, and `active_generations` holds the abort handle.
- Streaming runs in a spawned task that consumes provider chunks, so Stop aborts the task and saves whatever text had arrived.
- Image and video jobs poll in the command's own task and check a cancel flag each tick, so they can free the remote worker slot before returning.
- A provider that sends nothing for 90 seconds (`STREAM_EVENT_TIMEOUT`) is treated as stalled.

## Logging

`tracing` writes to stdout and to `logs/janus.log` (never rotated). The default filter is `janus_lib=debug,info`; set `RUST_LOG` to change it. The Settings, Logging tab reads the file back through `get_backend_logs_page`, and only the admin may read it because it can contain every account's prompts.

## Backups

`db::backup` uses SurrealDB's native export and import (`.surql`). `run_auto_backup` writes `backups/janus_auto_backup.surql` at every start. `export_data_backup` and `import_data_backup` do the same on demand (admin only), and the portable format is also the safe path across SurrealDB major versions, since a newer engine cannot read an older RocksDB store in place.

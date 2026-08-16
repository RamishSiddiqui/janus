# Contributing to Janus

Thanks for considering it. This document covers the dev workflow, how the codebase is organized, and the conventions worth keeping intact.

## Before you start

- **Small fixes / bugs** — just open a PR.
- **Anything larger** (a new feature, a new provider adapter, a UI redesign, changing a data model) — open an issue first describing the approach. This saves everyone time if the direction needs adjusting before code gets written.
- By submitting a contribution, you agree it's licensed under the same terms as the rest of the project (see [LICENSE](LICENSE)).

## Dev setup

See the [README's Getting Started section](README.md#getting-started) for prerequisites and how to run the app. In short:

```bash
npm install
npm run tauri dev      # hot-reloading frontend + Rust backend
```

The Rust backend and SvelteKit frontend both hot-reload independently. Backend changes (anything in `src-tauri/`) trigger a full rebuild + app restart; frontend changes (`src/`) hot-swap without losing app state.

## Before opening a PR

Run both of these — CI runs them too, so failures here are failures there:

```bash
npm run check                              # svelte-check: TypeScript + Svelte template errors
cd src-tauri && cargo check                # Rust compile check
cd src-tauri && cargo test                 # Rust unit/integration tests
```

For anything touching a Tauri command's signature (new command, changed params/return type, new `specta::Type` struct), the dev server needs a real restart (not just hot-reload) to regenerate `src/lib/services/bindings.ts` — the TypeScript types are generated from the Rust command signatures, not hand-written. Don't hand-edit `bindings.ts`; it's overwritten on every dev-server start.

## Codebase conventions

These patterns are consistent throughout the codebase — matching them makes a PR much faster to review.

### Backend (`src-tauri/`)

- **Repository-per-table** — each database table has a `Repo` struct in `src-tauri/src/db/` (e.g. `ConversationRepo`, `SceneRepo`) that owns all the SurrealDB queries for that table. Commands (`src-tauri/src/commands/`) call into repos; they don't write raw queries inline.
- **Commands are thin** — a `#[tauri::command]` function validates input, calls one or more repos/provider functions, and shapes the response. Business logic that isn't trivial belongs in a repo or a dedicated module (`src-tauri/src/context/` for the prompt-building pipeline, `src-tauri/src/providers/` for LLM/image/video provider clients), not inline in the command.
- **Every command is registered twice** in `src-tauri/src/lib.rs` — once in `collect_commands!` (for specta's TypeScript export) and once in `generate_handler!` (for Tauri's actual IPC dispatch). Forgetting one compiles fine but breaks the other side silently.
- **Errors** go through `MythicError` (`src-tauri/src/error.rs`) — pick the variant that matches the failure (`Validation` for bad user input, `Provider` for a third-party API/service failure, `NotFound`, etc.) rather than a generic string error. The frontend's error handling relies on being able to tell these apart.
- **New provider adapters** — look at `src-tauri/src/providers/comfyui.rs` (HTTP-based) or `src-tauri/src/providers/wangp.rs` (MCP-based) as templates. Both return `(Vec<u8>, serde_json::Value)` (raw media bytes + metadata JSON) and take a `cancel_flag: &Arc<AtomicBool>` for cooperative cancellation — new adapters should follow the same shape so they plug into the existing single-flight generation guard and Stop-button wiring in `commands/scenes.rs`.

### Frontend (`src/`)

- **IPC wrappers live in `src/lib/services/ipc.ts`** — components never call `invoke()` or the generated `bindings.ts` commands directly. Add a typed wrapper function there, with a doc comment, even for a thin pass-through.
- **State that needs to survive component remounts** (switching conversations, closing/reopening a panel) goes in a Svelte store under `src/lib/stores/`, not local component `$state` — see `src/lib/stores/sceneGeneration.ts` for the pattern (keyed by `conversation_id`, so it's shared across whichever component instance is currently mounted).
- **Design language** — the app uses a "Light Carousel" chip pattern for tabs/filters/mode switches: individual floating pills (not an enclosed segmented-control capsule), full opacity at rest, the active one pops forward with a solid accent fill, `scale(1.05)`, and a colored glow. Look at `.carousel-chip` in `src/routes/settings/+page.svelte` or `.toggle-btn` in `src/lib/components/SceneDisplay.svelte` for the canonical CSS. Don't introduce a different tab/toggle visual style without a reason.
- **Blob URLs, not `asset://`**, for loading local files (images, avatars, generated scenes) into `<img>`/`<video>` — the app's CSP doesn't allow `asset://`. Use `loadFileAsBlobUrl` from `src/lib/utils/blobUrl.ts`, and revoke the URL on unmount/replacement to avoid leaking blob memory.

## Commit messages

Look at `git log` for the house style: `type: short description` (`feat`, `fix`, `refactor`, `docs`, `build`), body explaining the *why* when it's not obvious from the diff, not a restatement of the code change.

## Reporting bugs

Open an issue with: what you did, what you expected, what happened instead, and your OS/platform. If it's provider-specific (a particular LLM/image adapter misbehaving), include which adapter and, if you can get one without exposing a key, the relevant error message from Settings → Logging.

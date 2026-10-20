# Contributing

The short version lives in `CONTRIBUTING.md` at the repository root. This page is the longer working guide.

## Workflow

1. **Raise an issue first** for anything bigger than a trivial fix: describe the bug or the feature and the approach. This saves time if the direction needs adjusting before code is written.
2. Work on a branch named for the change (`feat/...`, `fix/...`, `docs/...`).
3. Keep the change focused. Add or update tests and docs with it.
4. Open a pull request that references the issue. Put the reference in the **commit subject** too (`fix: ... (#123)`), because release notes only show subjects.
5. Wait for every check to pass, then merge (squash).

By contributing you agree your work is licensed under the project's licence (AGPL-3.0-or-later).

## Before you push

```bash
npm run check
npm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
python docs/tools/gen_reference.py --check
```

## Adding a feature: the usual path

1. **Data:** add tables or fields to `db/schema.rs`. If an existing field changes meaning, write a migration ({doc}`database`).
2. **Repo:** put queries in a `db/*.rs` repo. Give owned tables an `owner_id` and filter by it ({doc}`accounts`).
3. **Command:** write a thin, documented `#[tauri::command]` in `commands/`. Register it twice in `lib.rs`.
4. **Browser:** if browsers should call it, add it to a `web/rpc_*.rs` family ({doc}`browser-server`).
5. **Frontend:** restart the dev server to regenerate `bindings.ts`, add a typed wrapper in `ipc.ts`, then build the UI. Put state that must survive remounts in a store.
6. **Docs:** add the user-facing behaviour to the right user guide and anything architectural to these pages. The reference pages update from the code.
7. **Tests:** repo and access tests in `src-tauri/tests`, pure helpers in vitest.

## Conventions

**Backend**

- Repository per table; commands stay thin.
- Errors are `MythicError` variants chosen to match the failure.
- Background work logs and continues; it must never block or fail a user-visible operation.
- New image or video adapters follow `comfyui.rs` or `wangp.rs`: return `(Vec<u8>, serde_json::Value)`, take a cancel flag.
- Emit events with `emit_scoped`.

**Frontend**

- Backend calls only through `ipc.ts`.
- Local files through `loadFileAsBlobUrl`, never `asset://`.
- Tabs and filters use the carousel-chip pattern; match the existing styling and variables.
- Shared code must not assume Tauri; use `isWebMode()` when behaviour differs in a browser.

**Docs**

- Every command has a `///` doc comment.
- Every new source file should start with a one-line summary comment (`//!` in Rust, a header comment in TypeScript and Svelte) so the source map describes it.

## Reporting security problems

Do not open a public issue. Follow `SECURITY.md`.

Before you open a pull request, read {doc}`security`: it explains the review rules, the Contributor License Agreement and the checks your change will go through.

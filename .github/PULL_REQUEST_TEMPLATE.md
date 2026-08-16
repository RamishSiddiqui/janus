## What this does

<!-- Short description of the change and why it's needed. Link the issue it addresses, if any. -->

## How to test

<!-- What did you run to verify this? Steps to reproduce/confirm for a reviewer. -->

## Checklist

- [ ] `npm run check` passes
- [ ] `cd src-tauri && cargo check` passes
- [ ] `cd src-tauri && cargo test` passes (if you touched Rust code with test coverage)
- [ ] For a new/changed Tauri command: restarted the dev server so `src/lib/services/bindings.ts` regenerated, and committed the regenerated file
- [ ] Followed the conventions in [CONTRIBUTING.md](../CONTRIBUTING.md) (repository-per-table backend, IPC wrappers in `ipc.ts`, etc.)

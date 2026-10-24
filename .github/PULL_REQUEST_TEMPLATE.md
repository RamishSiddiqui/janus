## What this does

<!-- Short description of the change and why it's needed. Link the issue it addresses, if any. -->

## How to test

<!-- What did you run to verify this? Steps to reproduce/confirm for a reviewer. -->

## Checklist

- [ ] `npm run check`, `npm run lint` and `npm run format:check` pass
- [ ] `cd src-tauri && cargo check` passes
- [ ] `cd src-tauri && cargo test` passes (if you touched Rust code with test coverage)
- [ ] For a new/changed Tauri command: restarted the dev server so `src/lib/services/bindings.ts` regenerated, and committed the regenerated file
- [ ] Followed the conventions in [CONTRIBUTING.md](../CONTRIBUTING.md) (repository-per-table backend, IPC wrappers in `ipc.ts`, etc.)

## Agreement

- [ ] I have read and agree to the Janus Contributor License Agreement ([CLA.md](../CLA.md)), and I wrote this change or have the right to submit it.
- [ ] This change adds no new network destinations. If it does, I listed them above and why.

# Development setup

## Prerequisites

- [Node.js](https://nodejs.org/) 22.12 or newer and npm.
- The stable Rust toolchain.
- The platform build tools [Tauri v2 needs](https://v2.tauri.app/start/prerequisites/): WebView2 on Windows, Xcode command-line tools on macOS, `webkit2gtk` and friends on Linux.
- Python 3.10 or newer, only to build these docs.

## Run it

```bash
npm install
npm run tauri dev      # hot-reloading frontend and Rust backend
```

Frontend changes under `src/` hot-swap without losing app state. Backend changes under `src-tauri/` rebuild and restart the app.

When a debug build starts, it regenerates `src/lib/services/bindings.ts` from the Rust command signatures. **Do not edit that file by hand.** After changing a command's name, arguments or return type, restart the dev server and commit the regenerated file.

A debug build also seeds a few demo characters on first run (`db/seed.rs`, behind `debug_assertions`). Release builds do not.

## Everyday commands

| Command | What it does |
|---|---|
| `npm run tauri dev` | Run the app with hot reload. |
| `npm run check` | `svelte-kit sync` then `svelte-check` (TypeScript and Svelte). |
| `npm test` | Frontend unit tests (vitest). |
| `npm run build` | Build the static frontend into `build/`. |
| `npm run tauri build` | Build installers (`src-tauri/target/release/bundle/`). |
| `cargo check` / `cargo test` / `cargo clippy --all-targets -- -D warnings` / `cargo fmt --check` | Run from `src-tauri/`. CI runs the same. |
| `python docs/tools/gen_reference.py` | Regenerate the reference pages. |
| `python -m sphinx -W -b html docs docs/_build/html` | Build these docs. |

Run the full set before opening a pull request. CI is strict: clippy warnings and formatting differences fail the build.

## Where the app keeps its data

The dev app uses the same application data folder as an installed one (`com.janus.app`; see the user guide's [Data and privacy](../user/data-and-privacy.md)). To start fresh, close the app and move that folder aside.

## Build tips

- A full Rust build is heavy. If the compiler crashes with `STATUS_STACK_BUFFER_OVERRUN` on Windows, or links fail with "not enough space", free disk space and build with `CARGO_BUILD_JOBS=1` and `CARGO_INCREMENTAL=0`. Deleting `src-tauri/target/debug/incremental` frees a lot.
- Do not run other Rust tooling (such as a language server) against the same `target` folder while building.
- The browser server serves the frontend that was **embedded at compile time**. If you change frontend code and want to see it through `http://127.0.0.1:<port>`, run `npm run build` and touch `src-tauri/src/lib.rs` to force a re-embed. The desktop window uses the Vite dev server and is not affected.

## Trying browser access in development

1. Run `npm run tauri dev`.
2. In the desktop window, go to **Settings, Network Access**, create the admin account, turn **Browser access** on and apply.
3. Open the address it shows in a browser and sign in.

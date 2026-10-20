# Repository layout

```text
.
├── src/                    SvelteKit frontend
│   ├── routes/             pages (one folder per route)
│   ├── lib/
│   │   ├── components/     Svelte components
│   │   ├── stores/         app state that outlives components
│   │   ├── services/       ipc.ts, bindings.ts (generated), webBridge.ts, helpers
│   │   ├── utils/          pure helpers
│   │   ├── data/           bundled content (starter characters)
│   │   ├── types/          shared TypeScript types
│   │   └── constants/
│   ├── hooks.client.ts     installs the browser bridge before the app starts
│   └── app.html, app.css
├── src-tauri/              Rust backend and Tauri config
│   ├── src/
│   │   ├── commands/       #[tauri::command] entry points
│   │   ├── context/        prompt pipeline (budget, window, summary, RAG, NPC)
│   │   ├── providers/      LLM, image/video and voice adapters
│   │   ├── db/             schema, migrations, one Repo per table
│   │   ├── models/         data types shared across layers
│   │   ├── auth/           accounts, sessions, ownership checks
│   │   ├── web/            browser server (axum)
│   │   ├── tts/            Kokoro engine, chunker, model download
│   │   ├── events.rs       event routing to accounts and browsers
│   │   ├── error.rs        MythicError and small helpers
│   │   └── lib.rs, main.rs app state, command registry, startup
│   ├── tests/              integration tests (real embedded database)
│   ├── avatars/            bundled demo avatars copied on first run
│   ├── capabilities/       Tauri permission sets
│   └── tauri.conf.json
├── docs/                   this documentation (Sphinx + Furo)
├── branding/               logos and README screenshots
├── static/                 static assets served by the frontend
├── .github/                CI, CodeQL, release, audit workflows; issue templates
├── CLAUDE.md               project workflow notes
├── CONTRIBUTING.md  SECURITY.md  CODE_OF_CONDUCT.md  LICENSE (AGPL-3.0)
└── cliff.toml              changelog template for releases
```

The {doc}`/reference/source-map` lists every source file with a one-line purpose.

## Backend module map

| Module | Responsibility |
|---|---|
| `commands/*` | Validate input, call repos and helpers, shape the response. One file per feature area. |
| `commands/actor.rs` | `acting()`: who is calling. |
| `commands/chat/` | `send`, `retry`, `streaming`, `pipeline`, `attachments`. |
| `context/*` | Pure prompt-building logic and the post-turn pipelines. No Tauri state. |
| `providers/*` | Network clients. No database writes (callers persist). |
| `db/*` | All SurrealDB queries live here, one `*Repo` per table. |
| `models/*` | Structs that cross layers (database rows, command inputs and outputs). |
| `auth/*` | Hashing, session logic, ownership (`access.rs`). |
| `web/*` | HTTP server for browsers. |
| `events.rs` | Delivers events to the right account's screens. |

## Frontend module map

| Folder | Responsibility |
|---|---|
| `routes/` | One page per feature: chat (`+page.svelte`), gallery, personas, memories, providers, models, media-models, embedders, voice-models, trash, settings. |
| `components/` | Reusable UI: chat parts, sidebar, settings sections, memory graph and timeline, scene display, the sign-in gate, the first-run wizard. |
| `stores/` | State that must survive remounts (chat, stream, multi-character, scene generation, TTS playback, settings, toasts, logs). |
| `services/ipc.ts` | The only place that calls the backend. Typed wrappers. |
| `services/bindings.ts` | Generated types and raw command stubs. Never hand-edited. |
| `services/webBridge.ts` | Makes the same code run in a browser. |

# Architecture

Janus is a desktop app with two halves in one process tree:

```text
┌──────────────────────────────────────────────────────────────────────┐
│  Frontend: SvelteKit (static SPA, TypeScript, Svelte 5)              │
│    routes/  components/  stores/  services/ipc.ts                    │
└───────────────▲───────────────────────────────────▲──────────────────┘
                │ Tauri invoke / events (desktop)   │ HTTP /api/* + SSE (browser)
┌───────────────┴───────────────────────────────────┴──────────────────┐
│  Backend: Rust (Tauri 2)                                              │
│    commands/   thin entry points, one #[tauri::command] each          │
│    context/    prompt building, budget, window, summary, RAG, NPCs    │
│    providers/  LLM (rig-core), image/video, voice adapters             │
│    db/         one Repo per table, schema, migrations, backup          │
│    auth/ web/ events.rs   accounts, browser server, event routing      │
│    tts/        embedded Kokoro engine (ONNX)                           │
└───────────────┬───────────────────────────────────────────────────────┘
                │ embedded
        SurrealDB 3 (RocksDB) in the app data folder
```

## Design decisions

**Local first.** The database is embedded SurrealDB (RocksDB storage) inside the app. There is no server to run. Providers are called directly from the user's machine with the user's own keys.

**One command layer, two transports.** Every feature is a `#[tauri::command]` function. The desktop frontend calls it with Tauri's `invoke()`. The browser server calls *the same function* through `POST /api/rpc/{name}`, with the signed-in account carried in a task-local variable. There is no second copy of any logic. See {doc}`browser-server`.

**Typed IPC.** [tauri-specta](https://github.com/specta-rs/tauri-specta) generates `src/lib/services/bindings.ts` from the Rust command signatures when a debug build starts. The frontend wraps those calls in `src/lib/services/ipc.ts`; components never call `invoke()` directly.

**Per-account ownership.** Seven root tables carry an `owner_id`. Commands start with `acting(&state)` to learn who is calling and either filter by owner or call an `ensure_*` check on any id they are given. See {doc}`accounts`.

**Events, not polling.** Long work (streaming a reply, speaking, generating an image, indexing) pushes events to the screen. On the desktop they are Tauri events; for browsers the same events travel over Server-Sent Events. Each is routed to the account it belongs to. See {doc}`events`.

**Layered, token-budgeted prompts.** The prompt is assembled from fixed layers (system, card, persona, lorebook, memories, emotion, scene, summary) and a sliding window of recent history, under a token budget, with optional hybrid vector and keyword retrieval. See {doc}`context-engine`.

**Failures are contained.** Background pipelines (embedding, summaries, scene extraction, NPC detection, memory extraction) log and continue; they never block or fail the reply.

## A request, end to end

Sending a message on the desktop:

1. `ChatInput` calls a store function, which calls `ipc.sendMessage(...)`.
2. `invoke('send_message')` reaches `commands::chat::send::send_message`.
3. The command resolves the actor, saves the user message, builds the prompt, and starts a streaming task.
4. The streaming task forwards each chunk as a `chat-stream` event, saves the final message, then runs the background pipelines.
5. The store listens for `chat-stream` and updates the screen.

From a browser, steps 2 and 5 differ only in transport: `POST /api/rpc/send_message`, and the events arrive over `/api/events`. Steps 3 and 4 are identical. Details in {doc}`chat-pipeline`.

## Technology

| Concern | Choice |
|---|---|
| Shell | Tauri 2 |
| Frontend | SvelteKit (adapter-static SPA), Svelte 5 runes, TypeScript |
| Backend | Rust, tokio |
| Database | SurrealDB 3.x embedded (RocksDB) |
| LLM clients | [rig-core](https://github.com/0xPlaygrounds/rig) behind one `RigProvider` |
| Typed IPC | tauri-specta |
| Browser server | axum |
| Voice | Kokoro-82M via ONNX Runtime (`ort`), `misaki-rs` for phonemes |
| Tokens | tiktoken-rs |
| Passwords | argon2id |
| Tests | cargo test (unit and integration), vitest |

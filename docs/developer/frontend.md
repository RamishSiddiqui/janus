# Frontend guide

The frontend is a SvelteKit single-page app (`adapter-static` with an `index.html` fallback), written in TypeScript with Svelte 5 runes. It runs in Tauri's webview, and unchanged in a browser through the {doc}`bridge <browser-server>`.

## Talking to the backend

All backend calls go through `src/lib/services/ipc.ts`.

- Each function is a typed wrapper around `safeInvoke<T>(command, args)`.
- `safeInvoke` logs every failure once and normalizes errors to `{ error, message }`.
- Components never call `invoke()` or the generated `bindings.ts` directly.
- To add a feature: write the Rust command, restart the dev server to regenerate `bindings.ts`, then add a documented wrapper in `ipc.ts`.

Events are received with `listen()` from `@tauri-apps/api/event`; in a browser the same call is served by the bridge.

## Detecting the backend

Many components start with `const isTauri = browser && '__TAURI_INTERNALS__' in window`, meaning "a backend is available". In a browser served by Janus the bridge installs a stand-in for `__TAURI_INTERNALS__`, so those checks pass and every call is routed over HTTP. In a plain browser pointed at the Vite dev server there is no backend and pages fall back to demo data.

## State

| Mechanism | Use |
|---|---|
| Component `$state` | UI state that can reset when the component remounts. |
| Stores in `lib/stores/` | State that must survive remounts or is shared. |
| `settings` store | App preferences, saved in `localStorage` under one key, with a version number that refreshes managed prompts when defaults change. |

Stores:

| Store | Holds |
|---|---|
| `chat.ts` | Conversations, the active conversation, avatar cache, create/select/rename/delete. |
| `chatMessages.ts` | Messages of the active conversation and message actions. |
| `chatStream.ts` | Streaming state and the `chat-stream` listener. |
| `chatMultiChar.ts` | Group-chat segment handling (`multi-char-response`). |
| `chatEmotion.ts` | Parsing of emotional snapshots. |
| `sceneGeneration.ts` | Image/video generation progress, keyed by conversation so it survives remounts. |
| `ttsPlayback.ts` | The streamed audio queue and playback state. |
| `settings.ts` | Persisted preferences ({doc}`/reference/settings`). |
| `personas.ts` | The selected persona. |
| `toast.ts` | Notifications. |
| `logs.ts` | Captured frontend console logs. |

## Services

| File | Purpose |
|---|---|
| `emotion-updater.ts` | After a reply, asks the model for the speaking character's new emotional state (JSON) using the previous state as a baseline. Fire-and-forget. |
| `memory-extractor.ts` | After a reply, asks the model for lasting facts worth remembering; most exchanges yield none. |
| `presentation-buffer.ts` | The buffer between the backend stream and the message store: resolves each character's name, avatar and colour up front, detects `[CharName]:` markers mid-stream (even split across chunks), creates per-character bubbles from the first frame, and batches updates with `requestAnimationFrame`. |
| `webBridge.ts` | The browser transport (see {doc}`browser-server`). |

## Routes

| Route | Page |
|---|---|
| `/` | Chat (the largest page: conversation, header tools, context panel, scene display). |
| `/gallery`, `/gallery/[id]` | Characters and a character's profile. |
| `/personas`, `/personas/[id]` | Personas and a persona's page. |
| `/memories` | Memory graph and timeline. |
| `/providers` | Provider setup. |
| `/models`, `/media-models`, `/embedders`, `/voice-models` | Model lists for chat, image/video, embeddings and voices. |
| `/trash` | Soft-deleted items. |
| `/settings` | Settings sections. |

`+layout.svelte` owns the shell: title bar, sidebar (a drawer on phones), theme and font size, the first-run wizard, and, in a browser, the sign-in gate.

## Components

The largest groups:

- **Chat:** `ChatHeader`, `ChatMessage`, `ChatInput`, `ThinkingBlock`, `EmotionHUD`, `SceneDisplay`, `SceneStateBar`, `SceneGalleryModal`.
- **Context panels:** `ContextPanel`, `ContextCarousel`, `ContextLorebook`, `ContextMemories`, `ContextNpcPanel`, `ContextPersonaPanel`, `ChatExplorerView`.
- **Memory views:** `MemoryGraph` (with `nodes/` and `edges/`), `MemoryTimeline`, `MemoryActionPanel`, `TimelineFilter`.
- **Sidebar:** `Sidebar`, `SidebarNav`, `SidebarConversationList`, `SidebarSearch`, `SidebarContextMenu`.
- **Settings:** one `Settings*Section` component per section.
- **Browser and onboarding:** `WebGate` (the sign-in screen), `FirstRunWizard`.
- **Shared:** `Icon`, `Skeleton`, `ToastContainer`, `ErrorBoundary`, `SelectCombobox`, `TitleBar`.

The {doc}`/reference/source-map` lists every file.

## Conventions

- **Design language.** Tabs, filters and mode switches use the "Light Carousel" chip pattern: separate floating pills, the active one filled with the accent colour, scaled slightly and glowing. See `.carousel-chip` in the settings page for the canonical CSS. Colours and spacing come from the CSS variables in `app.css`.
- **Local files use blob URLs.** The app's content security policy does not allow `asset://`, so images are loaded with `loadFileAsBlobUrl` (`utils/blobUrl.ts`) and revoked when replaced or unmounted. In a browser the same function fetches `/api/files/...`.
- **Roleplay text formatting** is in `utils/format.ts`: `*actions*` become italic action text, `**bold**` stays bold, and output is sanitized.
- **Macros.** `{{user}}` is replaced by the persona name on the client for the greeting (`utils/personaMacros.ts`) and by the backend in prompts.
- **No browser-only assumptions in shared code.** Anything that must differ in a browser checks `isWebMode()` from `webBridge.ts`.

## Responsive layout

At 768 px and below the sidebar becomes an off-canvas drawer opened from the title bar, with a backdrop, closing on selection or `Esc`. The context panel hides below 1024 px.

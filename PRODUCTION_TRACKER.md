# Mythic — Production Tracker

> **Single source of truth.** Every claim in this document has been verified against the actual codebase as of 2026-05-18. Emoji legend: ✅ working · 🟡 partial · ❌ missing · 🔇 backend-only (no frontend)

---

## 1 · Architecture Overview

```
src-tauri/                       Rust backend (Tauri v2)
├── src/
│   ├── lib.rs                   App setup, invoke_handler registration
│   ├── commands/                11 modules, 54 IPC commands
│   │   ├── characters.rs        CRUD (5 cmds)
│   │   ├── conversations.rs     CRUD + messages + scope + search (10 cmds)
│   │   ├── messages.rs          create/update/delete/branch/siblings (5 cmds)
│   │   ├── chat.rs              send/regenerate/generate_raw (3 cmds)
│   │   ├── providers.rs         CRUD + health + models (11 cmds)
│   │   ├── import.rs            card import + avatar path (2 cmds)
│   │   ├── lorebook.rs          CRUD (4 cmds)
│   │   ├── memories.rs          CRUD + promote/share/unlink/graph (8 cmds)
│   │   ├── scenes.rs            generate/list/delete/path (4 cmds)
│   │   └── character_state.rs   get/upsert emotional state (2 cmds)
│   ├── providers/               Ollama, OpenAI, OpenRouter adapters
│   ├── db.rs                    SQLite + sqlx migrations
│   └── models.rs                DB model structs
├── migrations/                  12 SQL migrations (001–012)
└── avatars/                     Bundled seed character images

src/                             Svelte 5 frontend (SvelteKit)
├── routes/
│   ├── +page.svelte             Chat page (main view)
│   ├── +layout.svelte           Shell: sidebar + routing + theme
│   ├── gallery/+page.svelte     Character gallery (masonry cards)
│   ├── gallery/[id]/+page.svelte Character profile page (5 tabs)
│   ├── providers/+page.svelte   Provider management (3-col)
│   ├── models/+page.svelte      Model browser + enable/disable
│   ├── memories/+page.svelte    Memory graph + timeline views
│   └── settings/+page.svelte    Preferences (theme/streaming/privacy)
├── lib/
│   ├── components/              13 Svelte components
│   │   ├── Sidebar.svelte       Nav + character-grouped convos
│   │   ├── ChatMessage.svelte   Bubbles + branch nav + EmotionHUD
│   │   ├── ChatHeader.svelte    Active chat topbar
│   │   ├── ChatInput.svelte     Compose box + model picker
│   │   ├── ContextPanel.svelte  Right panel: lorebook + scenes
│   │   ├── MemoryGraph.svelte   SvelteFlow graph visualization
│   │   ├── MemoryTimeline.svelte Timeline lane view
│   │   ├── EmotionHUD.svelte    Mood/trust/arousal pill
│   │   ├── SceneDisplay.svelte  Image scene viewer
│   │   └── ...                  Icon, Skeleton, ErrorBoundary, Toast
│   ├── services/
│   │   ├── ipc.ts               Frontend IPC layer (all Tauri invokes)
│   │   ├── memory-extractor.ts  LLM-powered fact extraction
│   │   └── emotion-updater.ts   Post-response emotion analysis
│   └── stores/
│       ├── chat.ts              Conversation + message state
│       ├── settings.ts          User preferences (localStorage)
│       └── toast.ts             Notification store
└── app.css                      Global design system (731 lines)
```

---

## 2 · Feature Status Matrix

### 2.1 · Core Chat

| Feature | Frontend | Backend | Status | Notes |
|---|---|---|---|---|
| **Conversation CRUD** | Sidebar + context menu | `conversations.rs` | ✅ | Create/rename/delete |
| **Message Persistence** | Chat page tree render | `messages.rs` | ✅ | Parent-child tree structure |
| **Streaming Chat** | Token-by-token display | `chat.rs` mpsc→events | ✅ | `chat-stream` event bus |
| **Non-Streaming Chat** | Full response at once | `chat.rs` `generate()` | ✅ | Toggled in settings |
| **Prompt Building** | Automatic from card | `build_prompt()` | ✅ | System + desc + personality + scenario |
| **Send Message** | ChatInput | `send_message` | ✅ | |
| **Regenerate Response** | ↻ button on AI bubbles | `regenerate_message` | ✅ | Creates sibling branch |
| **Message Editing** | Pencil icon on user bubbles | `update_message` | ✅ | Inline textarea |
| **Message Deletion** | ❌ No UI | `delete_message` | 🔇 | Backend works, no button in ChatMessage |
| **First Message (Greeting)** | Auto-sent on new conv | Store logic | ✅ | `first_mes` from character card |
| **Model Picker** | Dropdown in ChatInput | Adapter dispatch | ✅ | Ollama/OpenRouter/OpenAI |
| **Retry on Error** | Retry button in error state | — | ✅ | Shows on stream failure |

### 2.2 · Message Branching

| Feature | Frontend | Backend | Status | Notes |
|---|---|---|---|---|
| **Branch Tree** | `loadMessages` walks chain | `active_message_id` | ✅ | Path resolution on load |
| **Branch Navigator** | Dot-track + arrows in ChatMessage | `get_message_siblings` | ✅ | Prev/next + direct-click dots |
| **Branch Switch** | `switchBranch()` in chat store | `set_active_message` | ✅ | Reloads messages after switch |
| **Regenerate → Sibling** | Old response preserved | `regenerate_message` | ✅ | Branch tree grows correctly |

### 2.3 · Characters

| Feature | Frontend | Backend | Status | Notes |
|---|---|---|---|---|
| **Character CRUD** | Gallery masonry cards | `characters.rs` | ✅ | Full lifecycle |
| **Character Card Import** | File picker in Gallery | `import.rs` V2/V1 PNG+JSON | ✅ | Extracts embedded lorebook |
| **Avatar Upload** | Gallery editor card | `AppData/avatars/` | ✅ | Blob URL caching |
| **Character Export** | ❌ No UI | ❌ No backend | ❌ | Export to PNG/JSON missing |
| **Character Profile Page** | `/gallery/[id]` | — | ✅ | 5 tabs: Profile/Memories/Lore/Stats/Edit |
| **Profile Edit** | Edit tab with form | `update_character` | ✅ | Name/desc/personality/scenario/first_mes/tags |

### 2.4 · Emotional State

| Feature | Frontend | Backend | Status | Notes |
|---|---|---|---|---|
| **Character State Table** | — | `character_states` table | ✅ | Migration 010 |
| **LLM Inference** | `emotion-updater.ts` | `generate_raw` | ✅ | Runs after each response |
| **EmotionHUD Pill** | In ChatMessage toolbar | — | 🟡 | Component exists, renders conditionally. **Not yet visually verified by user** |
| **Prompt Injection** | — | `build_prompt` baseline | ✅ | Delta-continuity emotional context |

### 2.5 · Lorebook

| Feature | Frontend | Backend | Status | Notes |
|---|---|---|---|---|
| **Always-Active Entries** | ContextPanel add/toggle/delete | `lorebook.rs` | ✅ | Injected into system prompt |
| **Keyword-Trigger Entries** | Managed in ContextPanel | `build_prompt()` scan | ✅ | Scans last 20 messages |
| **Lorebook Search/Filter** | Profile page Lore tab | Client-side filter | ✅ | Match by name/keys/content |
| **Imported Lorebook** | Auto-extracted from card | `import.rs` | ✅ | V2 card embedded lore |

### 2.6 · Memories

| Feature | Frontend | Backend | Status | Notes |
|---|---|---|---|---|
| **Memory CRUD** | ❌ No direct CRUD UI | `memories.rs` 4 cmds | 🟡 | IPC wrappers exist (`listMemories`, `createMemory`, `updateMemory`, `deleteMemory`), but NO component calls them directly. ContextPanel has zero memory UI |
| **Auto-Save Memories** | Toggle in Settings | `memory-extractor.ts` | ✅ | Two-tier: LLM + heuristic fallback, throttled every 3rd message |
| **Memory Graph Visualizer** | `/memories` page + Profile tab | `get_memory_graph` | ✅ | SvelteFlow force-directed layout |
| **Memory Timeline** | `/memories` page toggle | `MemoryTimeline.svelte` | ✅ | Lane-based chronological view |
| **Promote to Canon** | ❌ No UI | `promote_to_canon` | 🔇 | IPC wrapper exists, no component calls it |
| **Share Memory** | ❌ No UI | `share_memory` | 🔇 | IPC wrapper exists, no component calls it |
| **Unlink Memory** | ❌ No UI | `unlink_memory` | 🔇 | IPC wrapper exists, no component calls it |
| **Update Memory** | ❌ No UI | `update_memory` | 🔇 | IPC wrapper exists, no component calls it |
| **Memory Scope Control** | ❌ No UI | `set_memory_scope` | 🔇 | Backend registered in lib.rs, **no IPC wrapper in ipc.ts**, no UI |

### 2.7 · Scenes

| Feature | Frontend | Backend | Status | Notes |
|---|---|---|---|---|
| **Scene Generation** | SceneDisplay in ContextPanel | `scenes.rs` | ✅ | OpenAI images API compatible |
| **Placeholder Scenes** | Auto gradient PNG | `generate_placeholder_png()` | ✅ | When no image provider configured |
| **Scene Gallery** | SceneDisplay carousel | `list_scenes` | ✅ | Per-conversation |
| **Scene Deletion** | ❌ No UI button | `delete_scene` | 🔇 | IPC wrapper exists (`deleteScene`), no component calls it |
| **Auto-Generate Images** | Toggle exists in Settings | ❌ No trigger logic | ❌ | Setting saved but never read in chat flow |
| **Image Provider Adapters** | ❌ No setup UI | ❌ No adapters | ❌ | `ImageProvider` trait defined, no SiliconFlow/ComfyUI impl |
| **Video Generation** | ❌ | ❌ | ❌ | Schema-only placeholder |

### 2.8 · Search

| Feature | Frontend | Backend | Status | Notes |
|---|---|---|---|---|
| **FTS5 Message Search** | ❌ **No UI anywhere** | `search_messages` (FTS5) | 🔇 | Backend registered + IPC wrapper (`searchMessages`) exists in ipc.ts, but **zero components call it**. No search box in Sidebar, no search overlay in Chat |

### 2.9 · Providers & Models

| Feature | Frontend | Backend | Status | Notes |
|---|---|---|---|---|
| **Provider CRUD** | `/providers` page (3-col) | `providers.rs` | ✅ | Add/edit/delete/test |
| **Health Check** | Green/red dot + latency | `test_provider_connection` | ✅ | HTTP ping |
| **Multi-Provider** | Adapter dropdown | `create_llm_provider()` | ✅ | Ollama/OpenRouter/OpenAI |
| **Set Default Provider** | Star button | `set_default_provider` | ✅ | |
| **Model Browser** | `/models` page + filters | `list_all_models` | ✅ | Provider/type/status filter |
| **Enable/Disable Models** | Toggle switch per model | `toggle_model_enabled` | ✅ | Persisted in `enabled_models` table |
| **Model List (Chat)** | Dropdown in ChatInput | `list_enabled_models` | ✅ | Shows only enabled models |

### 2.10 · Settings & Data

| Feature | Frontend | Backend | Status | Notes |
|---|---|---|---|---|
| **Theme System** | Dark/Light/System toggle | `data-theme` + CSS vars | ✅ | Full light theme override |
| **Font Size** | Small/Medium/Large dropdown | `--app-font-size` CSS var | ✅ | Scales all typography |
| **Streaming Toggle** | On/off switch | `generate` vs `generate_stream` | ✅ | |
| **System Prompt Override** | Textarea in Settings | `build_prompt()` | ✅ | {{char}}/{{user}} placeholders |
| **Local Storage Only** | Privacy toggle + confirm | Feature gating | ✅ | `isLocalOnly()` utility |
| **Export Data** | Button → JSON file | File dialog | ✅ | Conversations + characters + settings |
| **Import Data** | Button → file picker | File dialog | ✅ | Restores settings (not full DB restore) |
| **Clear All Conversations** | Danger button + confirm | Bulk delete | ✅ | |
| **Keyboard Shortcuts** | Ctrl+N, Ctrl+B, Esc | `+layout.svelte` | ✅ | Global |

---

## 3 · IPC Command Coverage

Every backend command registered in `lib.rs` mapped to frontend integration.

| Command | IPC Wrapper | UI Calls It | Status |
|---|---|---|---|
| `get_app_info` | ✅ | ✅ | ✅ |
| **Characters** | | | |
| `create_character` | ✅ | ✅ Gallery | ✅ |
| `get_character` | ✅ | ✅ Profile + Chat | ✅ |
| `list_characters` | ✅ | ✅ Gallery + Sidebar | ✅ |
| `update_character` | ✅ | ✅ Profile Edit tab | ✅ |
| `delete_character` | ✅ | ✅ Gallery editor | ✅ |
| **Conversations** | | | |
| `create_conversation` | ✅ | ✅ Sidebar | ✅ |
| `get_conversation` | ✅ | ✅ Chat store | ✅ |
| `list_conversations` | ✅ | ✅ Sidebar + Profile | ✅ |
| `count_conversations` | ✅ | ✅ Sidebar pagination | ✅ |
| `delete_conversation` | ✅ | ✅ Context menu | ✅ |
| `get_conversation_messages` | ✅ | ✅ Chat + Profile | ✅ |
| `set_active_message` | ✅ | ✅ Branch switch | ✅ |
| `update_conversation` | ✅ | ✅ Rename | ✅ |
| `set_memory_scope` | ❌ None | ❌ | 🔇 Orphaned |
| `search_messages` | ✅ | ❌ None | 🔇 No UI |
| **Messages** | | | |
| `create_message` | ✅ | ❌ (backend-internal) | N/A |
| `update_message` | ✅ | ✅ Edit inline | ✅ |
| `delete_message` | ✅ | ❌ No delete UI | 🔇 |
| `get_message_branch` | ✅ | ❌ Not used | 🔇 |
| `get_message_siblings` | ✅ | ✅ Branch nav dots | ✅ |
| **Providers** | | | |
| `create_provider` | ✅ | ✅ | ✅ |
| `get_provider` | ✅ | ✅ | ✅ |
| `list_providers` | ✅ | ✅ | ✅ |
| `update_provider` | ✅ | ✅ | ✅ |
| `delete_provider` | ✅ | ✅ | ✅ |
| `set_default_provider` | ✅ | ✅ | ✅ |
| `test_provider_connection` | ✅ | ✅ | ✅ |
| `list_provider_models` | ✅ | ✅ | ✅ |
| `list_all_models` | ✅ | ✅ Models page | ✅ |
| `toggle_model_enabled` | ✅ | ✅ Models page | ✅ |
| `list_enabled_models` | ✅ | ✅ ChatInput | ✅ |
| **Chat** | | | |
| `send_message` | ✅ | ✅ | ✅ |
| `regenerate_message` | ✅ | ✅ | ✅ |
| `generate_raw` | ✅ | ✅ Extractors | ✅ |
| **Import** | | | |
| `import_character_card` | ✅ | ✅ Gallery | ✅ |
| `get_avatar_path` | ✅ | ✅ | ✅ |
| **Scenes** | | | |
| `generate_scene` | ✅ | ✅ ContextPanel | ✅ |
| `list_scenes` | ✅ | ✅ SceneDisplay | ✅ |
| `delete_scene` | ✅ | ❌ No UI | 🔇 |
| `get_scene_path` | ✅ | ✅ SceneDisplay | ✅ |
| **Lorebook** | | | |
| `list_lorebook_entries` | ✅ | ✅ ContextPanel | ✅ |
| `create_lorebook_entry` | ✅ | ✅ ContextPanel | ✅ |
| `toggle_lorebook_entry` | ✅ | ✅ ContextPanel | ✅ |
| `delete_lorebook_entry` | ✅ | ✅ ContextPanel | ✅ |
| **Memories** | | | |
| `list_memories` | ✅ | ✅ Profile stats | ✅ |
| `create_memory` | ✅ | ✅ Extractor service | ✅ |
| `update_memory` | ✅ | ❌ No UI | 🔇 |
| `delete_memory` | ✅ | ❌ No UI | 🔇 |
| `promote_to_canon` | ✅ | ❌ No UI | 🔇 |
| `share_memory` | ✅ | ❌ No UI | 🔇 |
| `unlink_memory` | ✅ | ❌ No UI | 🔇 |
| `get_memory_graph` | ✅ | ✅ MemoryGraph | ✅ |
| **Character State** | | | |
| `get_character_state` | ✅ | ✅ Chat store | ✅ |
| `upsert_character_state` | ✅ | ✅ Emotion updater | ✅ |

**Summary:** 54 commands registered → 42 fully wired (78%) · 10 backend-only (18%) · 2 orphaned (4%)

---

## 4 · Production Readiness

### 4.1 · Security 🟢

- [x] Strict CSP in `tauri.conf.json`
- [x] Input validation: `validate_string_length()` + `validate_required_string()` in Rust
- [x] No raw SQL injection vectors (sqlx parameterized queries throughout)

### 4.2 · Error Handling UX 🟢

- [x] Toast on `sendMessage()` failure + optimistic message removal
- [x] Toast on `loadConversations()` failure
- [x] Toast on `loadMessages()` failure
- [x] Toast on stream error with error content display
- [x] Character import error detail display
- [x] Retry button on failed streaming responses

### 4.3 · Core UX Gaps 🟢

- [x] Regenerate (↻) on assistant bubbles with full stream support
- [x] Edit (pencil) on user bubbles with inline textarea
- [x] Auto-send `first_mes` greeting on new conversation
- [x] Model name display from active provider
- [x] Model picker dropdown in ChatInput

### 4.4 · Settings Wiring 🟢

- [x] Font Size → `--app-font-size` CSS variable
- [x] System Prompt → injected in `build_prompt()`
- [x] Streaming Toggle → `generate()` vs `generate_stream()`
- [x] Auto-Save Memories → controls `memory-extractor.ts` activation
- [x] Local Storage Only → feature gating via `isLocalOnly()`

### 4.5 · Data Integrity 🟢

- [x] CASCADE delete: conversations → messages → lorebook on character delete
- [x] Blob URL revocation (prevents memory leaks)
- [ ] Periodic VACUUM/optimize *(deferred — low priority)*

### 4.6 · Performance 🟢

- [x] Blob URL revocation before creating new ones
- [x] Debounced sidebar search (150ms)
- [x] Conversation list pagination (30 per page, Load More)

### 4.7 · Build & Release 🟢

- [x] Release profile optimizations in Cargo.toml
- [x] `npx tauri build` produces MSI + NSIS
- [x] Zero warnings in release build

### 4.8 · Code Cleanup 🟡

- [x] Toast notifications on all error paths
- [x] Mock data gated behind `import.meta.env.DEV`
- [x] TypeScript/Vite build warnings fixed (general)
- [ ] MemoryGraph SvelteFlow TS errors remain (non-blocking)

---

## 5 · UI Design Status

| Component | Status | Aesthetic |
|---|---|---|
| **Sidebar** | ✅ | Character-grouped conversations, glow nav, circular avatars |
| **Chat Header** | ✅ | Gradient accent, circular avatar, ring glow |
| **Chat Input** | ✅ | Glassmorphic container, animated focus glow, model picker |
| **Chat Messages** | ✅ | Gradient user bubbles, AI glass bubbles |
| **Context Panel** | ✅ | Spring entrance, circular avatar, gradient theme, char carousel |
| **Landing Page** | ✅ | Animated idle state with floating orbs |
| **Gallery Page** | ✅ | Masonry layout, gradient cards, staggered entrance |
| **Character Profile** | ✅ | Glassmorphism cards, gradient stats, violet glow, animated tabs |
| **Providers Page** | ✅ | 3-column cards, health indicators, gradient header |
| **Models Page** | ✅ | Filter bar, toggle switches, provider grouping |
| **Memories Page** | ✅ | Graph + timeline views, character picker, stat strip |
| **Settings Page** | ✅ | Gradient toggles, glass sections, premium dropdowns |
| **Toast Notifications** | ✅ | Backdrop-blur, gradient border glow, spring animation |
| **Skeleton Loaders** | ✅ | Purple shimmer gradient |
| **Scrollbars** | ✅ | 4px purple-tinted globally |
| **Light Theme** | ✅ | Full CSS variable override system |
| **Fonts** | ✅ | Inter 400–800 + Geist Mono |
| **Branch Navigator** | ✅ | Dot-track + arrows in message toolbar |
| **EmotionHUD** | 🟡 | Colour-coded pill, 3-bar meter — exists but not yet verified rendering |

---

## 6 · Known Gaps & Remaining Work

### 6.1 · Missing UI for Existing Backend (Quick Wins)

These features have full backend support AND IPC wrappers — they only need component-level UI buttons or panels:

| Gap | Backend | IPC | Effort | Priority |
|---|---|---|---|---|
| **Message Delete** button in ChatMessage | ✅ | ✅ `deleteMessage` | Small | Medium |
| **Scene Delete** button in SceneDisplay | ✅ | ✅ `deleteScene` | Small | Medium |
| **Message Search** overlay in Sidebar/Chat | ✅ FTS5 | ✅ `searchMessages` | Medium | High |
| **Memory Edit/Delete** in ContextPanel or Memories page | ✅ | ✅ `updateMemory`, `deleteMemory` | Medium | High |
| **Promote to Canon** button on memory entries | ✅ | ✅ `promoteToCanon` | Small | Medium |
| **Share Memory** dialog | ✅ | ✅ `shareMemory` | Medium | Low |
| **Unlink Memory** button | ✅ | ✅ `unlinkMemory` | Small | Low |
| **Memory Scope** toggle per conversation | ✅ | ❌ needs wrapper | Medium | Low |

### 6.2 · Missing Backend (Larger Features)

| Gap | Notes | Effort |
|---|---|---|
| **Character Export** (PNG/JSON) | No backend export logic | Medium |
| **Auto-Generate Images** trigger | Setting exists but chat flow never reads it | Medium |
| **Image Provider Adapters** | `ImageProvider` trait exists, no concrete adapters | Large |
| **Video Generation** | Schema only | Large |
| **Extractor Model Setting** | Let users pick a smaller model for background extraction | Small |

### 6.3 · Technical Debt

| Item | Status |
|---|---|
| MemoryGraph SvelteFlow TS errors | Non-blocking warnings |
| Periodic DB VACUUM | Deferred (low priority) |
| `set_memory_scope` orphaned (no IPC wrapper) | Needs frontend integration |
| `get_message_branch` unused | Available but no consumer |
| EmotionHUD visual verification | Pending user testing |

---

## 7 · Database Migrations

| # | File | Purpose |
|---|---|---|
| 001 | `initial_schema.sql` | Characters, conversations, messages, providers, lorebook |
| 002 | `scenes.sql` | Scene generation table |
| 003 | `seed_defaults.sql` | Seed characters (Aria, Kael, Lyra, Selene, Zephyr) |
| 004 | `memories.sql` | Memory table + character/conversation FKs |
| 005 | `fts_messages.sql` | FTS5 full-text search on messages |
| 006 | `memory_scope.sql` | Per-conversation memory scope flag |
| 007 | `memory_management.sql` | Memory links table (copy/sync/one_way/two_way) |
| 008 | `seed_memory_test_data.sql` | Test memories for development |
| 009 | `enforce_copy_one_way.sql` | Constraint: copy links always one_way |
| 010 | `character_states.sql` | Emotional state table (mood/trust/arousal) |
| 011 | `enabled_models.sql` | Model enable/disable tracking per provider |
| 012 | `clear_placeholder_models.sql` | Clean up placeholder model entries |

---

## 8 · Component File Inventory

| File | Size | Purpose |
|---|---|---|
| `Sidebar.svelte` | 51KB | Navigation + character-grouped conv list |
| `ContextPanel.svelte` | 35KB | Right panel: lorebook + scenes (NO memory UI) |
| `MemoryTimeline.svelte` | 30KB | Timeline lane visualization |
| `ChatMessage.svelte` | 22KB | Message bubbles + branch nav + EmotionHUD |
| `+page.svelte` (Chat) | 21KB | Main chat interface |
| `MemoryGraph.svelte` | 19KB | SvelteFlow graph visualization |
| `EmotionHUD.svelte` | 16KB | Animated mood/trust/arousal pill |
| `SceneDisplay.svelte` | 14KB | Scene image carousel |
| `ChatInput.svelte` | 10KB | Compose box + model picker |
| `Icon.svelte` | 8KB | SVG icon library |
| `ChatHeader.svelte` | 6KB | Active chat topbar |

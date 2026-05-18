# Mythic — Project Tracker

## Status Legend
- 🔴 Not started
- 🟡 In progress
- 🟢 Complete

---

# Part 1: Feature Status

## ✅ Fully Working Features

| Feature | Frontend | Backend | Notes |
|---|---|---|---|
| **Character CRUD** | Gallery page with masonry cards | `characters.rs` | Full lifecycle |
| **Character Card Import** | File picker in Gallery | `import.rs` — V2/V1 PNG+JSON | Extracts embedded lorebook |
| **Avatar Upload** | Gallery editor card | `AppData/avatars/` | Blob URL caching in chat store |
| **Conversation CRUD** | Sidebar with recent list | `conversations.rs` | Context menu rename/delete |
| **Message Persistence** | Chat page with tree rendering | `messages.rs` | Parent-child tree structure |
| **Streaming Chat** | Real-time token display, cursor blink | `chat.rs` — mpsc → Tauri events | `chat-stream` event bus |
| **Prompt Building** | Automatic from character card | `build_prompt()` | System + description + personality + scenario |
| **Lorebook (always-active)** | ContextPanel add/toggle/delete | `lorebook.rs` CRUD | Injected into system prompt |
| **Lorebook (keyword-trigger)** | Managed in ContextPanel | Keyword scan in `build_prompt()` | Scans last 20 messages |
| **Provider Management** | Models page — add/edit/delete/test | `providers.rs` — full CRUD + health | 3-column layout |
| **Multi-Provider Support** | Adapter dropdown (Ollama/OpenRouter/OpenAI) | `create_llm_provider()` factory | Runtime dispatch |
| **Provider Health Check** | Green/red dot indicator | `test_provider_connection` | HTTP health ping |
| **Settings Persistence** | Settings page with toggles/dropdowns | `localStorage` via settings store | Theme, font size, streaming |
| **Theme System** | Dark/Light/System toggle | `data-theme` attribute + CSS vars | Full light theme |
| **Error Boundary** | ErrorBoundary wrapper component | — | Catches render errors |
| **Toast Notifications** | Glassmorphic slide-in toasts | Toast store | Success/error/info types |
| **Keyboard Shortcuts** | Ctrl+N, Ctrl+B, Esc | `+layout.svelte` | Global |
| **Scene Generation** | SceneDisplay in ContextPanel | `scenes.rs` — generate/list/delete | OpenAI images API compatible |
| **Placeholder Scenes** | Auto-generated gradient PNG | `generate_placeholder_png()` | When no image provider |

## ✅ Previously Partially Working (Now Complete)

| Feature | Status | Resolution |
|---|---|---|
| **Regenerate Response** | ✅ | UI button on assistant bubbles + full stream support |
| **Model Selection** | ✅ | Model picker dropdown (Ollama/OpenRouter/OpenAI) |
| **Conversation Rename** | ✅ | Auto-focus + select on rename, improved UX |
| **First Message (Greeting)** | ✅ | Auto-sent `first_mes` on new conversation |
| **Message Editing** | ✅ | Edit button on user bubbles with inline textarea |
| **Scene Display** | ✅ | Generate Scene button in chat toolbar → opens ContextPanel |
| **Memories** | ✅ | Full backend (SQLite table + CRUD), pin/delete UI in ContextPanel |
| **Font Size Setting** | ✅ | Applies via CSS custom property scale throughout all components |
| **System Prompt Override** | ✅ | Wired to `build_prompt()` backend pipeline |
| **Streaming Toggle** | ✅ | Backend supports both `generate` and `generate_stream` |
| **Message Search** | ✅ | FTS5 full-text search with highlighted snippets in sidebar overlay |
| **Character Profile Page** | ✅ | `/gallery/[id]` — hero panel + 5 tabs: Profile, Memories (canon graph), Lore, Stats, Edit |
| **Local Storage Only** | ✅ | Privacy guard with confirmation dialog, `isLocalOnly()` utility for feature gating |
| **Lorebook Search/Filter** | ✅ | Client-side filter by name, keywords, and content with match count |
| **Auto-save Memories** | ✅ | Two-tier extraction: LLM-powered (via `generate_raw`) with heuristic fallback, throttled every 3rd message |
| **Multi-Character Chat UX** | ✅ | Added Crossovers sidebar section and ContextPanel carousel with descriptions |
| **Message Branching (Quantum Timeline)** | ✅ | Awwwards-tier navigator: animated dot-track, direct-jump, Timeline Shift overlay. `loadMessages` walks active branch chain. `active_message_id` drives path resolution. |
| **Character Emotional State** | 🟡 Implemented, pending user verification | `character_states` table, LLM-inferred mood/trust/arousal after each response, delta-baseline continuity, injected into `build_prompt`, reactive `EmotionHUD` pill on message toolbar. **Not yet tested in app.** |
| **Regenerate → True Branching** | ✅ | `regenerate_message` now preserves old response as sibling instead of deleting it. Branch tree grows correctly on each regeneration. |


## 🔴 Not Implemented or Missing Layers

| Feature | Frontend | Backend | Notes |
|---|---|---|---|
| **Video Generation** | ❌ No UI | ❌ No adapter | Provider type `video` in schema only |
| **Image Provider Adapters** | ❌ No setup UI | ❌ No concrete adapters | `ImageProvider` trait defined, needs SiliconFlow/ComfyUI |
| **Character Export** | ❌ No UI | ❌ No export logic | Export to PNG/JSON missing |
| **Auto-generate Images** | ✅ Toggle UI exists | ❌ No auto-trigger | Logic needed in chat flow |
| **Message Deletion** | ❌ Missing UI | ✅ Implemented | Backend has `delete_message` |
| **Memory Scope Control** | ✅ Toggle in ContextPanel | ✅ Implemented | "Enable Memory" toggle in right pane (default off), wired to `set_memory_scope` |
| **Memory Editing/Linking** | ❌ Missing UI | ✅ Implemented | Backend supports update, promote to canon, share, unlink |
| **Scene Deletion** | ❌ Missing UI | ✅ Implemented | Backend has `delete_scene` |
| **EmotionHUD (Character Emotional State)** | 🟡 Implemented, not verified | ✅ Implemented | UI pill built and wired — user has not confirmed it appears correctly in app |
| **Extractor Model Setting** | ❌ Not started | ❌ Not started | **Future**: Add "Extractor Model" dropdown under AI Studio → Advanced to let users pick a smaller/faster model for background memory extraction. Currently uses same model as chat. |

## ✅ Memory Graph
| Feature | Frontend | Backend | Notes |
|---|---|---|---|
| **Memory Graph Visualizer** | ✅ Fully Working | ✅ Implemented | Fixed TypeScript issues with SvelteFlow integration |

## Backend Command Coverage

| IPC Command | Frontend Calls It? | Backend Works? |
|---|---|---|
| `get_app_info` | ✅ | ✅ |
| `create_character` | ✅ | ✅ |
| `get_character` | ✅ | ✅ |
| `list_characters` | ✅ | ✅ |
| `update_character` | ✅ | ✅ |
| `delete_character` | ✅ | ✅ |
| `import_character_card` | ✅ | ✅ |
| `get_avatar_path` | ❌ (reads file directly) | ✅ |
| `create_conversation` | ✅ | ✅ |
| `get_conversation` | ❌ | ✅ |
| `list_conversations` | ✅ | ✅ |
| `count_conversations` | ✅ | ✅ |
| `delete_conversation` | ✅ | ✅ |
| `update_conversation` | ✅ | ✅ |
| `get_conversation_messages` | ✅ | ✅ |
| `set_active_message` | ✅ | ✅ |
| `create_message` | ❌ (backend internal) | ✅ |
| `update_message` | ✅ | ✅ |
| `delete_message` | ❌ **No delete UI** | ✅ |
| `get_message_branch` | ❌ **No UI** | ✅ |
| `get_message_siblings` | ❌ **No UI** | ✅ |
| `create_provider` | ✅ | ✅ |
| `get_provider` | ✅ | ✅ |
| `list_providers` | ✅ | ✅ |
| `update_provider` | ✅ | ✅ |
| `delete_provider` | ✅ | ✅ |
| `set_default_provider` | ✅ | ✅ |
| `test_provider_connection` | ✅ | ✅ |
| `send_message` | ✅ | ✅ |
| `regenerate_message` | ✅ | ✅ |
| `generate_raw` | ✅ | ✅ |
| `generate_scene` | ✅ | ✅ |
| `list_scenes` | ✅ | ✅ |
| `delete_scene` | ❌ **No UI** | ✅ |
| `get_scene_path` | ✅ | ✅ |
| `list_lorebook_entries` | ✅ | ✅ |
| `create_lorebook_entry` | ✅ | ✅ |
| `toggle_lorebook_entry` | ✅ | ✅ |
| `delete_lorebook_entry` | ✅ | ✅ |
| `list_memories` | ✅ | ✅ |
| `create_memory` | ✅ | ✅ |
| `update_memory` | ❌ **No UI** | ✅ |
| `delete_memory` | ✅ | ✅ |
| `promote_to_canon` | ❌ **No UI** | ✅ |
| `share_memory` | ❌ **No UI** | ✅ |
| `unlink_memory` | ❌ **No UI** | ✅ |
| `get_memory_graph` | ✅ (has UI bugs) | ✅ |
| `get_character_state` | ✅ | ✅ |
| `upsert_character_state` | ✅ | ✅ |
| `set_active_message` (branch switch) | ✅ | ✅ |
| `get_message_siblings` | ✅ (Quantum Timeline) | ✅ |

---

# Part 2: Production Readiness Tasks

## Task 1: Security Hardening 🟢
- [x] Set strict CSP in `tauri.conf.json`
- [x] Add input length validation in Rust commands
- [x] Add `validate_string_length()` + `validate_required_string()` helpers

## Task 2: Error Handling UX 🟢
- [x] Show toast on `sendMessage()` failure + remove optimistic message
- [x] Show toast on `loadConversations()` failure
- [x] Show toast on `loadMessages()` failure
- [x] Show toast on stream error (with error content)
- [x] Improved character import error with details
- [x] Add "Retry" button on failed streaming responses

## Task 3: Core UX Gaps 🟢
- [x] Add Regenerate (↻) button on assistant bubbles — with full stream support
- [x] Add Edit (pencil) button on user bubbles with inline textarea
- [x] Auto-send `first_mes` greeting on new conversation
- [x] Model name display from active provider (was hardcoded)
- [x] Model picker dropdown in ChatInput (Ollama/OpenRouter/OpenAI-compatible)

## Task 4: Wire Disconnected Settings 🟢
- [x] Font Size → CSS variable `--app-font-size`
- [x] System Prompt → injected as first system message in `build_prompt()`
- [x] Streaming Toggle → uses `generate()` when disabled, `generate_stream()` when enabled

## Task 5: Data Integrity 🟢
- [x] CASCADE delete conversations + messages + lorebook on character delete
- [x] Blob URL revocation (prevents memory leaks)
- [ ] Periodic VACUUM/optimize *(deferred — low priority)*

## Task 6: Performance 🟢
- [x] Revoke old blob URLs before creating new ones
- [x] Debounce sidebar search (150ms)
- [x] Conversation list pagination (30 per page, Load More button)

## Task 7: Build & Release 🟢
- [x] Add release profile optimizations to Cargo.toml
- [x] Verify `npx tauri build` produces working binary (MSI + NSIS)
- [x] Fix release-mode build warnings (zero warnings in release build)

## Task 8: Code Cleanup 🟡
- [x] Toast notifications wired for all error paths
- [x] Gate mock data behind `import.meta.env.DEV` check
- [x] Fix TypeScript/Vite build warnings (except MemoryGraph errors)
- [ ] Fix MemoryGraph SvelteFlow TS errors

---

# Part 3: UI Redesign Status

## ✅ All Pages Complete

| Component | Status | Aesthetic |
|---|---|---|
| Sidebar | ✅ | Glassmorphic, circular avatars, search, glow nav, crossovers grouping |
| Chat Header | ✅ | Gradient accents, circular avatar, ring glow |
| Chat Input | ✅ | Glassmorphic container, animated focus glow |
| Chat Messages | ✅ | Gradient user bubbles, AI glass bubbles |
| Context Panel | ✅ | Spring entrance, circular avatar, gradient theme, char carousel |
| Landing Page | ✅ | Animated idle state with floating orbs |
| Gallery Page | ✅ | Masonry layout, gradient cards, staggered entrance |
| Models Page | ✅ | Glassmorphic provider cards, gradient header |
| Settings Page | ✅ | Gradient toggles, glass sections, premium dropdowns |
| Toast Notifications | ✅ | Backdrop-blur, gradient border glow, spring anim |
| Skeleton Loaders | ✅ | Purple shimmer gradient |
| Scrollbars | ✅ | 4px, purple-tinted globally |
| Light Theme | ✅ | Full CSS variable override system |
| Fonts | ✅ | Inter 400-800 + Geist Mono via Google Fonts |
| Quantum Timeline Navigator | ✅ | Animated dot-track, glow dots, Timeline Shift frosted-glass overlay with particle burst |
| EmotionHUD | 🟡 Not yet verified | Animated pill: colour-coded glow dot, emotion label, 3-bar mood/trust/arousal meter — pending user confirmation it renders |

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
| **Message Branching** | Sibling navigator arrows on bubbles | `get_message_branch`, `get_message_siblings` | Navigate alternate responses |
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
| **Local Storage Only** | ✅ | Privacy guard with confirmation dialog, `isLocalOnly()` utility for feature gating |


## 🔴 Not Implemented

| Feature | Status |
|---|---|
| **Video Generation** | Provider type `video` in schema, **no adapter** in Rust |
| **Image Provider Adapters** | `ImageProvider` trait defined, **no concrete adapter** (SiliconFlow/ComfyUI) |
| **Lorebook Search/Filter** | No search within lorebook entries |
| **Character Export** | No export-to-PNG/JSON feature |
| **Multi-character Chat** | No group/multi-char support |
| **Auto-save Memories** | Toggle exists, **no extraction pipeline** |
| **Auto-generate Images** | Toggle exists, **no auto-trigger logic** |

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
| `get_avatar_path` | ✅ | ✅ |
| `create_conversation` | ✅ | ✅ |
| `get_conversation` | ❌ | ✅ |
| `list_conversations` | ✅ | ✅ |
| `delete_conversation` | ✅ | ✅ |
| `update_conversation` | ✅ | ✅ |
| `get_conversation_messages` | ✅ | ✅ |
| `set_active_message` | ✅ | ✅ |
| `create_message` | ❌ (backend internal) | ✅ |
| `update_message` | ❌ **No edit UI** | ✅ |
| `delete_message` | ❌ **No delete UI** | ✅ |
| `get_message_branch` | ❌ | ✅ |
| `get_message_siblings` | ❌ | ✅ |
| `create_provider` | ✅ | ✅ |
| `get_provider` | ✅ | ✅ |
| `list_providers` | ✅ | ✅ |
| `update_provider` | ✅ | ✅ |
| `delete_provider` | ✅ | ✅ |
| `set_default_provider` | ✅ | ✅ |
| `test_provider_connection` | ✅ | ✅ |
| `send_message` | ✅ | ✅ |
| `regenerate_message` | ❌ **No UI** | ✅ |
| `generate_scene` | ❌ **No trigger UI** | ✅ |
| `list_scenes` | ✅ | ✅ |
| `delete_scene` | ❌ | ✅ |
| `get_scene_path` | ✅ | ✅ |
| `list_lorebook_entries` | ✅ | ✅ |
| `create_lorebook_entry` | ✅ | ✅ |
| `toggle_lorebook_entry` | ✅ | ✅ |
| `delete_lorebook_entry` | ✅ | ✅ |

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
- [x] Add Edit (pencil) button on user bubbles with inline textarea — already wired
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

## Task 8: Code Cleanup 🟢
- [x] Toast notifications wired for all error paths
- [x] Gate mock data behind `import.meta.env.DEV` check
- [x] Fix TypeScript/Vite build warnings (zero warnings in vite build)

---

# Part 3: UI Redesign Status

## ✅ All Pages Complete

| Component | Status | Aesthetic |
|---|---|---|
| Sidebar | ✅ | Glassmorphic, circular avatars, search, glow nav |
| Chat Header | ✅ | Gradient accents, circular avatar, ring glow |
| Chat Input | ✅ | Glassmorphic container, animated focus glow |
| Chat Messages | ✅ | Gradient user bubbles, AI glass bubbles |
| Context Panel | ✅ | Spring entrance, circular avatar, gradient theme |
| Landing Page | ✅ | Animated idle state with floating orbs |
| Gallery Page | ✅ | Masonry layout, gradient cards, staggered entrance |
| Models Page | ✅ | Glassmorphic provider cards, gradient header |
| Settings Page | ✅ | Gradient toggles, glass sections, premium dropdowns |
| Toast Notifications | ✅ | Backdrop-blur, gradient border glow, spring anim |
| Skeleton Loaders | ✅ | Purple shimmer gradient |
| Scrollbars | ✅ | 4px, purple-tinted globally |
| Light Theme | ✅ | Full CSS variable override system |
| Fonts | ✅ | Inter 400-800 + Geist Mono via Google Fonts |

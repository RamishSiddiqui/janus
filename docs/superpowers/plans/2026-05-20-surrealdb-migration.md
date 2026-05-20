# SurrealDB Migration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Mythic's SQLite/SQLx database layer with SurrealDB (embedded, RocksDB), using a repository pattern. Memories become graph-native.

**Architecture:** Repository pattern — each domain gets a repo module in `db/` that owns its SurrealQL queries. Command handlers become thin validators that delegate to repos. Schema is defined via idempotent DEFINE statements, not migration files.

**Tech Stack:** SurrealDB 2.x (embedded, RocksDB), surrealdb crate, Rust, Tauri 2

**Design Spec:** `docs/superpowers/specs/2026-05-20-surrealdb-migration-design.md`

---

### Task 1: Foundation — Dependencies, Errors, AppState

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/error.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/db/mod.rs` (replace existing)

- [ ] **Step 1: Update Cargo.toml**

Remove `sqlx` and add `surrealdb`:

```toml
# Remove this line:
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "migrate"] }

# Add this line:
surrealdb = { version = "2", features = ["kv-rocksdb"] }
```

- [ ] **Step 2: Update error.rs**

Replace sqlx error variants with surrealdb:

```rust
// Replace these two variants:
// #[error("Database error: {0}")]
// Database(#[from] sqlx::Error),
// #[error("Migration error: {0}")]
// Migration(#[from] sqlx::migrate::MigrateError),

// With this single variant:
#[error("Database error: {0}")]
Database(#[from] surrealdb::Error),

// Also add a generic string variant for DB errors that need context:
#[error("Database operation failed: {0}")]
DatabaseOp(String),
```

Remove any `use sqlx` imports. Keep all other variants unchanged.

- [ ] **Step 3: Rewrite db/mod.rs**

Replace the entire file. The new init connects SurrealDB embedded with RocksDB:

```rust
use std::path::Path;
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, RocksDb};
use tracing::info;

use crate::error::MythicError;

pub mod schema;
pub mod seed;
pub mod characters;
pub mod conversations;
pub mod messages;
pub mod memories;
pub mod providers;
pub mod scenes;
pub mod lorebook;
pub mod character_state;

/// Initializes the SurrealDB embedded database with RocksDB persistence.
pub async fn init_database(data_dir: &Path) -> Result<Surreal<Db>, MythicError> {
    let db_path = data_dir.join("mythic_surreal");
    info!("Initializing SurrealDB at: {:?}", db_path);

    // Ensure directory exists
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Connect embedded with RocksDB persistence
    let db = Surreal::new::<RocksDb>(&db_path).await?;

    // Select namespace and database
    db.use_ns("mythic").use_db("mythic").await?;

    // Bootstrap schema (idempotent)
    schema::define_schema(&db).await?;

    // Seed defaults if empty
    seed::seed_defaults(&db).await?;

    info!("SurrealDB initialized successfully");
    Ok(db)
}
```

- [ ] **Step 4: Update lib.rs AppState and setup**

Replace `sqlx::Pool<Sqlite>` with `Surreal<Db>`:

```rust
// Replace import:
// use sqlx::{Pool, Sqlite};
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

pub struct AppState {
    /// SurrealDB embedded connection
    pub db: Surreal<Db>,
    /// HTTP client shared across all providers
    pub http_client: reqwest::Client,
}
```

In the `setup` closure, replace the database initialization:

```rust
// Replace:
// let db_path = app_data_dir.join("mythic.db");
// let pool = rt.block_on(async {
//     db::init_database(&db_path).await
// }).expect("Failed to initialize database");

// With:
let db = rt.block_on(async {
    db::init_database(&app_data_dir).await
}).expect("Failed to initialize SurrealDB");
```

And update state creation:

```rust
let state = AppState {
    db,  // was: pool
    http_client,
};
```

- [ ] **Step 5: Verify compilation structure**

At this point the project won't compile (command files still reference sqlx). That's expected. Create placeholder (empty) files for all repo modules so `db/mod.rs` doesn't fail on module declarations:

```bash
# Create empty repo files
touch src-tauri/src/db/schema.rs
touch src-tauri/src/db/seed.rs
touch src-tauri/src/db/characters.rs
touch src-tauri/src/db/conversations.rs
touch src-tauri/src/db/messages.rs
touch src-tauri/src/db/memories.rs
touch src-tauri/src/db/providers.rs
touch src-tauri/src/db/scenes.rs
touch src-tauri/src/db/lorebook.rs
touch src-tauri/src/db/character_state.rs
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: foundation — swap sqlx for surrealdb, update AppState and db init"
```

---

### Task 2: Schema Definition

**Files:**
- Create: `src-tauri/src/db/schema.rs`

- [ ] **Step 1: Write schema.rs with all DEFINE statements**

Create `db/schema.rs` containing all table, field, index, analyzer, and event definitions. This runs on every startup (idempotent via `DEFINE ... IF NOT EXISTS` semantics — SurrealDB's DEFINE is already idempotent by default).

The schema must define ALL 9 tables plus the `memory_link` graph edge table:
- `characters` — with native `object` for data field
- `conversations` — with `record<T>` foreign keys
- `messages` — with FTS search analyzer + index
- `memories` — with graph-ready structure
- `memory_link` — graph edge table with enforce event
- `lorebook_entries` — with native `array<string>` for keys
- `provider_configs` — with native `object` for config
- `enabled_models` — with unique composite index
- `scenes` — with `record<T>` foreign keys
- `character_states` — with unique composite index

See design spec Section 5 for the complete SurrealQL for each table.

The function signature:
```rust
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use crate::error::MythicError;

pub async fn define_schema(db: &Surreal<Db>) -> Result<(), MythicError> {
    // Execute all DEFINE statements as a single multi-statement query
    db.query("
        -- Characters
        DEFINE TABLE characters SCHEMAFULL;
        DEFINE FIELD name ON characters TYPE string;
        ...
        
        -- Conversations
        DEFINE TABLE conversations SCHEMAFULL;
        ...
    ").await?;
    
    Ok(())
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/db/schema.rs
git commit -m "feat: define SurrealDB schema for all tables"
```

---

### Task 3: Seed Data

**Files:**
- Create: `src-tauri/src/db/seed.rs`

- [ ] **Step 1: Write seed.rs**

Port the seed data from SQLite migrations `003_seed_defaults.sql` and `008_seed_memory_test_data.sql`. The function should check if data exists before seeding (idempotent):

```rust
pub async fn seed_defaults(db: &Surreal<Db>) -> Result<(), MythicError> {
    // Check if providers already seeded
    let count: Option<usize> = db.query("SELECT count() FROM provider_configs GROUP ALL")
        .await?.take("count")?;
    
    if count.unwrap_or(0) == 0 {
        seed_providers(db).await?;
        seed_characters(db).await?;
    }
    Ok(())
}
```

Seed the 3 default providers (OpenRouter, SiliconFlow Image, SiliconFlow Video) and 10 default characters (6 College of Magic + 4 Neon Shadows) with their full CharacterData as native objects.

Reference the original seed SQL in migration files `003_seed_defaults.sql` for the exact character data JSON.

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/db/seed.rs
git commit -m "feat: seed default providers and characters for SurrealDB"
```

---

### Task 4: Update Models

**Files:**
- Modify: `src-tauri/src/models/character.rs`
- Modify: `src-tauri/src/models/conversation.rs`
- Modify: `src-tauri/src/models/memory.rs`
- Modify: `src-tauri/src/models/provider.rs`
- Modify: `src-tauri/src/models/scene.rs`
- Modify: `src-tauri/src/models/lorebook.rs`

- [ ] **Step 1: Update all models for SurrealDB compatibility**

Key changes across all models:

1. **ID fields**: Change from `String` to `surrealdb::sql::Thing`, but add a serialization helper that converts `Thing` to a plain string for frontend IPC:

```rust
use serde::{Serialize, Deserialize, Serializer};
use surrealdb::sql::Thing;

fn serialize_thing_as_string<S>(thing: &Thing, serializer: S) -> Result<S::Ok, S::Error>
where S: Serializer {
    serializer.serialize_str(&thing.id.to_raw())
}

// Helper for Option<Thing>
fn serialize_option_thing<S>(thing: &Option<Thing>, serializer: S) -> Result<S::Ok, S::Error>
where S: Serializer {
    match thing {
        Some(t) => serializer.serialize_some(&t.id.to_raw()),
        None => serializer.serialize_none(),
    }
}
```

2. **Native JSON fields**: Change `String` → `serde_json::Value` for `Character.data`, `ProviderConfig.config`
3. **Native arrays**: Change lorebook `keys` from `String` → `Vec<String>`
4. **Remove `sqlx::FromRow`** derives from `CharacterState`

Each model struct needs `#[derive(Serialize, Deserialize, Debug, Clone)]` and SurrealDB-compatible field types.

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/models/
git commit -m "feat: update models for SurrealDB — native JSON, Thing IDs"
```

---

### Task 5: CharacterRepo + Commands

**Files:**
- Create: `src-tauri/src/db/characters.rs`
- Modify: `src-tauri/src/commands/characters.rs`

- [ ] **Step 1: Implement CharacterRepo**

```rust
// db/characters.rs
pub struct CharacterRepo;

impl CharacterRepo {
    pub async fn create(db: &Surreal<Db>, name: &str, data: serde_json::Value) -> Result<Character, MythicError>;
    pub async fn get(db: &Surreal<Db>, id: &str) -> Result<Character, MythicError>;
    pub async fn list(db: &Surreal<Db>) -> Result<Vec<Character>, MythicError>;
    pub async fn update(db: &Surreal<Db>, id: &str, name: Option<&str>, data: Option<serde_json::Value>, avatar_path: Option<&str>) -> Result<Character, MythicError>;
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError>;
}
```

Each method maps to the SurrealQL equivalent of the current SQL queries. Use `db.create()`, `db.select()`, `db.update()`, `db.delete()` typed API where possible, fall back to `db.query()` for complex queries.

- [ ] **Step 2: Rewrite commands/characters.rs**

Replace all `sqlx::query()` calls with `CharacterRepo::method()` calls. Keep the same `#[tauri::command]` signatures so the frontend doesn't change. Remove all `use sqlx` imports.

The state access pattern changes from:
```rust
let db = state_guard.db.clone(); // sqlx Pool is cloneable
```
To:
```rust
let db = &state_guard.db; // Surreal<Db> — use reference
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/characters.rs src-tauri/src/commands/characters.rs
git commit -m "feat: character repo + commands on SurrealDB"
```

---

### Task 6: ConversationRepo + Commands

**Files:**
- Create: `src-tauri/src/db/conversations.rs`
- Modify: `src-tauri/src/commands/conversations.rs`

- [ ] **Step 1: Implement ConversationRepo**

Methods needed:
```rust
pub struct ConversationRepo;

impl ConversationRepo {
    pub async fn create(db, character_id: Option<&str>, title: Option<&str>) -> Result<Conversation>;
    pub async fn get(db, id: &str) -> Result<Conversation>;
    pub async fn list(db, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<Conversation>>;
    pub async fn count(db) -> Result<u32>;
    pub async fn delete(db, id: &str) -> Result<()>;
    pub async fn get_messages(db, conversation_id: &str) -> Result<Vec<Message>>;
    pub async fn set_active_message(db, conversation_id: &str, message_id: &str) -> Result<()>;
    pub async fn update(db, id: &str, title: &str) -> Result<Conversation>;
    pub async fn set_memory_scope(db, conversation_id: &str, scope: &str) -> Result<()>;
    pub async fn branch(db, parent_id: &str, branch_point_msg_id: &str, title: Option<&str>) -> Result<Conversation>;
    pub async fn search_messages(db, query: &str, limit: Option<u32>) -> Result<Vec<SearchResult>>;
}
```

Key conversions:
- `GROUP_CONCAT(DISTINCT ...)` for `shared_character_ids` → SurrealQL subquery with `array::distinct()`
- FTS5 `MATCH` → SurrealDB `@@ ` search operator
- `snippet()` → `search::highlight()`
- Branch conversation logic: copy messages + copy memories + create `RELATE` edges

- [ ] **Step 2: Rewrite commands/conversations.rs to use repo**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/conversations.rs src-tauri/src/commands/conversations.rs
git commit -m "feat: conversation repo + commands on SurrealDB"
```

---

### Task 7: MessageRepo + Commands

**Files:**
- Create: `src-tauri/src/db/messages.rs`
- Modify: `src-tauri/src/commands/messages.rs`

- [ ] **Step 1: Implement MessageRepo**

```rust
pub struct MessageRepo;

impl MessageRepo {
    pub async fn create(db, conversation_id, role, content, parent_id?, metadata?) -> Result<Message>;
    pub async fn get(db, id: &str) -> Result<Message>;
    pub async fn update(db, id: &str, content: &str) -> Result<Message>;
    pub async fn delete(db, id: &str) -> Result<()>;
    pub async fn get_branch(db, message_id: &str) -> Result<Vec<Message>>;
    pub async fn get_siblings(db, message_id: &str) -> Result<Vec<Message>>;
}
```

The `get_branch` method (walk parent chain) can be simplified with SurrealQL's recursive graph-like traversal, or implemented as an iterative loop.

- [ ] **Step 2: Rewrite commands/messages.rs**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/messages.rs src-tauri/src/commands/messages.rs
git commit -m "feat: message repo + commands on SurrealDB"
```

---

### Task 8: MemoryRepo + Commands (Graph-Native)

**Files:**
- Create: `src-tauri/src/db/memories.rs`
- Modify: `src-tauri/src/commands/memories.rs`

This is the most complex task — it uses SurrealDB's graph features.

- [ ] **Step 1: Implement MemoryRepo with graph operations**

```rust
pub struct MemoryRepo;

impl MemoryRepo {
    pub async fn list(db, character_id?, conversation_id?) -> Result<Vec<Memory>>;
    pub async fn create(db, character_id?, conversation_id?, content, source?) -> Result<Memory>;
    pub async fn update(db, id: &str, content: &str) -> Result<Memory>;
    pub async fn delete(db, id: &str) -> Result<()>;
    pub async fn promote_to_canon(db, id: &str) -> Result<Memory>;
    
    // Graph operations
    pub async fn share(db, source_memory_id, target_conversation_id, link_type, direction, sync_mode) -> Result<MemoryLink>;
    pub async fn unlink(db, link_id: &str) -> Result<()>;
    pub async fn get_graph(db, character_id: &str) -> Result<MemoryGraph>;
    
    // For chat context building
    pub async fn list_for_context(db, character_id?, conversation_id?, scope: &str) -> Result<Vec<Memory>>;
}
```

Key graph operations:
- `share()` → Uses `RELATE memories:source -> memory_link -> conversations:target SET ...`
- `unlink()` → `DELETE memory_link WHERE id = $id`
- `get_graph()` → Single query with `->memory_link->conversations` graph traversal

- [ ] **Step 2: Rewrite commands/memories.rs**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/memories.rs src-tauri/src/commands/memories.rs
git commit -m "feat: memory repo with graph-native RELATE edges on SurrealDB"
```

---

### Task 9: ProviderRepo + Commands

**Files:**
- Create: `src-tauri/src/db/providers.rs`
- Modify: `src-tauri/src/commands/providers.rs`

- [ ] **Step 1: Implement ProviderRepo**

```rust
pub struct ProviderRepo;

impl ProviderRepo {
    pub async fn create(db, name, provider_type, adapter, config, is_default?) -> Result<ProviderConfig>;
    pub async fn get(db, id: &str) -> Result<ProviderConfig>;
    pub async fn list(db, provider_type: Option<&str>) -> Result<Vec<ProviderConfig>>;
    pub async fn update(db, id, name?, config?) -> Result<ProviderConfig>;
    pub async fn delete(db, id: &str) -> Result<()>;
    pub async fn set_default(db, id: &str) -> Result<()>;
    pub async fn toggle_model_enabled(db, provider_id, model_id, model_type, enabled) -> Result<()>;
    pub async fn list_enabled_models(db, provider_id: Option<&str>) -> Result<Vec<ModelEntry>>;
}
```

Key conversion: `INSERT ... ON CONFLICT DO UPDATE` → SurrealDB `UPSERT` or `INSERT ... ON DUPLICATE KEY UPDATE`.

- [ ] **Step 2: Rewrite commands/providers.rs**

Note: `test_provider_connection`, `list_provider_models`, and `list_all_models` have HTTP logic that stays — only the DB queries change.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/providers.rs src-tauri/src/commands/providers.rs
git commit -m "feat: provider repo + commands on SurrealDB"
```

---

### Task 10: Remaining Repos (Scenes, Lorebook, CharacterState)

**Files:**
- Create: `src-tauri/src/db/scenes.rs`
- Create: `src-tauri/src/db/lorebook.rs`
- Create: `src-tauri/src/db/character_state.rs`
- Modify: `src-tauri/src/commands/scenes.rs`
- Modify: `src-tauri/src/commands/lorebook.rs`
- Modify: `src-tauri/src/commands/character_state.rs`

- [ ] **Step 1: Implement SceneRepo**

Simple CRUD: `create`, `list`, `delete`, `get_file_path`.

- [ ] **Step 2: Implement LorebookRepo**

Normalize the lorebook module to use the same `Arc<RwLock<AppState>>` pattern as everything else (it currently uses `State<'_, SqlitePool>` directly):

```rust
pub struct LorebookRepo;

impl LorebookRepo {
    pub async fn list(db, character_id: &str) -> Result<Vec<LorebookEntry>>;
    pub async fn create(db, character_id?, name, keys, content, always_active) -> Result<LorebookEntry>;
    pub async fn toggle(db, id: &str, enabled: bool) -> Result<()>;
    pub async fn delete(db, id: &str) -> Result<()>;
    // For chat context building:
    pub async fn list_active(db, character_id: &str) -> Result<Vec<LorebookEntry>>;
    pub async fn list_keyword_entries(db, character_id: &str) -> Result<Vec<LorebookEntry>>;
}
```

- [ ] **Step 3: Implement CharacterStateRepo**

```rust
pub struct CharacterStateRepo;

impl CharacterStateRepo {
    pub async fn get(db, character_id: &str, conversation_id: &str) -> Result<Option<CharacterState>>;
    pub async fn upsert(db, character_id, conversation_id, mood, trust, arousal, dominant_emotion, state_summary) -> Result<CharacterState>;
}
```

The UPSERT uses SurrealDB's `UPSERT` statement or `INSERT ... ON DUPLICATE KEY UPDATE`.

- [ ] **Step 4: Rewrite all 3 command files**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db/scenes.rs src-tauri/src/db/lorebook.rs src-tauri/src/db/character_state.rs
git add src-tauri/src/commands/scenes.rs src-tauri/src/commands/lorebook.rs src-tauri/src/commands/character_state.rs
git commit -m "feat: scene, lorebook, character_state repos + commands on SurrealDB"
```

---

### Task 11: Chat Command Rewrite

**Files:**
- Modify: `src-tauri/src/commands/chat.rs`

- [ ] **Step 1: Rewrite build_prompt() to use repos**

Replace all inline SQL queries with repo calls:

```rust
async fn build_prompt(
    db: &Surreal<Db>,  // was: &sqlx::Pool<Sqlite>
    conversation_id: &str,
    up_to_message_id: &str,
    user_system_prompt: Option<&str>,
    post_history_instructions: Option<&str>,
) -> Result<Vec<ChatMessage>, MythicError> {
    // 1. Get conversation metadata
    let conv = ConversationRepo::get(db, conversation_id).await?;
    
    // 2. Get character data (native object, no parsing needed!)
    if let Some(ref char_id) = conv.character_id {
        let character = CharacterRepo::get(db, char_id).await?;
        // character.data is already serde_json::Value — no from_str needed
    }
    
    // 3. Get lorebook entries
    let active_entries = LorebookRepo::list_active(db, char_id).await?;
    
    // 4. Walk message tree
    let chain = MessageRepo::get_branch(db, up_to_message_id).await?;
    
    // 5. Get keyword-triggered lorebook
    let keyword_entries = LorebookRepo::list_keyword_entries(db, char_id).await?;
    
    // 6. Get memories
    let memories = MemoryRepo::list_for_context(db, char_id, conversation_id, &conv.memory_scope).await?;
    
    // 7. Get emotional state
    let state = CharacterStateRepo::get(db, char_id, conversation_id).await?;
    
    // ... assemble prompt (same logic as before)
}
```

- [ ] **Step 2: Update send_message, retry_failed_message, regenerate_message**

Replace `sqlx::query()` calls for:
- Inserting user/assistant messages → `MessageRepo::create()`
- Updating active_message_id → `ConversationRepo::set_active_message()`
- Getting provider config → `ProviderRepo::get_default_llm()`
- Updating message content → `MessageRepo::update()`

Keep the Rig provider creation and streaming logic unchanged.

- [ ] **Step 3: Update generate_raw**

Replace provider lookup with `ProviderRepo::get_default_llm()`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/chat.rs
git commit -m "feat: rewrite chat command to use repos — zero inline SQL"
```

---

### Task 12: Import Command

**Files:**
- Modify: `src-tauri/src/commands/import.rs`

- [ ] **Step 1: Rewrite import to use CharacterRepo**

Replace the two `sqlx::query()` calls with:
```rust
let character = CharacterRepo::create_with_avatar(db, &name, data, Some(&avatar_rel)).await?;
```

The PNG parsing / avatar extraction logic stays identical.

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/commands/import.rs
git commit -m "feat: import command on SurrealDB"
```

---

### Task 13: Cleanup — Remove SQLite Artifacts

**Files:**
- Delete: `src-tauri/migrations/` (entire directory — 13 SQL files)
- Verify: No remaining `use sqlx` or `sqlx::` references anywhere

- [ ] **Step 1: Delete migrations directory**

```bash
rm -rf src-tauri/migrations/
```

- [ ] **Step 2: Grep for any remaining sqlx references**

```bash
grep -r "sqlx" src-tauri/src/ --include="*.rs"
```

Fix any remaining references.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore: remove SQLite migration files, clean up sqlx references"
```

---

### Task 14: Build + Verify

- [ ] **Step 1: Cargo build**

```bash
cd src-tauri && cargo build 2>&1
```

Fix any compilation errors. This is the main verification — if it compiles, the type system guarantees most correctness.

- [ ] **Step 2: Cargo clippy**

```bash
cd src-tauri && cargo clippy 2>&1
```

Fix any warnings.

- [ ] **Step 3: Frontend check**

```bash
npm run check
```

Verify zero TypeScript errors — since we didn't change any IPC signatures, this should pass unchanged.

- [ ] **Step 4: Launch and smoke test**

```bash
npm run tauri dev
```

Verify:
1. App launches without errors
2. Gallery shows 10 seed characters
3. Can create a new conversation
4. Can send a message and get a streaming response
5. Can search messages
6. Can create/share/unlink memories
7. Memory graph view works

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: SurrealDB migration complete — all systems verified"
```

---

## Execution Order & Dependencies

```
Task 1 (Foundation) ──→ Task 2 (Schema) ──→ Task 3 (Seed)
                                              │
Task 4 (Models) ──────────────────────────────┤
                                              │
                              ┌───────────────┼───────────────┐
                              ▼               ▼               ▼
                        Task 5-9         Task 10          Task 11-12
                      (Major repos)    (Minor repos)    (Chat + Import)
                              │               │               │
                              └───────────────┼───────────────┘
                                              ▼
                                    Task 13 (Cleanup)
                                              ▼
                                    Task 14 (Build + Verify)
```

Tasks 1-4 are sequential (each depends on the previous).
Tasks 5-12 can be done in any order after Task 4 (they're independent domain repos).
Tasks 13-14 must be last.

# SurrealDB Migration — Design Spec

## Goal

Replace Mythic's entire SQLite/SQLx database layer with SurrealDB (embedded, RocksDB backend), using a repository pattern architecture. Memories become graph-native with RELATE edges. Fresh start — no data migration from existing SQLite.

## Decisions (User-Approved)

| Decision | Choice |
|----------|--------|
| Database | SurrealDB embedded, RocksDB backend |
| Migration strategy | Big-bang rewrite |
| Data migration | Fresh start, no SQLite import |
| Memory graph | Native graph edges from day one |
| Architecture | Repository pattern (Approach B) |
| Branch | `feat/surrealdb-migration` |

---

## 1. Architecture Overview

### Current (SQLite/SQLx)

```
commands/*.rs  ──(raw SQL strings)──>  sqlx::Pool<Sqlite>  ──>  mythic.db
```

- SQL scattered across 9 command files (~80 queries)
- Models define structs but don't own their queries
- Lorebook uses inconsistent state pattern (`SqlitePool` instead of `Arc<RwLock<AppState>>`)

### Target (SurrealDB)

```
commands/*.rs  ──(method calls)──>  db/*.rs repos  ──(SurrealQL)──>  Surreal<Db>  ──>  RocksDB
```

- Commands become thin: validate input → call repo → return result
- Each repo owns its SurrealQL queries
- All repos share the same `Surreal<Db>` connection
- Models are shared between commands and repos

### File Structure

```
src-tauri/src/
├── db/
│   ├── mod.rs                ← init_database(), Surreal<Db> setup, schema bootstrap
│   ├── schema.rs             ← All DEFINE TABLE/FIELD/INDEX/EVENT statements
│   ├── seed.rs               ← Default providers + seed characters
│   ├── characters.rs         ← CharacterRepo
│   ├── conversations.rs      ← ConversationRepo
│   ├── messages.rs           ← MessageRepo
│   ├── memories.rs           ← MemoryRepo (graph-native with RELATE)
│   ├── providers.rs          ← ProviderRepo
│   ├── scenes.rs             ← SceneRepo
│   ├── lorebook.rs           ← LorebookRepo
│   └── character_state.rs    ← CharacterStateRepo
├── commands/                 ← Thin handlers (validate → repo call → return)
│   ├── characters.rs
│   ├── conversations.rs
│   ├── messages.rs
│   ├── memories.rs
│   ├── providers.rs
│   ├── scenes.rs
│   ├── lorebook.rs
│   ├── character_state.rs
│   ├── chat.rs
│   └── import.rs
├── models/                   ← Shared structs (Serialize + Deserialize)
│   ├── character.rs
│   ├── conversation.rs
│   ├── memory.rs
│   ├── provider.rs
│   ├── scene.rs
│   └── lorebook.rs
├── providers/                ← Rig LLM layer (unchanged)
├── error.rs                  ← Updated: surrealdb::Error instead of sqlx::Error
└── lib.rs                    ← AppState: Surreal<Db> instead of Pool<Sqlite>
```

---

## 2. Dependencies (Cargo.toml)

### Remove
```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "migrate"] }
```

### Add
```toml
surrealdb = { version = "2", features = ["kv-rocksdb"] }
```

### Keep (unchanged)
All other dependencies remain: `rig-core`, `tauri`, `serde`, `tokio`, `uuid`, `chrono`, etc.

---

## 3. AppState Changes (`lib.rs`)

### Before
```rust
pub struct AppState {
    pub db: Pool<Sqlite>,
    pub http_client: reqwest::Client,
}
```

### After
```rust
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

pub struct AppState {
    pub db: Surreal<Db>,
    pub http_client: reqwest::Client,
}
```

### Initialization (setup block)
```rust
// Before: db::init_database(&db_path).await
// After:
let db = db::init_database(&app_data_dir).await
    .expect("Failed to initialize SurrealDB");
```

---

## 4. Database Initialization (`db/mod.rs`)

```rust
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, RocksDb};

pub async fn init_database(data_dir: &Path) -> Result<Surreal<Db>, MythicError> {
    let db_path = data_dir.join("mythic_surreal");
    
    // Connect embedded with RocksDB persistence
    let db = Surreal::new::<RocksDb>(db_path).await?;
    
    // Select namespace and database
    db.use_ns("mythic").use_db("mythic").await?;
    
    // Bootstrap schema (idempotent DEFINE statements)
    schema::define_schema(&db).await?;
    
    // Seed defaults if empty
    seed::seed_defaults(&db).await?;
    
    Ok(db)
}
```

Key difference from SQLite: No migration files. SurrealDB uses idempotent `DEFINE TABLE/FIELD/INDEX` statements that are safe to re-run on every startup. Schema evolution is handled by adding new DEFINE statements (existing data is preserved).

---

## 5. Schema Design (`db/schema.rs`)

### 5.1 Characters

```surql
DEFINE TABLE characters SCHEMAFULL;
DEFINE FIELD name       ON characters TYPE string ASSERT $value != NONE;
DEFINE FIELD spec       ON characters TYPE string DEFAULT "chara_card_v2";
DEFINE FIELD data       ON characters TYPE object;          -- Native JSON, not a string!
DEFINE FIELD avatar_path ON characters TYPE option<string>;
DEFINE FIELD created_at ON characters TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON characters TYPE datetime DEFAULT time::now();

DEFINE INDEX idx_characters_updated ON characters FIELDS updated_at;
```

**Key change**: `data` is now a native `object` — no more `serde_json::from_str()` parsing. Character card fields are directly queryable:
```surql
SELECT data.name, data.personality FROM characters WHERE data.tags CONTAINS "fantasy";
```

### 5.2 Conversations

```surql
DEFINE TABLE conversations SCHEMAFULL;
DEFINE FIELD title                    ON conversations TYPE string;
DEFINE FIELD character_id             ON conversations TYPE option<record<characters>>;
DEFINE FIELD active_message_id        ON conversations TYPE option<record<messages>>;
DEFINE FIELD memory_scope             ON conversations TYPE string DEFAULT "character"
    ASSERT $value IN ["character", "conversation", "none"];
DEFINE FIELD parent_conversation_id   ON conversations TYPE option<record<conversations>>;
DEFINE FIELD branch_point_message_id  ON conversations TYPE option<record<messages>>;
DEFINE FIELD created_at               ON conversations TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at               ON conversations TYPE datetime DEFAULT time::now();

DEFINE INDEX idx_conversations_character ON conversations FIELDS character_id;
DEFINE INDEX idx_conversations_updated   ON conversations FIELDS updated_at;
```

**Key change**: Foreign keys are `record<T>` types — SurrealDB enforces referential integrity natively.

### 5.3 Messages

```surql
DEFINE TABLE messages SCHEMAFULL;
DEFINE FIELD conversation_id ON messages TYPE record<conversations>;
DEFINE FIELD role             ON messages TYPE string
    ASSERT $value IN ["user", "assistant", "system"];
DEFINE FIELD content          ON messages TYPE string;
DEFINE FIELD parent_id        ON messages TYPE option<record<messages>>;
DEFINE FIELD metadata         ON messages TYPE option<object>;
DEFINE FIELD created_at       ON messages TYPE datetime DEFAULT time::now();

DEFINE INDEX idx_messages_conversation ON messages FIELDS conversation_id;
DEFINE INDEX idx_messages_parent       ON messages FIELDS parent_id;

-- Full-text search (replaces FTS5 + triggers)
DEFINE ANALYZER msg_analyzer TOKENIZERS unicode FILTERS lowercase, edgengram(2, 15);
DEFINE INDEX idx_messages_fts ON messages FIELDS content
    SEARCH ANALYZER msg_analyzer BM25;
```

**Key change**: Full-text search is a built-in index — no virtual tables, no triggers, no sync logic. Just `DEFINE INDEX ... SEARCH ANALYZER`.

### 5.4 Memories (Graph-Native)

```surql
DEFINE TABLE memories SCHEMAFULL;
DEFINE FIELD character_id    ON memories TYPE option<record<characters>>;
DEFINE FIELD conversation_id ON memories TYPE option<record<conversations>>;
DEFINE FIELD content          ON memories TYPE string;
DEFINE FIELD source           ON memories TYPE string DEFAULT "user"
    ASSERT $value IN ["user", "auto"];
DEFINE FIELD parent_id        ON memories TYPE option<record<memories>>;
DEFINE FIELD version          ON memories TYPE int DEFAULT 1;
DEFINE FIELD is_canon         ON memories TYPE bool DEFAULT false;
DEFINE FIELD created_at       ON memories TYPE datetime DEFAULT time::now();

DEFINE INDEX idx_memories_character    ON memories FIELDS character_id;
DEFINE INDEX idx_memories_conversation ON memories FIELDS conversation_id;

-- Graph edge: replaces memory_links join table
DEFINE TABLE memory_link SCHEMAFULL;
DEFINE FIELD link_type  ON memory_link TYPE string ASSERT $value IN ["copy", "sync"];
DEFINE FIELD direction  ON memory_link TYPE string ASSERT $value IN ["one_way", "two_way"];
DEFINE FIELD sync_mode  ON memory_link TYPE string ASSERT $value IN ["auto", "manual"];
DEFINE FIELD created_at ON memory_link TYPE datetime DEFAULT time::now();

-- Constraint: copy links must be one_way
DEFINE EVENT enforce_copy_direction ON memory_link WHEN $event = "CREATE" OR $event = "UPDATE" THEN {
    IF $after.link_type = "copy" AND $after.direction != "one_way" {
        THROW "Copy links must be one_way";
    };
};
```

**Key change**: `memory_links` becomes a **graph edge table** (`memory_link`). Instead of:
```sql
INSERT INTO memory_links (source_memory_id, target_conversation_id, ...) VALUES (?, ?, ...)
```
We use:
```surql
RELATE memories:source_id -> memory_link -> conversations:target_id
    SET link_type = "copy", direction = "one_way", sync_mode = "auto";
```

And graph traversal becomes:
```surql
-- Find all conversations a memory is shared to
SELECT ->memory_link->conversations FROM memories:mem_id;

-- Find all memories shared INTO a conversation
SELECT <-memory_link<-memories FROM conversations:conv_id;

-- Full memory graph for a character
SELECT *, ->memory_link->conversations AS shared_to
    FROM memories WHERE character_id = characters:char_id;
```

### 5.5 Lorebook

```surql
DEFINE TABLE lorebook_entries SCHEMAFULL;
DEFINE FIELD character_id    ON lorebook_entries TYPE option<record<characters>>;
DEFINE FIELD keys            ON lorebook_entries TYPE array<string>;    -- Native array, not JSON string!
DEFINE FIELD content         ON lorebook_entries TYPE string;
DEFINE FIELD enabled         ON lorebook_entries TYPE bool DEFAULT true;
DEFINE FIELD always_active   ON lorebook_entries TYPE bool DEFAULT false;
DEFINE FIELD priority        ON lorebook_entries TYPE int DEFAULT 10;
DEFINE FIELD insertion_order ON lorebook_entries TYPE int DEFAULT 100;
DEFINE FIELD name            ON lorebook_entries TYPE option<string>;

DEFINE INDEX idx_lorebook_character ON lorebook_entries FIELDS character_id;
```

**Key change**: `keys` is now a native `array<string>` — no more `serde_json::from_str::<Vec<String>>()` with CSV fallback.

### 5.6 Provider Configs

```surql
DEFINE TABLE provider_configs SCHEMAFULL;
DEFINE FIELD name          ON provider_configs TYPE string;
DEFINE FIELD provider_type ON provider_configs TYPE string
    ASSERT $value IN ["llm", "image", "video"];
DEFINE FIELD adapter       ON provider_configs TYPE string;
DEFINE FIELD config        ON provider_configs TYPE object;   -- Native object!
DEFINE FIELD is_default    ON provider_configs TYPE bool DEFAULT false;

DEFINE INDEX idx_provider_type ON provider_configs FIELDS provider_type;
```

### 5.7 Enabled Models

```surql
DEFINE TABLE enabled_models SCHEMAFULL;
DEFINE FIELD provider_id ON enabled_models TYPE record<provider_configs>;
DEFINE FIELD model_id    ON enabled_models TYPE string;
DEFINE FIELD model_type  ON enabled_models TYPE string DEFAULT "llm";
DEFINE FIELD enabled     ON enabled_models TYPE bool DEFAULT true;
DEFINE FIELD created_at  ON enabled_models TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at  ON enabled_models TYPE datetime DEFAULT time::now();

DEFINE INDEX idx_enabled_provider_model ON enabled_models FIELDS provider_id, model_id UNIQUE;
```

### 5.8 Scenes

```surql
DEFINE TABLE scenes SCHEMAFULL;
DEFINE FIELD conversation_id ON scenes TYPE record<conversations>;
DEFINE FIELD message_id      ON scenes TYPE option<record<messages>>;
DEFINE FIELD media_type      ON scenes TYPE string ASSERT $value IN ["image", "video"];
DEFINE FIELD prompt           ON scenes TYPE string;
DEFINE FIELD file_path        ON scenes TYPE string;
DEFINE FIELD caption          ON scenes TYPE option<string>;
DEFINE FIELD metadata         ON scenes TYPE option<object>;
DEFINE FIELD created_at       ON scenes TYPE datetime DEFAULT time::now();

DEFINE INDEX idx_scenes_conversation ON scenes FIELDS conversation_id;
```

### 5.9 Character States

```surql
DEFINE TABLE character_states SCHEMAFULL;
DEFINE FIELD character_id    ON character_states TYPE record<characters>;
DEFINE FIELD conversation_id ON character_states TYPE record<conversations>;
DEFINE FIELD mood             ON character_states TYPE int DEFAULT 50;
DEFINE FIELD trust            ON character_states TYPE int DEFAULT 50;
DEFINE FIELD arousal          ON character_states TYPE int DEFAULT 30;
DEFINE FIELD dominant_emotion ON character_states TYPE string DEFAULT "neutral";
DEFINE FIELD state_summary    ON character_states TYPE string DEFAULT "";
DEFINE FIELD updated_at       ON character_states TYPE datetime DEFAULT time::now();

DEFINE INDEX idx_charstate_unique ON character_states FIELDS character_id, conversation_id UNIQUE;
```

---

## 6. Repository Pattern

Each repository is a struct with methods. No trait abstraction needed (YAGNI) — just plain impl blocks.

### Example: CharacterRepo

```rust
// db/characters.rs
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use crate::models::character::Character;
use crate::error::MythicError;

pub struct CharacterRepo;

impl CharacterRepo {
    pub async fn create(db: &Surreal<Db>, name: &str, data: serde_json::Value) -> Result<Character, MythicError> {
        let character: Option<Character> = db
            .create("characters")
            .content(CreateCharacter { name, spec: "chara_card_v2", data })
            .await?;
        character.ok_or(MythicError::Database("Failed to create character".into()))
    }

    pub async fn get(db: &Surreal<Db>, id: &str) -> Result<Character, MythicError> { ... }
    pub async fn list(db: &Surreal<Db>) -> Result<Vec<Character>, MythicError> { ... }
    pub async fn update(db: &Surreal<Db>, id: &str, ...) -> Result<Character, MythicError> { ... }
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> { ... }
}
```

### Example: MemoryRepo (Graph-Native)

```rust
// db/memories.rs
pub struct MemoryRepo;

impl MemoryRepo {
    pub async fn share(
        db: &Surreal<Db>,
        source_memory_id: &str,
        target_conversation_id: &str,
        link_type: &str,
        direction: &str,
        sync_mode: &str,
    ) -> Result<MemoryLink, MythicError> {
        // Graph edge creation via RELATE
        let result = db.query(
            "RELATE $source -> memory_link -> $target
             SET link_type = $link_type, direction = $direction, sync_mode = $sync_mode"
        )
        .bind(("source", format!("memories:{}", source_memory_id)))
        .bind(("target", format!("conversations:{}", target_conversation_id)))
        .bind(("link_type", link_type))
        .bind(("direction", direction))
        .bind(("sync_mode", sync_mode))
        .await?;
        // ...
    }

    pub async fn get_graph(db: &Surreal<Db>, character_id: &str) -> Result<MemoryGraph, MythicError> {
        // Single graph query replaces 4 separate SQL queries
        let result = db.query(
            "SELECT *,
                ->memory_link->conversations AS shared_to
             FROM memories
             WHERE character_id = $char_id
             ORDER BY created_at ASC"
        )
        .bind(("char_id", format!("characters:{}", character_id)))
        .await?;
        // ...
    }
}
```

### Command Handler (Thin)

```rust
// commands/characters.rs — BEFORE (embedded SQL)
pub async fn create_character(state: ..., name: String, data: Value) -> Result<Character> {
    let db = state.read().await.db.clone();
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO characters (id, name, spec, data) VALUES (?, ?, ?, ?)")
        .bind(&id).bind(&name).bind("chara_card_v2").bind(&data_str)
        .execute(&db).await?;
    // ... fetch back, parse, return
}

// commands/characters.rs — AFTER (repo call)
pub async fn create_character(state: ..., name: String, data: Value) -> Result<Character> {
    validate_required_string("name", &name, 200)?;
    let db = &state.read().await.db;
    CharacterRepo::create(db, &name, data).await
}
```

---

## 7. Model Changes

### Native JSON fields (no more string parsing)

| Field | Before (SQLite) | After (SurrealDB) |
|-------|----------------|-------------------|
| `characters.data` | `String` (JSON string) | `serde_json::Value` (native object) |
| `provider_configs.config` | `String` (JSON string) | `serde_json::Value` (native object) |
| `messages.metadata` | `Option<String>` (JSON string) | `Option<serde_json::Value>` (native object) |
| `scenes.metadata` | `Option<String>` (JSON string) | `Option<serde_json::Value>` (native object) |
| `lorebook_entries.keys` | `String` (JSON array string) | `Vec<String>` (native array) |

### Record IDs

SurrealDB uses `table:id` format for record IDs. Models need to handle this:

```rust
use surrealdb::sql::Thing;

pub struct Character {
    pub id: Thing,           // "characters:abc123"
    pub name: String,
    pub data: serde_json::Value,  // Native object now
    // ...
}
```

The `Thing` type serializes as `{ "tb": "characters", "id": "abc123" }`. For frontend compatibility, we'll serialize IDs as plain strings in the Tauri command responses.

---

## 8. Error Handling (`error.rs`)

### Replace
```rust
#[error("Database error: {0}")]
Database(#[from] sqlx::Error),

#[error("Migration error: {0}")]
Migration(#[from] sqlx::migrate::MigrateError),
```

### With
```rust
#[error("Database error: {0}")]
Database(#[from] surrealdb::Error),
```

No separate migration error — SurrealDB doesn't use file-based migrations.

---

## 9. Full-Text Search Migration

### Before (SQLite FTS5)
- Virtual table `messages_fts` with 3 auto-sync triggers
- `snippet()` function for highlighted results
- `MATCH` operator + `rank` for relevance

### After (SurrealDB Search)
- Built-in search analyzer on `messages.content` field
- No triggers needed — index auto-syncs
- `search::highlight()` and `search::score()` functions

```surql
-- Search query
SELECT
    id AS message_id,
    conversation_id,
    role,
    content,
    search::highlight('<mark>', '</mark>', 1) AS snippet,
    search::score(1) AS relevance
FROM messages
WHERE content @1@ $query
ORDER BY relevance DESC
LIMIT $limit;
```

---

## 10. Cascade Deletes

SurrealDB doesn't have SQL-style `ON DELETE CASCADE`. Instead, we use `DEFINE EVENT`:

```surql
-- When a character is deleted, cascade to conversations, memories, lorebook
DEFINE EVENT cascade_character_delete ON characters WHEN $event = "DELETE" THEN {
    DELETE FROM conversations WHERE character_id = $before.id;
    DELETE FROM memories WHERE character_id = $before.id;
    DELETE FROM lorebook_entries WHERE character_id = $before.id;
};

-- When a conversation is deleted, cascade to messages, scenes, memories
DEFINE EVENT cascade_conversation_delete ON conversations WHEN $event = "DELETE" THEN {
    DELETE FROM messages WHERE conversation_id = $before.id;
    DELETE FROM scenes WHERE conversation_id = $before.id;
    DELETE FROM memories WHERE conversation_id = $before.id;
    DELETE FROM character_states WHERE conversation_id = $before.id;
};

-- When a memory is deleted, remove its graph edges
DEFINE EVENT cascade_memory_delete ON memories WHEN $event = "DELETE" THEN {
    DELETE memory_link WHERE in = $before.id OR out = $before.id;
};
```

---

## 11. Chat Command (`chat.rs`) Changes

The `build_prompt()` function changes from raw SQL to repo calls:

| Current SQL Call | Replacement |
|------------------|-------------|
| `sqlx::query("SELECT character_id, memory_scope FROM conversations WHERE id = ?")` | `ConversationRepo::get(db, id)` |
| `sqlx::query("SELECT data FROM characters WHERE id = ?")` | `CharacterRepo::get(db, id)` |
| `sqlx::query("SELECT content FROM lorebook_entries WHERE ...")` | `LorebookRepo::list_active(db, char_id)` |
| Message tree walk (while loop with N+1 queries) | `MessageRepo::get_branch(db, msg_id)` |
| `sqlx::query("SELECT content FROM memories WHERE ...")` | `MemoryRepo::list_for_context(db, char_id, conv_id, scope)` |
| `sqlx::query("SELECT mood, trust, ... FROM character_states WHERE ...")` | `CharacterStateRepo::get(db, char_id, conv_id)` |

The prompt assembly logic stays the same — only the data fetching changes.

---

## 12. Scope & What Does NOT Change

### Unchanged
- **Frontend** — zero Svelte changes, all IPC command signatures stay the same
- **Rig LLM layer** — `providers/unified.rs` is untouched
- **Image processing** — PNG import, avatar handling stays the same
- **HTTP client** — reqwest stays the same
- **All Tauri command signatures** — same function names, same parameters, same return types

### Removed
- `src-tauri/migrations/` directory (13 SQL files) — replaced by `db/schema.rs`
- `sqlx` dependency from `Cargo.toml`
- All `sqlx::query()` calls across all command files
- FTS5 triggers (replaced by built-in search index)

---

## 13. Testing Strategy

1. **Compile check**: `cargo build` — ensures all types are correct
2. **Schema bootstrap**: App launches, creates SurrealDB, defines all tables
3. **Seed data**: Default providers + characters appear in gallery
4. **CRUD smoke test**: Create/read/update/delete a character via UI
5. **Chat flow**: Send a message, verify streaming works end-to-end
6. **Memory graph**: Create memories, share between conversations, verify graph view
7. **FTS**: Search messages, verify highlighting works
8. **Branch conversation**: Fork a conversation, verify memory copies
9. **Svelte check**: `npm run check` — zero type errors from frontend

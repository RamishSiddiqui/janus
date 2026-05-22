use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use tracing::info;

use crate::error::MythicError;

/// Defines the complete SurrealDB schema for Mythic.
/// This function is idempotent — safe to call on every app startup.
/// SurrealDB's DEFINE statements are no-ops when the definition already exists.
pub async fn define_schema(db: &Surreal<Db>) -> Result<(), MythicError> {
    // ── 1. characters ───────────────────────────────────────────────────
    info!("  schema: characters...");
    db.query("
        DEFINE TABLE IF NOT EXISTS characters SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS name       ON characters TYPE string ASSERT $value != NONE;
        DEFINE FIELD IF NOT EXISTS spec       ON characters TYPE string DEFAULT 'chara_card_v2';
        DEFINE FIELD IF NOT EXISTS data       ON characters FLEXIBLE TYPE object;
        DEFINE FIELD IF NOT EXISTS avatar_path ON characters TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS created_at ON characters TYPE datetime DEFAULT time::now();
        DEFINE FIELD IF NOT EXISTS updated_at ON characters TYPE datetime DEFAULT time::now();

        DEFINE INDEX IF NOT EXISTS idx_characters_updated ON characters FIELDS updated_at;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:characters: {}", e)))?;

    // ── 2. conversations ────────────────────────────────────────────────
    info!("  schema: conversations...");
    db.query("
        DEFINE TABLE IF NOT EXISTS conversations SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS title                    ON conversations TYPE string;
        DEFINE FIELD IF NOT EXISTS character_id             ON conversations TYPE option<record<characters>>;
        DEFINE FIELD IF NOT EXISTS active_message_id        ON conversations TYPE option<record<messages>>;
        DEFINE FIELD IF NOT EXISTS memory_scope             ON conversations TYPE string DEFAULT 'character'
            ASSERT $value IN ['character', 'conversation', 'none'];
        DEFINE FIELD IF NOT EXISTS shared_character_ids     ON conversations TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS parent_conversation_id   ON conversations TYPE option<record<conversations>>;
        DEFINE FIELD IF NOT EXISTS branch_point_message_id  ON conversations TYPE option<record<messages>>;
        DEFINE FIELD IF NOT EXISTS created_at               ON conversations TYPE datetime DEFAULT time::now();
        DEFINE FIELD IF NOT EXISTS updated_at               ON conversations TYPE datetime DEFAULT time::now();

        DEFINE INDEX IF NOT EXISTS idx_conversations_character ON conversations FIELDS character_id;
        DEFINE INDEX IF NOT EXISTS idx_conversations_updated   ON conversations FIELDS updated_at;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:conversations: {}", e)))?;

    // ── 3. messages ─────────────────────────────────────────────────────
    info!("  schema: messages...");
    db.query("
        DEFINE TABLE IF NOT EXISTS messages SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS conversation_id ON messages TYPE record<conversations>;
        DEFINE FIELD IF NOT EXISTS role             ON messages TYPE string
            ASSERT $value IN ['user', 'assistant', 'system'];
        DEFINE FIELD IF NOT EXISTS content          ON messages TYPE string;
        DEFINE FIELD IF NOT EXISTS parent_id        ON messages TYPE option<record<messages>>;
        DEFINE FIELD IF NOT EXISTS metadata         ON messages FLEXIBLE TYPE option<object>;
        DEFINE FIELD IF NOT EXISTS created_at       ON messages TYPE datetime DEFAULT time::now();

        DEFINE INDEX IF NOT EXISTS idx_messages_conversation ON messages FIELDS conversation_id;
        DEFINE INDEX IF NOT EXISTS idx_messages_parent       ON messages FIELDS parent_id;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:messages: {}", e)))?;

    // FTS analyzer and index (separate query to isolate errors)
    info!("  schema: messages FTS...");
    db.query("
        DEFINE ANALYZER IF NOT EXISTS msg_analyzer TOKENIZERS class FILTERS lowercase, edgengram(2, 15);
        DEFINE INDEX IF NOT EXISTS idx_messages_fts ON messages FIELDS content
            SEARCH ANALYZER msg_analyzer BM25;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:messages_fts: {}", e)))?;

    // ── 4. memories ─────────────────────────────────────────────────────
    info!("  schema: memories...");
    db.query("
        DEFINE TABLE IF NOT EXISTS memories SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS character_id    ON memories TYPE option<record<characters>>;
        DEFINE FIELD IF NOT EXISTS conversation_id ON memories TYPE option<record<conversations>>;
        DEFINE FIELD IF NOT EXISTS content          ON memories TYPE string;
        DEFINE FIELD IF NOT EXISTS source           ON memories TYPE string DEFAULT 'user'
            ASSERT $value IN ['user', 'auto'];
        DEFINE FIELD IF NOT EXISTS parent_id        ON memories TYPE option<record<memories>>;
        DEFINE FIELD IF NOT EXISTS version          ON memories TYPE int DEFAULT 1;
        DEFINE FIELD IF NOT EXISTS is_canon         ON memories TYPE bool DEFAULT false;
        DEFINE FIELD IF NOT EXISTS created_at       ON memories TYPE datetime DEFAULT time::now();

        DEFINE INDEX IF NOT EXISTS idx_memories_character    ON memories FIELDS character_id;
        DEFINE INDEX IF NOT EXISTS idx_memories_conversation ON memories FIELDS conversation_id;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:memories: {}", e)))?;

    // ── 5. memory_link (graph edge table) ───────────────────────────────
    info!("  schema: memory_link...");
    db.query("
        DEFINE TABLE IF NOT EXISTS memory_link TYPE RELATION FROM memories TO conversations SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS link_type    ON memory_link TYPE string ASSERT $value IN ['copy', 'sync'];
        DEFINE FIELD IF NOT EXISTS direction    ON memory_link TYPE string ASSERT $value IN ['one_way', 'two_way'];
        DEFINE FIELD IF NOT EXISTS sync_mode    ON memory_link TYPE string ASSERT $value IN ['auto', 'manual'];
        DEFINE FIELD IF NOT EXISTS linked_memory_id ON memory_link TYPE option<record<memories>>;
        DEFINE FIELD IF NOT EXISTS created_at   ON memory_link TYPE datetime DEFAULT time::now();
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:memory_link: {}", e)))?;

    // memory_link event
    info!("  schema: memory_link event...");
    db.query("
        DEFINE EVENT IF NOT EXISTS enforce_copy_direction ON TABLE memory_link
            WHEN $event = 'CREATE' OR $event = 'UPDATE' THEN {
            IF $after.link_type = 'copy' AND $after.direction != 'one_way' {
                THROW 'Copy links must be one_way';
            };
        };
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:memory_link_event: {}", e)))?;

    // ── 6. lorebook_entries ─────────────────────────────────────────────
    info!("  schema: lorebook_entries...");
    db.query("
        DEFINE TABLE IF NOT EXISTS lorebook_entries SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS character_id    ON lorebook_entries TYPE option<record<characters>>;
        DEFINE FIELD IF NOT EXISTS keys            ON lorebook_entries TYPE array;
        DEFINE FIELD IF NOT EXISTS content         ON lorebook_entries TYPE string;
        DEFINE FIELD IF NOT EXISTS enabled         ON lorebook_entries TYPE bool DEFAULT true;
        DEFINE FIELD IF NOT EXISTS always_active   ON lorebook_entries TYPE bool DEFAULT false;
        DEFINE FIELD IF NOT EXISTS priority        ON lorebook_entries TYPE int DEFAULT 10;
        DEFINE FIELD IF NOT EXISTS insertion_order ON lorebook_entries TYPE int DEFAULT 100;
        DEFINE FIELD IF NOT EXISTS name            ON lorebook_entries TYPE option<string>;

        DEFINE INDEX IF NOT EXISTS idx_lorebook_character ON lorebook_entries FIELDS character_id;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:lorebook: {}", e)))?;

    // ── 7. provider_configs ─────────────────────────────────────────────
    info!("  schema: provider_configs...");
    db.query("
        DEFINE TABLE IF NOT EXISTS provider_configs SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS name          ON provider_configs TYPE string;
        DEFINE FIELD IF NOT EXISTS provider_type ON provider_configs TYPE string
            ASSERT $value IN ['llm', 'image', 'video'];
        DEFINE FIELD IF NOT EXISTS adapter       ON provider_configs TYPE string;
        DEFINE FIELD IF NOT EXISTS config        ON provider_configs FLEXIBLE TYPE object;
        DEFINE FIELD IF NOT EXISTS is_default    ON provider_configs TYPE bool DEFAULT false;

        DEFINE INDEX IF NOT EXISTS idx_provider_type ON provider_configs FIELDS provider_type;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:providers: {}", e)))?;

    // ── 8. enabled_models ───────────────────────────────────────────────
    info!("  schema: enabled_models...");
    db.query("
        DEFINE TABLE IF NOT EXISTS enabled_models SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS provider_id ON enabled_models TYPE record<provider_configs>;
        DEFINE FIELD IF NOT EXISTS model_id    ON enabled_models TYPE string;
        DEFINE FIELD IF NOT EXISTS model_type  ON enabled_models TYPE string DEFAULT 'llm';
        DEFINE FIELD IF NOT EXISTS enabled     ON enabled_models TYPE bool DEFAULT true;
        DEFINE FIELD IF NOT EXISTS created_at  ON enabled_models TYPE datetime DEFAULT time::now();
        DEFINE FIELD IF NOT EXISTS updated_at  ON enabled_models TYPE datetime DEFAULT time::now();

        DEFINE INDEX IF NOT EXISTS idx_enabled_provider_model ON enabled_models FIELDS provider_id, model_id UNIQUE;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:enabled_models: {}", e)))?;

    // ── 9. scenes ───────────────────────────────────────────────────────
    info!("  schema: scenes...");
    db.query("
        DEFINE TABLE IF NOT EXISTS scenes SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS conversation_id ON scenes TYPE record<conversations>;
        DEFINE FIELD IF NOT EXISTS message_id      ON scenes TYPE option<record<messages>>;
        DEFINE FIELD IF NOT EXISTS media_type      ON scenes TYPE string ASSERT $value IN ['image', 'video'];
        DEFINE FIELD IF NOT EXISTS prompt           ON scenes TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path        ON scenes TYPE string;
        DEFINE FIELD IF NOT EXISTS caption          ON scenes TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS metadata         ON scenes FLEXIBLE TYPE option<object>;
        DEFINE FIELD IF NOT EXISTS created_at       ON scenes TYPE datetime DEFAULT time::now();

        DEFINE INDEX IF NOT EXISTS idx_scenes_conversation ON scenes FIELDS conversation_id;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:scenes: {}", e)))?;

    // ── 10. character_states ────────────────────────────────────────────
    info!("  schema: character_states...");
    db.query("
        DEFINE TABLE IF NOT EXISTS character_states SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS character_id    ON character_states TYPE record<characters>;
        DEFINE FIELD IF NOT EXISTS conversation_id ON character_states TYPE record<conversations>;
        DEFINE FIELD IF NOT EXISTS mood             ON character_states TYPE int DEFAULT 50;
        DEFINE FIELD IF NOT EXISTS trust            ON character_states TYPE int DEFAULT 50;
        DEFINE FIELD IF NOT EXISTS arousal          ON character_states TYPE int DEFAULT 30;
        DEFINE FIELD IF NOT EXISTS dominant_emotion ON character_states TYPE string DEFAULT 'neutral';
        DEFINE FIELD IF NOT EXISTS state_summary    ON character_states TYPE string DEFAULT '';
        DEFINE FIELD IF NOT EXISTS updated_at       ON character_states TYPE datetime DEFAULT time::now();

        DEFINE INDEX IF NOT EXISTS idx_charstate_unique ON character_states FIELDS character_id, conversation_id UNIQUE;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:character_states: {}", e)))?;

    // ── 11. conversation_summaries ──────────────────────────────────────
    info!("  schema: conversation_summaries...");
    db.query("
        DEFINE TABLE IF NOT EXISTS conversation_summaries SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS conversation_id ON conversation_summaries TYPE record<conversations>;
        DEFINE FIELD IF NOT EXISTS summary_text ON conversation_summaries TYPE string;
        DEFINE FIELD IF NOT EXISTS covered_message_count ON conversation_summaries TYPE int DEFAULT 0;
        DEFINE FIELD IF NOT EXISTS token_count ON conversation_summaries TYPE int DEFAULT 0;
        DEFINE FIELD IF NOT EXISTS window_start_message_id ON conversation_summaries TYPE option<record<messages>>;
        DEFINE FIELD IF NOT EXISTS created_at ON conversation_summaries TYPE datetime DEFAULT time::now();
        DEFINE FIELD IF NOT EXISTS updated_at ON conversation_summaries TYPE datetime DEFAULT time::now();

        DEFINE INDEX IF NOT EXISTS idx_summary_conversation ON conversation_summaries FIELDS conversation_id UNIQUE;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:conversation_summaries: {}", e)))?;

    // ── 12. message_embeddings ──────────────────────────────────────────
    info!("  schema: message_embeddings...");
    db.query("
        DEFINE TABLE IF NOT EXISTS message_embeddings SCHEMAFULL;

        DEFINE FIELD IF NOT EXISTS message_id ON message_embeddings TYPE record<messages>;
        DEFINE FIELD IF NOT EXISTS conversation_id ON message_embeddings TYPE record<conversations>;
        DEFINE FIELD IF NOT EXISTS embedding ON message_embeddings TYPE array<float>;
        DEFINE FIELD IF NOT EXISTS model_name ON message_embeddings TYPE string;
        DEFINE FIELD IF NOT EXISTS created_at ON message_embeddings TYPE datetime DEFAULT time::now();

        DEFINE INDEX IF NOT EXISTS idx_me_message ON message_embeddings FIELDS message_id UNIQUE;
        DEFINE INDEX IF NOT EXISTS idx_me_conversation ON message_embeddings FIELDS conversation_id;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:message_embeddings: {}", e)))?;

    // MTREE vector index (separate to isolate errors)
    info!("  schema: message_embeddings MTREE...");
    db.query("
        DEFINE INDEX IF NOT EXISTS idx_me_embedding ON message_embeddings
            FIELDS embedding MTREE DIMENSION 1536 DIST COSINE TYPE F32;
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:message_embeddings_mtree: {}", e)))?;

    // ── Cascade delete events ───────────────────────────────────────────
    info!("  schema: cascade events...");
    db.query("
        DEFINE EVENT IF NOT EXISTS cascade_character_delete ON TABLE characters
            WHEN $event = 'DELETE' THEN {
            DELETE FROM conversations WHERE character_id = $before.id;
            DELETE FROM memories WHERE character_id = $before.id;
            DELETE FROM lorebook_entries WHERE character_id = $before.id;
        };
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:cascade_char: {}", e)))?;

    db.query("
        DEFINE EVENT IF NOT EXISTS cascade_conversation_delete ON TABLE conversations
            WHEN $event = 'DELETE' THEN {
            DELETE FROM messages WHERE conversation_id = $before.id;
            DELETE FROM scenes WHERE conversation_id = $before.id;
            DELETE FROM memories WHERE conversation_id = $before.id;
            DELETE FROM character_states WHERE conversation_id = $before.id;
            DELETE FROM conversation_summaries WHERE conversation_id = $before.id;
            DELETE FROM message_embeddings WHERE conversation_id = $before.id;
        };
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:cascade_conv: {}", e)))?;

    db.query("
        DEFINE EVENT IF NOT EXISTS cascade_memory_delete ON TABLE memories
            WHEN $event = 'DELETE' THEN {
            DELETE FROM memory_link WHERE in = $before.id;
            DELETE FROM memory_link WHERE linked_memory_id = $before.id;
        };
    ")
    .await?
    .check()
    .map_err(|e| MythicError::DatabaseOp(format!("schema:cascade_mem: {}", e)))?;

    Ok(())
}

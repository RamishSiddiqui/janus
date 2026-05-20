use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::MythicError;

/// Defines the complete SurrealDB schema for Mythic.
/// This function is idempotent — safe to call on every app startup.
/// SurrealDB's DEFINE statements are no-ops when the definition already exists.
pub async fn define_schema(db: &Surreal<Db>) -> Result<(), MythicError> {
    // ── 1. characters ───────────────────────────────────────────────────
    db.query("
        DEFINE TABLE characters SCHEMAFULL;

        DEFINE FIELD name       ON characters TYPE string ASSERT $value != NONE;
        DEFINE FIELD spec       ON characters TYPE string DEFAULT 'chara_card_v2';
        DEFINE FIELD data       ON characters FLEXIBLE TYPE object;
        DEFINE FIELD avatar_path ON characters TYPE option<string>;
        DEFINE FIELD created_at ON characters TYPE datetime DEFAULT time::now();
        DEFINE FIELD updated_at ON characters TYPE datetime DEFAULT time::now();

        DEFINE INDEX idx_characters_updated ON characters FIELDS updated_at;
    ")
    .await?;

    // ── 2. conversations ────────────────────────────────────────────────
    db.query("
        DEFINE TABLE conversations SCHEMAFULL;

        DEFINE FIELD title                    ON conversations TYPE string;
        DEFINE FIELD character_id             ON conversations TYPE option<record<characters>>;
        DEFINE FIELD active_message_id        ON conversations TYPE option<record<messages>>;
        DEFINE FIELD memory_scope             ON conversations TYPE string DEFAULT 'character'
            ASSERT $value IN ['character', 'conversation', 'none'];
        DEFINE FIELD parent_conversation_id   ON conversations TYPE option<record<conversations>>;
        DEFINE FIELD branch_point_message_id  ON conversations TYPE option<record<messages>>;
        DEFINE FIELD created_at               ON conversations TYPE datetime DEFAULT time::now();
        DEFINE FIELD updated_at               ON conversations TYPE datetime DEFAULT time::now();

        DEFINE INDEX idx_conversations_character ON conversations FIELDS character_id;
        DEFINE INDEX idx_conversations_updated   ON conversations FIELDS updated_at;
    ")
    .await?;

    // ── 3. messages ─────────────────────────────────────────────────────
    db.query("
        DEFINE TABLE messages SCHEMAFULL;

        DEFINE FIELD conversation_id ON messages TYPE record<conversations>;
        DEFINE FIELD role             ON messages TYPE string
            ASSERT $value IN ['user', 'assistant', 'system'];
        DEFINE FIELD content          ON messages TYPE string;
        DEFINE FIELD parent_id        ON messages TYPE option<record<messages>>;
        DEFINE FIELD metadata         ON messages FLEXIBLE TYPE option<object>;
        DEFINE FIELD created_at       ON messages TYPE datetime DEFAULT time::now();

        DEFINE INDEX idx_messages_conversation ON messages FIELDS conversation_id;
        DEFINE INDEX idx_messages_parent       ON messages FIELDS parent_id;

        -- Full-text search (replaces FTS5)
        DEFINE ANALYZER msg_analyzer TOKENIZERS unicode FILTERS lowercase, edgengram(2, 15);
        DEFINE INDEX idx_messages_fts ON messages FIELDS content
            SEARCH ANALYZER msg_analyzer BM25;
    ")
    .await?;

    // ── 4. memories ─────────────────────────────────────────────────────
    db.query("
        DEFINE TABLE memories SCHEMAFULL;

        DEFINE FIELD character_id    ON memories TYPE option<record<characters>>;
        DEFINE FIELD conversation_id ON memories TYPE option<record<conversations>>;
        DEFINE FIELD content          ON memories TYPE string;
        DEFINE FIELD source           ON memories TYPE string DEFAULT 'user'
            ASSERT $value IN ['user', 'auto'];
        DEFINE FIELD parent_id        ON memories TYPE option<record<memories>>;
        DEFINE FIELD version          ON memories TYPE int DEFAULT 1;
        DEFINE FIELD is_canon         ON memories TYPE bool DEFAULT false;
        DEFINE FIELD created_at       ON memories TYPE datetime DEFAULT time::now();

        DEFINE INDEX idx_memories_character    ON memories FIELDS character_id;
        DEFINE INDEX idx_memories_conversation ON memories FIELDS conversation_id;
    ")
    .await?;

    // ── 5. memory_link (graph edge table) ───────────────────────────────
    db.query("
        DEFINE TABLE memory_link SCHEMAFULL;

        DEFINE FIELD in           ON memory_link TYPE record<memories>;
        DEFINE FIELD out          ON memory_link TYPE record<conversations>;
        DEFINE FIELD link_type    ON memory_link TYPE string ASSERT $value IN ['copy', 'sync'];
        DEFINE FIELD direction    ON memory_link TYPE string ASSERT $value IN ['one_way', 'two_way'];
        DEFINE FIELD sync_mode    ON memory_link TYPE string ASSERT $value IN ['auto', 'manual'];
        DEFINE FIELD linked_memory_id ON memory_link TYPE option<record<memories>>;
        DEFINE FIELD created_at   ON memory_link TYPE datetime DEFAULT time::now();

        DEFINE EVENT enforce_copy_direction ON TABLE memory_link
            WHEN $event = 'CREATE' OR $event = 'UPDATE' THEN {
            IF $after.link_type = 'copy' AND $after.direction != 'one_way' {
                THROW 'Copy links must be one_way';
            };
        };
    ")
    .await?;

    // ── 6. lorebook_entries ─────────────────────────────────────────────
    db.query("
        DEFINE TABLE lorebook_entries SCHEMAFULL;

        DEFINE FIELD character_id    ON lorebook_entries TYPE option<record<characters>>;
        DEFINE FIELD keys            ON lorebook_entries TYPE array;
        DEFINE FIELD content         ON lorebook_entries TYPE string;
        DEFINE FIELD enabled         ON lorebook_entries TYPE bool DEFAULT true;
        DEFINE FIELD always_active   ON lorebook_entries TYPE bool DEFAULT false;
        DEFINE FIELD priority        ON lorebook_entries TYPE int DEFAULT 10;
        DEFINE FIELD insertion_order ON lorebook_entries TYPE int DEFAULT 100;
        DEFINE FIELD name            ON lorebook_entries TYPE option<string>;

        DEFINE INDEX idx_lorebook_character ON lorebook_entries FIELDS character_id;
    ")
    .await?;

    // ── 7. provider_configs ─────────────────────────────────────────────
    db.query("
        DEFINE TABLE provider_configs SCHEMAFULL;

        DEFINE FIELD name          ON provider_configs TYPE string;
        DEFINE FIELD provider_type ON provider_configs TYPE string
            ASSERT $value IN ['llm', 'image', 'video'];
        DEFINE FIELD adapter       ON provider_configs TYPE string;
        DEFINE FIELD config        ON provider_configs FLEXIBLE TYPE object;
        DEFINE FIELD is_default    ON provider_configs TYPE bool DEFAULT false;

        DEFINE INDEX idx_provider_type ON provider_configs FIELDS provider_type;
    ")
    .await?;

    // ── 8. enabled_models ───────────────────────────────────────────────
    db.query("
        DEFINE TABLE enabled_models SCHEMAFULL;

        DEFINE FIELD provider_id ON enabled_models TYPE record<provider_configs>;
        DEFINE FIELD model_id    ON enabled_models TYPE string;
        DEFINE FIELD model_type  ON enabled_models TYPE string DEFAULT 'llm';
        DEFINE FIELD enabled     ON enabled_models TYPE bool DEFAULT true;
        DEFINE FIELD created_at  ON enabled_models TYPE datetime DEFAULT time::now();
        DEFINE FIELD updated_at  ON enabled_models TYPE datetime DEFAULT time::now();

        DEFINE INDEX idx_enabled_provider_model ON enabled_models FIELDS provider_id, model_id UNIQUE;
    ")
    .await?;

    // ── 9. scenes ───────────────────────────────────────────────────────
    db.query("
        DEFINE TABLE scenes SCHEMAFULL;

        DEFINE FIELD conversation_id ON scenes TYPE record<conversations>;
        DEFINE FIELD message_id      ON scenes TYPE option<record<messages>>;
        DEFINE FIELD media_type      ON scenes TYPE string ASSERT $value IN ['image', 'video'];
        DEFINE FIELD prompt           ON scenes TYPE string;
        DEFINE FIELD file_path        ON scenes TYPE string;
        DEFINE FIELD caption          ON scenes TYPE option<string>;
        DEFINE FIELD metadata         ON scenes FLEXIBLE TYPE option<object>;
        DEFINE FIELD created_at       ON scenes TYPE datetime DEFAULT time::now();

        DEFINE INDEX idx_scenes_conversation ON scenes FIELDS conversation_id;
    ")
    .await?;

    // ── 10. character_states ────────────────────────────────────────────
    db.query("
        DEFINE TABLE character_states SCHEMAFULL;

        DEFINE FIELD character_id    ON character_states TYPE record<characters>;
        DEFINE FIELD conversation_id ON character_states TYPE record<conversations>;
        DEFINE FIELD mood             ON character_states TYPE int DEFAULT 50;
        DEFINE FIELD trust            ON character_states TYPE int DEFAULT 50;
        DEFINE FIELD arousal          ON character_states TYPE int DEFAULT 30;
        DEFINE FIELD dominant_emotion ON character_states TYPE string DEFAULT 'neutral';
        DEFINE FIELD state_summary    ON character_states TYPE string DEFAULT '';
        DEFINE FIELD updated_at       ON character_states TYPE datetime DEFAULT time::now();

        DEFINE INDEX idx_charstate_unique ON character_states FIELDS character_id, conversation_id UNIQUE;
    ")
    .await?;

    // ── Cascade delete events ───────────────────────────────────────────
    db.query("
        DEFINE EVENT cascade_character_delete ON TABLE characters
            WHEN $event = 'DELETE' THEN {
            DELETE FROM conversations WHERE character_id = $before.id;
            DELETE FROM memories WHERE character_id = $before.id;
            DELETE FROM lorebook_entries WHERE character_id = $before.id;
        };

        DEFINE EVENT cascade_conversation_delete ON TABLE conversations
            WHEN $event = 'DELETE' THEN {
            DELETE FROM messages WHERE conversation_id = $before.id;
            DELETE FROM scenes WHERE conversation_id = $before.id;
            DELETE FROM memories WHERE conversation_id = $before.id;
            DELETE FROM character_states WHERE conversation_id = $before.id;
        };

        DEFINE EVENT cascade_memory_delete ON TABLE memories
            WHEN $event = 'DELETE' THEN {
            DELETE FROM memory_link WHERE in = $before.id;
        };
    ")
    .await?;

    Ok(())
}

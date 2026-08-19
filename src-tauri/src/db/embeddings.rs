use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::debug;

use crate::error::MythicError;

pub struct EmbeddingRepo;

impl EmbeddingRepo {
    /// Ensures the HNSW vector index exists with the correct dimension.
    /// If the index exists with a different dimension, it is dropped and recreated.
    pub async fn ensure_vector_index(
        db: &Surreal<Db>,
        dimension: usize,
    ) -> Result<(), MythicError> {
        // Drop existing index (safe if it doesn't exist)
        let _ = db
            .query("REMOVE INDEX IF EXISTS idx_me_embedding ON message_embeddings")
            .await;

        // Create with the correct dimension
        let query = format!(
            "DEFINE INDEX idx_me_embedding ON message_embeddings FIELDS embedding HNSW DIMENSION {} DIST COSINE TYPE F32",
            dimension
        );
        db.query(&query).await?.check().map_err(|e| {
            MythicError::DatabaseOp(format!("ensure_vector_index({}): {}", dimension, e))
        })?;

        tracing::info!("[embeddings] HNSW index set to dimension {}", dimension);
        Ok(())
    }

    /// Returns the dimension of existing embeddings, or None if no embeddings exist.
    pub async fn get_index_dimension(
        db: &Surreal<Db>,
        conversation_id: Option<&str>,
    ) -> Result<Option<usize>, MythicError> {
        #[derive(serde::Deserialize)]
        struct DimRow {
            dimension: i64,
        }

        let rows: Vec<DimRow> = match conversation_id {
            Some(conv_id) => {
                let mut result = db
                    .query(
                        "SELECT dimension FROM message_embeddings \
                     WHERE conversation_id = type::record('conversations', $conv_id) \
                        AND entry_type = 'message' LIMIT 1",
                    )
                    .bind(("conv_id", conv_id.to_string()))
                    .await?;
                crate::db::value_bridge::from_value_vec(result.take(0)?)?
            }
            None => {
                let mut result = db.query(
                    "SELECT dimension FROM message_embeddings WHERE entry_type = 'message' LIMIT 1"
                ).await?;
                crate::db::value_bridge::from_value_vec(result.take(0)?)?
            }
        };

        Ok(rows.into_iter().next().map(|r| r.dimension as usize))
    }

    // ── Store: Messages ──────────────────────────────────────────────────

    /// Store an embedding for a chat message.
    pub async fn store(
        db: &Surreal<Db>,
        message_id: &str,
        conversation_id: &str,
        embedding: &[f64],
        model_name: &str,
        character_id: Option<&str>,
    ) -> Result<(), MythicError> {
        // Convert f64 to f32 for storage efficiency (HNSW index uses F32)
        let embedding_f32: Vec<f32> = embedding.iter().map(|&v| v as f32).collect();
        let dimension = embedding_f32.len() as i64;

        let query = if let Some(char_id) = character_id {
            db.query(
                "CREATE message_embeddings SET \
                    message_id = type::record('messages', $msg_id), \
                    conversation_id = type::record('conversations', $conv_id), \
                    character_id = type::record('characters', $char_id), \
                    embedding = $embedding, \
                    model_name = $model, \
                    dimension = $dim, \
                    entry_type = 'message'",
            )
            .bind(("char_id", char_id.to_string()))
            .bind(("msg_id", message_id.to_string()))
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("embedding", embedding_f32))
            .bind(("model", model_name.to_string()))
            .bind(("dim", dimension))
            .await?
        } else {
            db.query(
                "CREATE message_embeddings SET \
                    message_id = type::record('messages', $msg_id), \
                    conversation_id = type::record('conversations', $conv_id), \
                    embedding = $embedding, \
                    model_name = $model, \
                    dimension = $dim, \
                    entry_type = 'message'",
            )
            .bind(("msg_id", message_id.to_string()))
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("embedding", embedding_f32))
            .bind(("model", model_name.to_string()))
            .bind(("dim", dimension))
            .await?
        };

        let _ = query; // consume the response

        debug!(
            "[embeddings] Stored message embedding {} (dim={})",
            message_id, dimension
        );
        Ok(())
    }

    // ── Store: Memories ──────────────────────────────────────────────────

    /// Store an embedding for a memory fact.
    /// Uses `entry_type = "memory"` and `source_id` to track the memory ID.
    pub async fn store_memory(
        db: &Surreal<Db>,
        memory_id: &str,
        character_id: &str,
        embedding: &[f64],
        model_name: &str,
    ) -> Result<(), MythicError> {
        let embedding_f32: Vec<f32> = embedding.iter().map(|&v| v as f32).collect();
        let dimension = embedding_f32.len() as i64;

        db.query(
            "CREATE message_embeddings SET \
                character_id = type::record('characters', $char_id), \
                source_id = $source_id, \
                embedding = $embedding, \
                model_name = $model, \
                dimension = $dim, \
                entry_type = 'memory'",
        )
        .bind(("char_id", character_id.to_string()))
        .bind(("source_id", memory_id.to_string()))
        .bind(("embedding", embedding_f32))
        .bind(("model", model_name.to_string()))
        .bind(("dim", dimension))
        .await?;

        debug!(
            "[embeddings] Stored memory embedding {} (dim={})",
            memory_id, dimension
        );
        Ok(())
    }

    /// Check if an embedding exists for a memory.
    pub async fn memory_exists(db: &Surreal<Db>, memory_id: &str) -> Result<bool, MythicError> {
        let mut result = db
            .query(
                "SELECT count() FROM message_embeddings \
                 WHERE source_id = $source_id AND entry_type = 'memory' \
                 GROUP ALL",
            )
            .bind(("source_id", memory_id.to_string()))
            .await?;

        let count: Option<serde_json::Value> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        Ok(count
            .and_then(|v| v.get("count").and_then(|c| c.as_u64()))
            .unwrap_or(0)
            > 0)
    }

    /// Delete embedding for a memory (when memory is deleted).
    pub async fn delete_memory_embedding(
        db: &Surreal<Db>,
        memory_id: &str,
    ) -> Result<(), MythicError> {
        db.query(
            "DELETE FROM message_embeddings \
             WHERE source_id = $source_id AND entry_type = 'memory'",
        )
        .bind(("source_id", memory_id.to_string()))
        .await?;

        debug!("[embeddings] Deleted memory embedding for {}", memory_id);
        Ok(())
    }

    // ── Existence Check: Messages ────────────────────────────────────────

    /// Check if an embedding exists for a message.
    pub async fn exists(db: &Surreal<Db>, message_id: &str) -> Result<bool, MythicError> {
        let mut result = db
            .query(
                "SELECT count() FROM message_embeddings \
                 WHERE message_id = type::record('messages', $msg_id) \
                 GROUP ALL",
            )
            .bind(("msg_id", message_id.to_string()))
            .await?;

        let count: Option<serde_json::Value> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        Ok(count
            .and_then(|v| v.get("count").and_then(|c| c.as_u64()))
            .unwrap_or(0)
            > 0)
    }

    // ── Query: Messages ──────────────────────────────────────────────────

    /// Query top-K similar messages using cosine similarity.
    ///
    /// Supports two modes:
    /// - **Conversation-scoped**: `conversation_id = Some(...)`, `character_id = None`
    ///   → searches within a single conversation
    /// - **Character-scoped**: `conversation_id = None`, `character_id = Some(...)`
    ///   → searches across ALL conversations for this character
    pub async fn query_similar(
        db: &Surreal<Db>,
        conversation_id: Option<&str>,
        character_id: Option<&str>,
        query_embedding: &[f64],
        top_k: usize,
        min_similarity: f64,
        exclude_message_ids: &[String],
    ) -> Result<Vec<RetrievedContext>, MythicError> {
        let query_f32: Vec<f32> = query_embedding.iter().map(|&v| v as f32).collect();

        // Build exclude list as bound Record IDs — never interpolated into the query text
        let exclude_things: Vec<surrealdb::types::RecordId> = exclude_message_ids
            .iter()
            .map(|id| surrealdb::types::RecordId::new("messages", id.as_str()))
            .collect();

        // Build scope filter
        let scope_filter = match (conversation_id, character_id) {
            (Some(_conv_id), _) => {
                "conversation_id = type::record('conversations', $scope_id)".to_string()
            }
            (None, Some(_char_id)) => {
                "character_id = type::record('characters', $scope_id)".to_string()
            }
            (None, None) => "true".to_string(), // no filter (shouldn't happen)
        };

        let scope_id = conversation_id.or(character_id).unwrap_or("");

        let query = format!(
            "SELECT \
                message_id, \
                vector::similarity::cosine(embedding, $query_vec) AS similarity \
             FROM message_embeddings \
             WHERE {scope_filter} \
                AND entry_type = 'message' \
                AND vector::similarity::cosine(embedding, $query_vec) >= $min_sim \
                AND message_id NOT IN $excluded_ids \
             ORDER BY similarity DESC \
             LIMIT $top_k"
        );

        let mut result = db
            .query(&query)
            .bind(("scope_id", scope_id.to_string()))
            .bind(("query_vec", query_f32))
            .bind(("min_sim", min_similarity as f32))
            .bind(("top_k", top_k as i64))
            .bind(("excluded_ids", exclude_things))
            .await?;

        #[derive(serde::Deserialize, Debug)]
        struct EmbeddingHit {
            #[serde(deserialize_with = "crate::models::deserialize_thing")]
            message_id: surrealdb::types::RecordId,
            similarity: f64,
        }

        let hits: Vec<EmbeddingHit> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;

        // For each hit, fetch the actual message content
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            let msg_id = crate::db::value_bridge::record_id_to_string(&hit.message_id);
            let mut msg_result = db
                .query("SELECT role, content FROM type::record('messages', $id)")
                .bind(("id", msg_id.clone()))
                .await?;

            #[derive(serde::Deserialize)]
            struct MsgContent {
                role: String,
                content: String,
            }

            let raw = msg_result.take::<Option<surrealdb::types::Value>>(0);
            if let Ok(Some(msg)) = raw
                .map(|v| v.and_then(|v| crate::db::value_bridge::from_value::<MsgContent>(v).ok()))
            {
                results.push(RetrievedContext {
                    message_id: msg_id,
                    role: msg.role,
                    content: msg.content,
                    similarity: hit.similarity,
                });
            }
        }

        debug!(
            "[embeddings] Message query returned {} results (scope={})",
            results.len(),
            scope_id,
        );

        Ok(results)
    }

    /// Keyword search over message content using the BM25 full-text index
    /// already defined on `messages` (see `schema.rs`). Scoped identically to
    /// `query_similar` so the two result sets can be fused by a caller.
    ///
    /// Only rank (list order) is used by callers, not the raw BM25 score, so
    /// `search::score` isn't selected — `ORDER BY search::score(1) DESC` is
    /// enough to establish rank.
    pub async fn keyword_search_messages(
        db: &Surreal<Db>,
        conversation_id: Option<&str>,
        character_id: Option<&str>,
        query_text: &str,
        top_k: usize,
        exclude_message_ids: &[String],
    ) -> Result<Vec<RetrievedContext>, MythicError> {
        if query_text.trim().is_empty() {
            return Ok(vec![]);
        }

        // `ORDER BY` can't call `search::score()` directly — it must
        // reference an aliased column from the SELECT list, or SurrealDB
        // fails at runtime with "Missing order idiom" (a parse-time-valid
        // but runtime-invalid query — cargo check can't catch this).
        //
        // Scope/role/exclusion filtering happens in Rust rather than the
        // WHERE clause because it's untested whether ANDing those with the
        // `@1@` match affects index selection; this shape is proven to work.
        let padded_limit = (top_k * 5).max(50);

        let mut result = db
            .query(
                "SELECT id AS message_id, role, content, conversation_id, character_id, \
                        conversation_id.character_id AS conv_character_id, \
                        search::score(1) AS relevance \
                 FROM messages \
                 WHERE content @1@ $query \
                 ORDER BY relevance DESC \
                 LIMIT $limit",
            )
            .bind(("query", query_text.to_string()))
            .bind(("limit", padded_limit as i64))
            .await?;

        #[derive(serde::Deserialize)]
        struct KeywordHit {
            #[serde(deserialize_with = "crate::models::deserialize_thing")]
            message_id: surrealdb::types::RecordId,
            role: String,
            content: String,
            #[serde(deserialize_with = "crate::models::deserialize_thing")]
            conversation_id: surrealdb::types::RecordId,
            // Message-level attribution (multi-char segments) — `None` for
            // ordinary single-character messages.
            #[serde(default, deserialize_with = "crate::models::deserialize_option_thing")]
            character_id: Option<surrealdb::types::RecordId>,
            // The owning conversation's character — set for every message in
            // a single-character conversation.
            #[serde(default, deserialize_with = "crate::models::deserialize_option_thing")]
            conv_character_id: Option<surrealdb::types::RecordId>,
        }

        let hits: Vec<KeywordHit> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;
        let exclude_set: std::collections::HashSet<&str> =
            exclude_message_ids.iter().map(|s| s.as_str()).collect();

        // `similarity` has no BM25 meaning here — the caller (RRF fusion)
        // only reads list order, and overwrites this field once fused.
        let filtered: Vec<RetrievedContext> = hits
            .into_iter()
            .filter(|h| {
                if !matches!(h.role.as_str(), "user" | "assistant") || h.content.is_empty() {
                    return false;
                }
                if exclude_set
                    .contains(crate::db::value_bridge::record_id_to_string(&h.message_id).as_str())
                {
                    return false;
                }
                match (conversation_id, character_id) {
                    (Some(conv), _) => {
                        crate::db::value_bridge::record_id_to_string(&h.conversation_id) == conv
                    }
                    (None, Some(ch)) => {
                        h.character_id
                            .as_ref()
                            .map(crate::db::value_bridge::record_id_to_string)
                            .as_deref()
                            == Some(ch)
                            || h.conv_character_id
                                .as_ref()
                                .map(crate::db::value_bridge::record_id_to_string)
                                .as_deref()
                                == Some(ch)
                    }
                    (None, None) => true,
                }
            })
            .take(top_k)
            .map(|h| RetrievedContext {
                message_id: crate::db::value_bridge::record_id_to_string(&h.message_id),
                role: h.role,
                content: h.content,
                similarity: 0.0,
            })
            .collect();

        debug!(
            "[embeddings] Keyword query returned {} results (conv={:?}, char={:?})",
            filtered.len(),
            conversation_id,
            character_id,
        );

        Ok(filtered)
    }

    // ── Query: Memories ──────────────────────────────────────────────────

    /// Query top-K semantically similar memories for a character.
    ///
    /// `conversation_id`: when `Some`, restricts results to memories that
    /// either belong to this specific conversation or are canon (settled,
    /// visible everywhere) — mirrors `memory_scope = "conversation"`'s
    /// isolation guarantee, which this query used to ignore entirely
    /// (always searching the character's memories across every conversation
    /// regardless of scope). `None` preserves the original character-wide
    /// search, used for `memory_scope = "character"`.
    ///
    /// Filtering happens in Rust after an over-fetch (`top_k * 2` from the
    /// vector query) rather than in the SurrealQL itself, since memory
    /// embeddings don't carry their own conversation_id (only their owning
    /// memory record does) — same over-fetch-then-filter shape already used
    /// for RRF fusion elsewhere in this module.
    pub async fn query_memory_similar(
        db: &Surreal<Db>,
        character_id: &str,
        query_embedding: &[f64],
        top_k: usize,
        min_similarity: f64,
        conversation_id: Option<&str>,
    ) -> Result<Vec<RetrievedMemoryContext>, MythicError> {
        let query_f32: Vec<f32> = query_embedding.iter().map(|&v| v as f32).collect();
        let fetch_k = if conversation_id.is_some() {
            top_k * 2
        } else {
            top_k
        };

        let mut result = db
            .query(
                "SELECT \
                    source_id, \
                    vector::similarity::cosine(embedding, $query_vec) AS similarity \
                 FROM message_embeddings \
                 WHERE character_id = type::record('characters', $char_id) \
                    AND entry_type = 'memory' \
                    AND vector::similarity::cosine(embedding, $query_vec) >= $min_sim \
                 ORDER BY similarity DESC \
                 LIMIT $top_k",
            )
            .bind(("char_id", character_id.to_string()))
            .bind(("query_vec", query_f32))
            .bind(("min_sim", min_similarity as f32))
            .bind(("top_k", fetch_k as i64))
            .await?;

        #[derive(serde::Deserialize, Debug)]
        struct MemoryHit {
            source_id: String,
            similarity: f64,
        }

        let hits: Vec<MemoryHit> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;

        // Fetch memory content for each hit
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            let mut mem_result = db
                .query("SELECT content, is_canon, importance, last_accessed, conversation_id FROM type::record('memories', $id)")
                .bind(("id", hit.source_id.clone()))
                .await?;

            #[derive(serde::Deserialize)]
            struct MemContent {
                content: String,
                is_canon: bool,
                #[serde(default = "crate::models::memory::default_importance")]
                importance: i32,
                #[serde(
                    default,
                    deserialize_with = "crate::models::deserialize_option_datetime"
                )]
                last_accessed: Option<String>,
                #[serde(default, deserialize_with = "crate::models::deserialize_option_thing")]
                conversation_id: Option<surrealdb::types::RecordId>,
            }

            let raw = mem_result.take::<Option<surrealdb::types::Value>>(0);
            if let Ok(Some(mem)) = raw
                .map(|v| v.and_then(|v| crate::db::value_bridge::from_value::<MemContent>(v).ok()))
            {
                if let Some(scope_conv_id) = conversation_id {
                    let belongs_here = mem
                        .conversation_id
                        .as_ref()
                        .map(|t| crate::db::value_bridge::record_id_to_string(t) == scope_conv_id)
                        .unwrap_or(false);
                    if !mem.is_canon && !belongs_here {
                        continue;
                    }
                }
                results.push(RetrievedMemoryContext {
                    memory_id: hit.source_id,
                    content: mem.content,
                    is_canon: mem.is_canon,
                    similarity: hit.similarity,
                    importance: mem.importance,
                    last_accessed: mem.last_accessed,
                });
                if results.len() >= top_k {
                    break;
                }
            }
        }

        debug!(
            "[embeddings] Memory query returned {} results for character {}",
            results.len(),
            character_id,
        );

        Ok(results)
    }

    /// Keyword search over memory content using the BM25 full-text index on
    /// `memories`. Scoped identically to `query_memory_similar` (including
    /// the same `conversation_id` isolation for `memory_scope =
    /// "conversation"`) so the two result sets can be fused by a caller.
    /// Only rank is used by callers.
    pub async fn keyword_search_memories(
        db: &Surreal<Db>,
        character_id: &str,
        query_text: &str,
        top_k: usize,
        conversation_id: Option<&str>,
    ) -> Result<Vec<RetrievedMemoryContext>, MythicError> {
        if query_text.trim().is_empty() {
            return Ok(vec![]);
        }

        // See keyword_search_messages for why: ORDER BY must reference an
        // aliased column, not call search::score() directly; and scope
        // filtering happens in Rust against a proven query shape.
        let padded_limit = (top_k * 5).max(50);

        let mut result = db
            .query(
                "SELECT id AS memory_id, content, is_canon, importance, last_accessed, character_id, conversation_id, \
                        search::score(1) AS relevance \
                 FROM memories \
                 WHERE content @1@ $query \
                 ORDER BY relevance DESC \
                 LIMIT $limit"
            )
            .bind(("query", query_text.to_string()))
            .bind(("limit", padded_limit as i64))
            .await?;

        #[derive(serde::Deserialize)]
        struct KeywordMemHit {
            #[serde(deserialize_with = "crate::models::deserialize_thing")]
            memory_id: surrealdb::types::RecordId,
            content: String,
            is_canon: bool,
            #[serde(default = "crate::models::memory::default_importance")]
            importance: i32,
            #[serde(
                default,
                deserialize_with = "crate::models::deserialize_option_datetime"
            )]
            last_accessed: Option<String>,
            #[serde(default, deserialize_with = "crate::models::deserialize_option_thing")]
            character_id: Option<surrealdb::types::RecordId>,
            #[serde(default, deserialize_with = "crate::models::deserialize_option_thing")]
            conversation_id: Option<surrealdb::types::RecordId>,
        }

        let hits: Vec<KeywordMemHit> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;

        // `similarity` has no BM25 meaning here — overwritten once fused.
        let filtered: Vec<RetrievedMemoryContext> = hits
            .into_iter()
            .filter(|h| {
                h.character_id
                    .as_ref()
                    .map(crate::db::value_bridge::record_id_to_string)
                    .as_deref()
                    == Some(character_id)
            })
            .filter(|h| match conversation_id {
                Some(scope_conv_id) => {
                    h.is_canon
                        || h.conversation_id
                            .as_ref()
                            .map(|t| {
                                crate::db::value_bridge::record_id_to_string(t) == scope_conv_id
                            })
                            .unwrap_or(false)
                }
                None => true,
            })
            .take(top_k)
            .map(|h| RetrievedMemoryContext {
                memory_id: crate::db::value_bridge::record_id_to_string(&h.memory_id),
                content: h.content,
                is_canon: h.is_canon,
                importance: h.importance,
                last_accessed: h.last_accessed,
                similarity: 0.0,
            })
            .collect();

        debug!(
            "[embeddings] Keyword memory query returned {} results for character {}",
            filtered.len(),
            character_id,
        );

        Ok(filtered)
    }

    // ── Cleanup ──────────────────────────────────────────────────────────

    /// Delete all embeddings for a conversation.
    pub async fn delete_for_conversation(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<(), MythicError> {
        db.query(
            "DELETE FROM message_embeddings \
             WHERE conversation_id = type::record('conversations', $conv_id)",
        )
        .bind(("conv_id", conversation_id.to_string()))
        .await?;

        Ok(())
    }
}

/// A message retrieved via vector similarity search.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetrievedContext {
    pub message_id: String,
    pub role: String,
    pub content: String,
    pub similarity: f64,
}

/// A memory fact retrieved via vector similarity search.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetrievedMemoryContext {
    pub memory_id: String,
    pub content: String,
    pub is_canon: bool,
    pub similarity: f64,
    /// Manual importance tier (1-10, default 5) — used to weight final
    /// ranking alongside relevance.
    pub importance: i32,
    /// When this memory was last surfaced via retrieval, if ever.
    pub last_accessed: Option<String>,
}

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::debug;

use crate::error::MythicError;

pub struct EmbeddingRepo;

impl EmbeddingRepo {
    /// Ensures the MTREE index exists with the correct dimension.
    /// If the index exists with a different dimension, it is dropped and recreated.
    pub async fn ensure_mtree_index(
        db: &Surreal<Db>,
        dimension: usize,
    ) -> Result<(), MythicError> {
        // Drop existing index (safe if it doesn't exist)
        let _ = db.query("REMOVE INDEX IF EXISTS idx_me_embedding ON message_embeddings").await;

        // Create with the correct dimension
        let query = format!(
            "DEFINE INDEX idx_me_embedding ON message_embeddings FIELDS embedding MTREE DIMENSION {} DIST COSINE TYPE F32",
            dimension
        );
        db.query(&query).await?.check()
            .map_err(|e| MythicError::DatabaseOp(format!("ensure_mtree_index({}): {}", dimension, e)))?;

        tracing::info!("[embeddings] MTREE index set to dimension {}", dimension);
        Ok(())
    }

    /// Returns the dimension of existing embeddings, or None if no embeddings exist.
    pub async fn get_index_dimension(
        db: &Surreal<Db>,
        conversation_id: Option<&str>,
    ) -> Result<Option<usize>, MythicError> {
        let query = match conversation_id {
            Some(conv_id) => format!(
                "SELECT dimension FROM message_embeddings WHERE conversation_id = type::thing('conversations', '{}') LIMIT 1",
                conv_id
            ),
            None => "SELECT dimension FROM message_embeddings LIMIT 1".to_string(),
        };

        let mut result = db.query(&query).await?;

        #[derive(serde::Deserialize)]
        struct DimRow { dimension: i64 }

        let rows: Vec<DimRow> = result.take(0)?;
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
        // Convert f64 to f32 for storage efficiency (MTREE index uses F32)
        let embedding_f32: Vec<f32> = embedding.iter().map(|&v| v as f32).collect();
        let dimension = embedding_f32.len() as i64;

        let query = if let Some(char_id) = character_id {
            db.query(
                "CREATE message_embeddings SET \
                    message_id = type::thing('messages', $msg_id), \
                    conversation_id = type::thing('conversations', $conv_id), \
                    character_id = type::thing('characters', $char_id), \
                    embedding = $embedding, \
                    model_name = $model, \
                    dimension = $dim, \
                    entry_type = 'message'"
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
                    message_id = type::thing('messages', $msg_id), \
                    conversation_id = type::thing('conversations', $conv_id), \
                    embedding = $embedding, \
                    model_name = $model, \
                    dimension = $dim, \
                    entry_type = 'message'"
            )
            .bind(("msg_id", message_id.to_string()))
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("embedding", embedding_f32))
            .bind(("model", model_name.to_string()))
            .bind(("dim", dimension))
            .await?
        };

        let _ = query; // consume the response

        debug!("[embeddings] Stored message embedding {} (dim={})", message_id, dimension);
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
                character_id = type::thing('characters', $char_id), \
                source_id = $source_id, \
                embedding = $embedding, \
                model_name = $model, \
                dimension = $dim, \
                entry_type = 'memory'"
        )
        .bind(("char_id", character_id.to_string()))
        .bind(("source_id", memory_id.to_string()))
        .bind(("embedding", embedding_f32))
        .bind(("model", model_name.to_string()))
        .bind(("dim", dimension))
        .await?;

        debug!("[embeddings] Stored memory embedding {} (dim={})", memory_id, dimension);
        Ok(())
    }

    /// Check if an embedding exists for a memory.
    pub async fn memory_exists(
        db: &Surreal<Db>,
        memory_id: &str,
    ) -> Result<bool, MythicError> {
        let mut result = db
            .query(
                "SELECT count() FROM message_embeddings \
                 WHERE source_id = $source_id AND entry_type = 'memory' \
                 GROUP ALL"
            )
            .bind(("source_id", memory_id.to_string()))
            .await?;

        let count: Option<serde_json::Value> = result.take(0)?;
        Ok(count.and_then(|v| v.get("count").and_then(|c| c.as_u64())).unwrap_or(0) > 0)
    }

    /// Delete embedding for a memory (when memory is deleted).
    pub async fn delete_memory_embedding(
        db: &Surreal<Db>,
        memory_id: &str,
    ) -> Result<(), MythicError> {
        db.query(
            "DELETE FROM message_embeddings \
             WHERE source_id = $source_id AND entry_type = 'memory'"
        )
        .bind(("source_id", memory_id.to_string()))
        .await?;

        debug!("[embeddings] Deleted memory embedding for {}", memory_id);
        Ok(())
    }

    // ── Existence Check: Messages ────────────────────────────────────────

    /// Check if an embedding exists for a message.
    pub async fn exists(
        db: &Surreal<Db>,
        message_id: &str,
    ) -> Result<bool, MythicError> {
        let mut result = db
            .query(
                "SELECT count() FROM message_embeddings \
                 WHERE message_id = type::thing('messages', $msg_id) \
                 GROUP ALL"
            )
            .bind(("msg_id", message_id.to_string()))
            .await?;

        let count: Option<serde_json::Value> = result.take(0)?;
        Ok(count.and_then(|v| v.get("count").and_then(|c| c.as_u64())).unwrap_or(0) > 0)
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

        // Build exclude list for SurrealQL
        let exclude_things: Vec<String> = exclude_message_ids
            .iter()
            .map(|id| format!("type::thing('messages', '{}')", id))
            .collect();
        let exclude_expr = if exclude_things.is_empty() {
            String::new()
        } else {
            format!(" AND message_id NOT IN [{}]", exclude_things.join(", "))
        };

        // Build scope filter
        let scope_filter = match (conversation_id, character_id) {
            (Some(_conv_id), _) => {
                "conversation_id = type::thing('conversations', $scope_id)".to_string()
            }
            (None, Some(_char_id)) => {
                "character_id = type::thing('characters', $scope_id)".to_string()
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
                {exclude_expr} \
             ORDER BY similarity DESC \
             LIMIT $top_k"
        );

        let mut result = db
            .query(&query)
            .bind(("scope_id", scope_id.to_string()))
            .bind(("query_vec", query_f32))
            .bind(("min_sim", min_similarity as f32))
            .bind(("top_k", top_k as i64))
            .await?;

        #[derive(serde::Deserialize, Debug)]
        struct EmbeddingHit {
            message_id: surrealdb::sql::Thing,
            similarity: f64,
        }

        let hits: Vec<EmbeddingHit> = result.take(0)?;

        // For each hit, fetch the actual message content
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            let msg_id = hit.message_id.id.to_raw();
            let mut msg_result = db
                .query("SELECT role, content FROM type::thing('messages', $id)")
                .bind(("id", msg_id.clone()))
                .await?;

            #[derive(serde::Deserialize)]
            struct MsgContent {
                role: String,
                content: String,
            }

            if let Ok(Some(msg)) = msg_result.take::<Option<MsgContent>>(0) {
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

    // ── Query: Memories ──────────────────────────────────────────────────

    /// Query top-K semantically similar memories for a character.
    /// Returns memory content and similarity scores.
    pub async fn query_memory_similar(
        db: &Surreal<Db>,
        character_id: &str,
        query_embedding: &[f64],
        top_k: usize,
        min_similarity: f64,
    ) -> Result<Vec<RetrievedMemoryContext>, MythicError> {
        let query_f32: Vec<f32> = query_embedding.iter().map(|&v| v as f32).collect();

        let mut result = db
            .query(
                "SELECT \
                    source_id, \
                    vector::similarity::cosine(embedding, $query_vec) AS similarity \
                 FROM message_embeddings \
                 WHERE character_id = type::thing('characters', $char_id) \
                    AND entry_type = 'memory' \
                    AND vector::similarity::cosine(embedding, $query_vec) >= $min_sim \
                 ORDER BY similarity DESC \
                 LIMIT $top_k"
            )
            .bind(("char_id", character_id.to_string()))
            .bind(("query_vec", query_f32))
            .bind(("min_sim", min_similarity as f32))
            .bind(("top_k", top_k as i64))
            .await?;

        #[derive(serde::Deserialize, Debug)]
        struct MemoryHit {
            source_id: String,
            similarity: f64,
        }

        let hits: Vec<MemoryHit> = result.take(0)?;

        // Fetch memory content for each hit
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            let mut mem_result = db
                .query("SELECT content, is_canon FROM type::thing('memories', $id)")
                .bind(("id", hit.source_id.clone()))
                .await?;

            #[derive(serde::Deserialize)]
            struct MemContent {
                content: String,
                is_canon: bool,
            }

            if let Ok(Some(mem)) = mem_result.take::<Option<MemContent>>(0) {
                results.push(RetrievedMemoryContext {
                    memory_id: hit.source_id,
                    content: mem.content,
                    is_canon: mem.is_canon,
                    similarity: hit.similarity,
                });
            }
        }

        debug!(
            "[embeddings] Memory query returned {} results for character {}",
            results.len(), character_id,
        );

        Ok(results)
    }

    // ── Cleanup ──────────────────────────────────────────────────────────

    /// Delete all embeddings for a conversation.
    pub async fn delete_for_conversation(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<(), MythicError> {
        db.query(
            "DELETE FROM message_embeddings \
             WHERE conversation_id = type::thing('conversations', $conv_id)"
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
}

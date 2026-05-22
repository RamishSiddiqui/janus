use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::debug;

use crate::error::MythicError;

pub struct EmbeddingRepo;

impl EmbeddingRepo {
    /// Store an embedding for a message.
    pub async fn store(
        db: &Surreal<Db>,
        message_id: &str,
        conversation_id: &str,
        embedding: &[f64],
        model_name: &str,
    ) -> Result<(), MythicError> {
        // Convert f64 to f32 for storage efficiency (MTREE index uses F32)
        let embedding_f32: Vec<f32> = embedding.iter().map(|&v| v as f32).collect();

        db.query(
            "CREATE message_embeddings SET \
                message_id = type::thing('messages', $msg_id), \
                conversation_id = type::thing('conversations', $conv_id), \
                embedding = $embedding, \
                model_name = $model"
        )
        .bind(("msg_id", message_id.to_string()))
        .bind(("conv_id", conversation_id.to_string()))
        .bind(("embedding", embedding_f32))
        .bind(("model", model_name.to_string()))
        .await?;

        debug!("[embeddings] Stored embedding for message {}", message_id);
        Ok(())
    }

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

    /// Query top-K similar messages in a conversation using cosine similarity.
    /// Returns (message_id, role, content, similarity_score) tuples.
    pub async fn query_similar(
        db: &Surreal<Db>,
        conversation_id: &str,
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

        let query = format!(
            "SELECT \
                message_id, \
                vector::similarity::cosine(embedding, $query_vec) AS similarity \
             FROM message_embeddings \
             WHERE conversation_id = type::thing('conversations', $conv_id) \
                AND vector::similarity::cosine(embedding, $query_vec) >= $min_sim \
                {exclude_expr} \
             ORDER BY similarity DESC \
             LIMIT $top_k"
        );

        let mut result = db
            .query(&query)
            .bind(("conv_id", conversation_id.to_string()))
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
            "[embeddings] Query returned {} results for conversation {}",
            results.len(), conversation_id
        );

        Ok(results)
    }

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

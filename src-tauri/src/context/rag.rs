//! Vector RAG (Retrieval-Augmented Generation) for conversation context.
//!
//! Two entry points:
//! - `embed_and_store()` — called asynchronously after each message is saved
//! - `query_relevant_context()` — called during `build_prompt()` to retrieve relevant history

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::{debug, info, warn};

use crate::db::embeddings::{EmbeddingRepo, RetrievedContext};
use crate::error::MythicError;
use crate::providers::unified::RigProvider;

/// Embeds a message and stores the embedding in the database.
/// Designed to be called in a background `tokio::spawn` — failures are
/// logged but never propagated (the chat continues without RAG).
pub async fn embed_and_store(
    db: &Surreal<Db>,
    provider: &RigProvider,
    embedding_model: &str,
    message_id: &str,
    conversation_id: &str,
    content: &str,
) -> Result<(), MythicError> {
    // Skip empty content
    if content.trim().is_empty() {
        return Ok(());
    }

    // Skip if already embedded
    match EmbeddingRepo::exists(db, message_id).await {
        Ok(true) => {
            debug!("[rag] Embedding already exists for message {}", message_id);
            return Ok(());
        }
        Err(e) => {
            warn!("[rag] Failed to check embedding existence: {}", e);
            // Continue anyway — worst case we get a duplicate key error
        }
        _ => {}
    }

    // Generate embedding
    let embeddings = provider
        .generate_embedding(embedding_model, vec![content.to_string()])
        .await?;

    let embedding = embeddings.into_iter().next().ok_or_else(|| {
        MythicError::Provider("Embedding API returned empty result".to_string())
    })?;

    // Store
    EmbeddingRepo::store(db, message_id, conversation_id, &embedding, embedding_model).await?;

    info!(
        "[rag] Embedded message {} ({} dimensions)",
        message_id,
        embedding.len()
    );

    Ok(())
}

/// Queries the vector store for messages semantically relevant to the query text.
/// Used during `build_prompt()` to inject relevant older messages into context.
pub async fn query_relevant_context(
    db: &Surreal<Db>,
    provider: &RigProvider,
    embedding_model: &str,
    conversation_id: &str,
    query_text: &str,
    top_k: usize,
    min_similarity: f64,
    exclude_message_ids: &[String],
) -> Result<Vec<RetrievedContext>, MythicError> {
    if query_text.trim().is_empty() {
        return Ok(vec![]);
    }

    // Embed the query
    let embeddings = provider
        .generate_embedding(embedding_model, vec![query_text.to_string()])
        .await?;

    let query_embedding = embeddings.into_iter().next().ok_or_else(|| {
        MythicError::Provider("Embedding API returned empty result for query".to_string())
    })?;

    // Search
    let results = EmbeddingRepo::query_similar(
        db,
        conversation_id,
        &query_embedding,
        top_k,
        min_similarity,
        exclude_message_ids,
    )
    .await?;

    if !results.is_empty() {
        info!(
            "[rag] Retrieved {} relevant messages for conversation {} (top similarity: {:.2})",
            results.len(),
            conversation_id,
            results.first().map(|r| r.similarity).unwrap_or(0.0)
        );
    }

    Ok(results)
}

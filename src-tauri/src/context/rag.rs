//! Vector RAG (Retrieval-Augmented Generation) for conversation context.
//!
//! Three retrieval paths:
//! - `embed_and_store()` — embeds a chat message (background, after save)
//! - `embed_memory()` — embeds a memory fact (background, after extraction)
//! - `query_relevant_context()` — retrieves semantically relevant messages
//! - `query_relevant_memories()` — retrieves semantically relevant memory facts

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::{debug, info, warn};

use crate::db::embeddings::{EmbeddingRepo, RetrievedContext, RetrievedMemoryContext};
use crate::error::MythicError;
use crate::providers::unified::RigProvider;

/// Embeds a chat message and stores the embedding in the database.
/// Designed to be called in a background `tokio::spawn` — failures are
/// logged but never propagated (the chat continues without RAG).
pub async fn embed_and_store(
    db: &Surreal<Db>,
    provider: &RigProvider,
    embedding_model: &str,
    message_id: &str,
    conversation_id: &str,
    content: &str,
    character_id: Option<&str>,
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

    // Store with character_id for cross-conversation search
    EmbeddingRepo::store(
        db, message_id, conversation_id, &embedding, embedding_model, character_id,
    ).await?;

    info!(
        "[rag] Embedded message {} ({} dimensions)",
        message_id,
        embedding.len()
    );

    Ok(())
}

/// Embeds a memory fact and stores the embedding in the vector database.
/// This enables semantic retrieval of memories during prompt building,
/// replacing the recency-ordered list with relevance-ordered retrieval.
pub async fn embed_memory(
    db: &Surreal<Db>,
    provider: &RigProvider,
    embedding_model: &str,
    memory_id: &str,
    character_id: &str,
    content: &str,
) -> Result<(), MythicError> {
    if content.trim().is_empty() {
        return Ok(());
    }

    // Skip if already embedded
    match EmbeddingRepo::memory_exists(db, memory_id).await {
        Ok(true) => {
            debug!("[rag] Embedding already exists for memory {}", memory_id);
            return Ok(());
        }
        Err(e) => {
            warn!("[rag] Failed to check memory embedding existence: {}", e);
        }
        _ => {}
    }

    // Generate embedding
    let embeddings = provider
        .generate_embedding(embedding_model, vec![content.to_string()])
        .await?;

    let embedding = embeddings.into_iter().next().ok_or_else(|| {
        MythicError::Provider("Embedding API returned empty result for memory".to_string())
    })?;

    // Store as memory entry
    EmbeddingRepo::store_memory(db, memory_id, character_id, &embedding, embedding_model).await?;

    info!(
        "[rag] Embedded memory {} ({} dimensions)",
        memory_id,
        embedding.len()
    );

    Ok(())
}

/// Queries the vector store for messages semantically relevant to the query text.
/// Used during `build_prompt()` to inject relevant older messages into context.
///
/// Supports two scopes:
/// - **Conversation-scoped**: `conversation_id = Some(...)` — searches within one conversation
/// - **Character-scoped**: `character_id = Some(...)` — searches across all conversations
pub async fn query_relevant_context(
    db: &Surreal<Db>,
    provider: &RigProvider,
    embedding_model: &str,
    conversation_id: Option<&str>,
    character_id: Option<&str>,
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

    // Search with appropriate scope
    let results = EmbeddingRepo::query_similar(
        db,
        conversation_id,
        character_id,
        &query_embedding,
        top_k,
        min_similarity,
        exclude_message_ids,
    )
    .await?;

    if !results.is_empty() {
        info!(
            "[rag] Retrieved {} relevant messages (top similarity: {:.2})",
            results.len(),
            results.first().map(|r| r.similarity).unwrap_or(0.0)
        );
    }

    Ok(results)
}

/// Queries the vector store for memory facts semantically relevant to the query text.
/// Used during `build_prompt()` to replace the recency-ordered memory list with
/// relevance-ordered semantic retrieval.
pub async fn query_relevant_memories(
    db: &Surreal<Db>,
    provider: &RigProvider,
    embedding_model: &str,
    character_id: &str,
    query_text: &str,
    top_k: usize,
    min_similarity: f64,
) -> Result<Vec<RetrievedMemoryContext>, MythicError> {
    if query_text.trim().is_empty() {
        return Ok(vec![]);
    }

    // Embed the query
    let embeddings = provider
        .generate_embedding(embedding_model, vec![query_text.to_string()])
        .await?;

    let query_embedding = embeddings.into_iter().next().ok_or_else(|| {
        MythicError::Provider("Embedding API returned empty result for memory query".to_string())
    })?;

    // Search memory embeddings
    let results = EmbeddingRepo::query_memory_similar(
        db,
        character_id,
        &query_embedding,
        top_k,
        min_similarity,
    )
    .await?;

    if !results.is_empty() {
        info!(
            "[rag] Retrieved {} relevant memories (top similarity: {:.2})",
            results.len(),
            results.first().map(|r| r.similarity).unwrap_or(0.0)
        );
    }

    Ok(results)
}

//! Embedding index management commands for the Memory settings UI.
//!
//! Provides status checks and full index rebuilds for the vector embedding
//! system that powers RAG (Retrieval-Augmented Generation).

use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::db::embeddings::EmbeddingRepo;
use crate::db::providers::ProviderRepo;
use crate::error::MythicError;
use crate::AppState;

use super::chat::create_rig_provider;

/// Embedding index status for the frontend Memory settings.
#[derive(Clone, Debug, serde::Serialize)]
pub struct EmbeddingIndexStatus {
    /// Total messages across all conversations (or in a specific one)
    pub total_messages: usize,
    /// Messages that have embeddings stored
    pub embedded_messages: usize,
    /// The model name used for existing embeddings (None if no embeddings exist)
    pub index_model: Option<String>,
    /// Whether the index needs rebuilding (stored model != selected model)
    pub needs_rebuild: bool,
    /// Percentage of messages embedded (0-100)
    pub coverage_percent: f64,
    /// Dimension of existing embeddings (None if no embeddings exist)
    pub index_dimension: Option<usize>,
    /// Dimension of the currently selected embedding model (from known dimensions map)
    pub selected_dimension: Option<usize>,
    /// Whether dimensions mismatch between stored and selected model
    pub dimension_mismatch: bool,
}

/// Returns the known embedding dimension for common models.
fn get_model_dimension(model_id: &str) -> Option<usize> {
    let id = model_id.to_lowercase();
    if id.contains("text-embedding-3-small") { return Some(1536); }
    if id.contains("text-embedding-3-large") { return Some(3072); }
    if id.contains("text-embedding-ada-002") { return Some(1536); }
    if id.contains("nomic-embed-text") { return Some(768); }
    if id.contains("mxbai-embed-large") { return Some(1024); }
    if id.contains("all-minilm") { return Some(384); }
    if id.contains("bge-large") { return Some(1024); }
    if id.contains("bge-base") { return Some(768); }
    if id.contains("bge-m3") { return Some(1024); }
    if id.contains("gte-base") { return Some(768); }
    if id.contains("gte-large") { return Some(1024); }
    if id.contains("e5-large") { return Some(1024); }
    if id.contains("e5-base") { return Some(768); }
    if id.contains("embed-english-v3") || id.contains("embed-multilingual-v3") { return Some(1024); }
    if id.contains("gemini-embedding") { return Some(768); }
    if id.contains("mistral-embed") { return Some(1024); }
    if id.contains("codestral-embed") { return Some(1024); }
    if id.contains("nemotron-embed") { return Some(4096); }
    if id.contains("pplx-embed") { return Some(4096); }
    if id.contains("qwen3-embedding-8b") { return Some(4096); }
    if id.contains("qwen3-embedding-4b") { return Some(2048); }
    if id.contains("multi-qa-mpnet") { return Some(768); }
    if id.contains("all-mpnet") { return Some(768); }
    if id.contains("paraphrase-minilm") { return Some(384); }
    if id.contains("m2-bert") { return Some(768); }
    None
}

/// Inner helper that operates on a raw `Surreal<Db>` reference.
/// Shared between the Tauri command and `rebuild_embedding_index`.
async fn get_embedding_index_status_inner(
    db: &Surreal<Db>,
    conversation_id: Option<String>,
    selected_model: Option<String>,
) -> Result<EmbeddingIndexStatus, MythicError> {
    // Count total user/assistant messages (optionally filtered by conversation)
    let total_query = match &conversation_id {
        Some(conv_id) => format!(
            "SELECT count() FROM messages WHERE conversation_id = type::thing('conversations', '{}') AND role IN ['user', 'assistant'] GROUP ALL",
            conv_id
        ),
        None => "SELECT count() FROM messages WHERE role IN ['user', 'assistant'] GROUP ALL".to_string(),
    };
    let mut total_result = db.query(&total_query).await?;
    let total_val: Option<serde_json::Value> = total_result.take(0)?;
    let total_messages = total_val
        .and_then(|v| v.get("count").and_then(|c| c.as_u64()))
        .unwrap_or(0) as usize;

    // Count embedded messages
    let embedded_query = match &conversation_id {
        Some(conv_id) => format!(
            "SELECT count() FROM message_embeddings WHERE conversation_id = type::thing('conversations', '{}') GROUP ALL",
            conv_id
        ),
        None => "SELECT count() FROM message_embeddings GROUP ALL".to_string(),
    };
    let mut embedded_result = db.query(&embedded_query).await?;
    let embedded_val: Option<serde_json::Value> = embedded_result.take(0)?;
    let embedded_messages = embedded_val
        .and_then(|v| v.get("count").and_then(|c| c.as_u64()))
        .unwrap_or(0) as usize;

    // Get the model used for existing embeddings (check first row)
    let model_query = match &conversation_id {
        Some(conv_id) => format!(
            "SELECT model_name FROM message_embeddings WHERE conversation_id = type::thing('conversations', '{}') LIMIT 1",
            conv_id
        ),
        None => "SELECT model_name FROM message_embeddings LIMIT 1".to_string(),
    };
    let mut model_result = db.query(&model_query).await?;

    #[derive(serde::Deserialize)]
    struct ModelRow {
        model_name: String,
    }

    let model_rows: Vec<ModelRow> = model_result.take(0)?;
    let index_model = model_rows.into_iter().next().map(|r| r.model_name);

    // Get stored dimension from existing embeddings
    let index_dimension = EmbeddingRepo::get_index_dimension(db, conversation_id.as_deref()).await?;

    // Get the expected dimension for the selected model
    let selected_dimension = selected_model.as_deref().and_then(get_model_dimension);

    // Check if dimensions mismatch
    let dimension_mismatch = match (index_dimension, selected_dimension) {
        (Some(stored), Some(selected)) => stored != selected,
        _ => false,
    };

    // Check if rebuild is needed (stored model differs from the one the user selected,
    // or dimensions mismatch)
    let needs_rebuild = match (&index_model, &selected_model) {
        (Some(stored), Some(selected)) => stored != selected,
        _ => false,
    } || dimension_mismatch;

    let coverage_percent = if total_messages > 0 {
        (embedded_messages as f64 / total_messages as f64) * 100.0
    } else {
        0.0
    };

    Ok(EmbeddingIndexStatus {
        total_messages,
        embedded_messages,
        index_model,
        needs_rebuild,
        coverage_percent,
        index_dimension,
        selected_dimension,
        dimension_mismatch,
    })
}

/// Returns the current state of the vector embedding index.
///
/// The frontend uses this to show how many messages are embedded,
/// which model was used, and whether a rebuild is needed.
#[tauri::command]
pub async fn get_embedding_index_status(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: Option<String>,
    selected_model: Option<String>,
) -> Result<EmbeddingIndexStatus, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    drop(state_guard);

    get_embedding_index_status_inner(&db, conversation_id, selected_model).await
}

/// Rebuilds the embedding index by deleting existing embeddings and
/// re-embedding all user/assistant messages with the specified model.
///
/// Returns the updated `EmbeddingIndexStatus` when done.
#[tauri::command]
pub async fn rebuild_embedding_index(
    state: State<'_, Arc<RwLock<AppState>>>,
    _app: tauri::AppHandle,
    conversation_id: Option<String>,
    embedding_model: String,
) -> Result<EmbeddingIndexStatus, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    drop(state_guard);

    // Find the provider that has this embedding model enabled
    // (NOT the default LLM provider, which may not support embeddings)
    let all_enabled = ProviderRepo::list_enabled_models(&db, None).await?;
    let embedding_entry = all_enabled
        .iter()
        .find(|m| m.model_id == embedding_model && m.model_type == "embedding")
        .ok_or_else(|| MythicError::Config(format!(
            "Embedding model '{}' is not enabled. Go to AI Studio → Embedding Models and enable it.",
            embedding_model
        )))?;

    let provider_config = ProviderRepo::get(&db, &embedding_entry.provider_id).await?;
    let provider = create_rig_provider(&provider_config)?;

    // Delete existing embeddings for the scope
    match &conversation_id {
        Some(conv_id) => {
            EmbeddingRepo::delete_for_conversation(&db, conv_id).await?;
        }
        None => {
            db.query("DELETE FROM message_embeddings").await?;
        }
    }

    // Ensure the MTREE index matches the new model's dimension
    if let Some(dim) = get_model_dimension(&embedding_model) {
        EmbeddingRepo::ensure_mtree_index(&db, dim).await?;
    }

    // Fetch all user/assistant messages in scope
    let messages_query = match &conversation_id {
        Some(conv_id) => format!(
            "SELECT id, conversation_id, content FROM messages \
             WHERE conversation_id = type::thing('conversations', '{}') \
             AND role IN ['user', 'assistant'] \
             ORDER BY created_at",
            conv_id
        ),
        None => {
            "SELECT id, conversation_id, content FROM messages \
             WHERE role IN ['user', 'assistant'] \
             ORDER BY created_at"
                .to_string()
        }
    };

    let mut result = db.query(&messages_query).await?;

    #[derive(serde::Deserialize)]
    struct MsgRow {
        id: surrealdb::sql::Thing,
        conversation_id: surrealdb::sql::Thing,
        content: String,
    }

    let messages: Vec<MsgRow> = result.take(0)?;
    let total = messages.len();

    info!(
        "[rebuild_index] Rebuilding embedding index: {} messages with model {}",
        total, embedding_model
    );

    // Process in batches of 10
    let batch_size = 10;
    let mut embedded = 0;

    for chunk in messages.chunks(batch_size) {
        let texts: Vec<String> = chunk.iter().map(|m| m.content.clone()).collect();

        match provider.generate_embedding(&embedding_model, texts).await {
            Ok(embeddings) => {
                for (msg, embedding) in chunk.iter().zip(embeddings.iter()) {
                    let msg_id = msg.id.id.to_raw();
                    let conv_id = msg.conversation_id.id.to_raw();
                    let _ = EmbeddingRepo::store(
                        &db,
                        &msg_id,
                        &conv_id,
                        embedding,
                        &embedding_model,
                    )
                    .await;
                    embedded += 1;
                }
            }
            Err(e) => {
                tracing::warn!("[rebuild_index] Batch embedding failed: {}", e);
            }
        }
    }

    info!(
        "[rebuild_index] Done: {}/{} messages embedded",
        embedded, total
    );

    // Return updated status
    get_embedding_index_status_inner(&db, conversation_id, Some(embedding_model)).await
}

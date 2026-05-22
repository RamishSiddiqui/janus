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
use crate::error::MythicError;
use crate::AppState;

use super::chat::{create_rig_provider, get_default_llm_provider};

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

    // Check if rebuild is needed (stored model differs from the one the user selected)
    let needs_rebuild = match (&index_model, &selected_model) {
        (Some(stored), Some(selected)) => stored != selected,
        _ => false,
    };

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

    // Get the provider
    let provider_config = get_default_llm_provider(&db).await?;
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

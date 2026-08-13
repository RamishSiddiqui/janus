//! Embedding index management commands for the Memory settings UI.
//!
//! Provides status checks and full index rebuilds for the vector embedding
//! system that powers RAG (Retrieval-Augmented Generation).

use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::RwLock;
use tracing::{info, warn};

use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::db::embeddings::EmbeddingRepo;
use crate::db::providers::ProviderRepo;
use crate::error::MythicError;
use crate::AppState;

use super::chat::create_rig_provider;

/// Embedding index status for the frontend Memory settings.
#[derive(Clone, Debug, serde::Serialize, specta::Type)]
pub struct EmbeddingIndexStatus {
    /// Total messages across all conversations (or in a specific one)
    #[specta(type = u32)]
    pub total_messages: usize,
    /// Messages that have embeddings stored
    #[specta(type = u32)]
    pub embedded_messages: usize,
    /// The model name used for existing embeddings (None if no embeddings exist)
    pub index_model: Option<String>,
    /// Whether the index needs rebuilding (stored model != selected model)
    pub needs_rebuild: bool,
    /// Percentage of messages embedded (0-100)
    pub coverage_percent: f64,
    /// Dimension of existing embeddings (None if no embeddings exist)
    #[specta(type = Option<u32>)]
    pub index_dimension: Option<usize>,
    /// Dimension of the currently selected embedding model (from known dimensions map)
    #[specta(type = Option<u32>)]
    pub selected_dimension: Option<usize>,
    /// Whether dimensions mismatch between stored and selected model
    pub dimension_mismatch: bool,
}

/// Returns the known embedding dimension for common models.
pub(crate) fn get_model_dimension(model_id: &str) -> Option<usize> {
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
    if id.contains("nemotron-embed") { return Some(2048); }
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
    // Bound unconditionally — harmless when the query text for the `None`
    // branch doesn't reference $conv_id at all.
    let conv_bind = conversation_id.clone().unwrap_or_default();

    // Count total user/assistant messages (optionally filtered by conversation)
    let total_query = match &conversation_id {
        Some(_) => "SELECT count() FROM messages WHERE conversation_id = type::thing('conversations', $conv_id) AND role IN ['user', 'assistant'] GROUP ALL",
        None => "SELECT count() FROM messages WHERE role IN ['user', 'assistant'] GROUP ALL",
    };
    let mut total_result = db.query(total_query).bind(("conv_id", conv_bind.clone())).await?;
    let total_val: Option<serde_json::Value> = total_result.take(0)?;
    let total_messages = total_val
        .and_then(|v| v.get("count").and_then(|c| c.as_u64()))
        .unwrap_or(0) as usize;

    // Count embedded messages — entry_type filter is required here, not
    // optional: memory-fact embeddings (entry_type='memory') live in this
    // same table with no conversation_id, so the unscoped (None) branch
    // would otherwise count them as if they were embedded messages and
    // inflate "coverage" above what's actually indexed.
    let embedded_query = match &conversation_id {
        Some(_) => "SELECT count() FROM message_embeddings WHERE conversation_id = type::thing('conversations', $conv_id) AND entry_type = 'message' GROUP ALL",
        None => "SELECT count() FROM message_embeddings WHERE entry_type = 'message' GROUP ALL",
    };
    let mut embedded_result = db.query(embedded_query).bind(("conv_id", conv_bind.clone())).await?;
    let embedded_val: Option<serde_json::Value> = embedded_result.take(0)?;
    let embedded_messages = embedded_val
        .and_then(|v| v.get("count").and_then(|c| c.as_u64()))
        .unwrap_or(0) as usize;

    // Get the model used for existing embeddings (check first row)
    let model_query = match &conversation_id {
        Some(_) => "SELECT model_name FROM message_embeddings WHERE conversation_id = type::thing('conversations', $conv_id) AND entry_type = 'message' LIMIT 1",
        None => "SELECT model_name FROM message_embeddings WHERE entry_type = 'message' LIMIT 1",
    };
    let mut model_result = db.query(model_query).bind(("conv_id", conv_bind.clone())).await?;

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
#[specta::specta]
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
#[specta::specta]
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

    // Delete existing MESSAGE embeddings for the scope — never memory-fact
    // embeddings (entry_type='memory'), which live in the same table but
    // aren't what this function re-creates below. An unscoped `DELETE FROM
    // message_embeddings` here would silently wipe every character's
    // memory embeddings app-wide with no way to rebuild them afterward.
    match &conversation_id {
        Some(conv_id) => {
            EmbeddingRepo::delete_for_conversation(&db, conv_id).await?;
        }
        None => {
            db.query("DELETE FROM message_embeddings WHERE entry_type = 'message'").await?;
        }
    }

    // Ensure MTREE index if we know the dimension upfront
    let known_dim = get_model_dimension(&embedding_model);
    if let Some(dim) = known_dim {
        EmbeddingRepo::ensure_mtree_index(&db, dim).await?;
    }

    // Fetch all user/assistant messages in scope
    let messages_query = match &conversation_id {
        Some(_) => "SELECT id, conversation_id, content, created_at FROM messages \
             WHERE conversation_id = type::thing('conversations', $conv_id) \
             AND role IN ['user', 'assistant'] \
             ORDER BY created_at",
        None => "SELECT id, conversation_id, content, created_at FROM messages \
             WHERE role IN ['user', 'assistant'] \
             ORDER BY created_at",
    };

    let mut result = db.query(messages_query)
        .bind(("conv_id", conversation_id.clone().unwrap_or_default()))
        .await?;

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
    let mut mtree_ensured = known_dim.is_some();

    for chunk in messages.chunks(batch_size) {
        let texts: Vec<String> = chunk.iter().map(|m| m.content.clone()).collect();

        match provider.generate_embedding(&embedding_model, texts).await {
            Ok(embeddings) => {
                // On first successful batch, detect actual dimension and ensure MTREE
                if !mtree_ensured {
                    if let Some(first) = embeddings.first() {
                        let actual_dim = first.len();
                        info!("[rebuild_index] Detected embedding dimension: {}", actual_dim);
                        EmbeddingRepo::ensure_mtree_index(&db, actual_dim).await?;
                        mtree_ensured = true;
                    }
                }

                for (msg, embedding) in chunk.iter().zip(embeddings.iter()) {
                    let msg_id = msg.id.id.to_raw();
                    let conv_id = msg.conversation_id.id.to_raw();
                    if let Err(e) = EmbeddingRepo::store(
                        &db,
                        &msg_id,
                        &conv_id,
                        embedding,
                        &embedding_model,
                        None, // character_id — not available during bulk rebuild
                    ).await {
                        tracing::warn!("[rebuild_index] Failed to store embedding for message {}: {}", msg_id, e);
                        continue;
                    }
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

/// Finds messages that don't have embeddings yet and embeds them in batches.
/// This is a non-destructive catch-up — it only fills gaps without touching
/// existing embeddings. Designed to run on conversation load or periodically.
///
/// Returns the updated `EmbeddingIndexStatus` when done.
#[tauri::command]
#[specta::specta]
pub async fn backfill_missing_embeddings(
    state: State<'_, Arc<RwLock<AppState>>>,
    app: tauri::AppHandle,
    conversation_id: Option<String>,
) -> Result<EmbeddingIndexStatus, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    drop(state_guard);

    // Find the enabled embedding model
    let all_enabled = ProviderRepo::list_enabled_models(&db, None).await?;
    let embedding_entry = all_enabled
        .iter()
        .find(|m| m.model_type == "embedding")
        .ok_or_else(|| MythicError::Config(
            "No embedding model enabled. Go to AI Studio → Embedding Models and enable one.".to_string()
        ))?;

    let embedding_model = embedding_entry.model_id.clone();
    let provider_config = ProviderRepo::get(&db, &embedding_entry.provider_id).await?;
    let provider = create_rig_provider(&provider_config)?;

    // Two-query approach: SurrealDB subqueries with NOT IN are unreliable.
    let conv_bind = conversation_id.clone().unwrap_or_default();

    // 0) Purge orphaned message embeddings — a bug in the multi-character
    // response path used to delete the combined parent message and replace
    // it with per-segment rows, but kept embedding the (now-deleted) parent
    // id instead of the real segments. That created `message_embeddings`
    // rows pointing at messages that no longer exist: junk that both wastes
    // vector search results and inflates the "embedded" count in the index
    // status shown above, without indexing anything real. The bug itself is
    // fixed at the source (`spawn_embed_message` call sites in chat.rs), but
    // rows it already created need a one-time sweep — this is the natural
    // place, since it already runs a full embedded-vs-real diff.
    {
        #[derive(serde::Deserialize)]
        struct EmbeddingRow {
            id: surrealdb::sql::Thing,
            message_id: Option<surrealdb::sql::Thing>,
        }
        let mut all_embeddings_result = db
            .query("SELECT id, message_id FROM message_embeddings WHERE entry_type = 'message'")
            .await?;
        let all_embeddings: Vec<EmbeddingRow> = all_embeddings_result.take(0).unwrap_or_else(|e| {
            warn!("[backfill] Failed to deserialize message_embeddings rows during orphan sweep: {}", e);
            Vec::new()
        });

        let mut real_msg_ids_result = db.query("SELECT VALUE id FROM messages").await?;
        let real_msg_things: Vec<surrealdb::sql::Thing> = real_msg_ids_result.take(0).unwrap_or_else(|e| {
            warn!("[backfill] Failed to deserialize message ids during orphan sweep: {}", e);
            Vec::new()
        });
        let real_msg_ids: std::collections::HashSet<String> = real_msg_things
            .into_iter()
            .map(|t| format!("{}:{}", t.tb, t.id.to_raw()))
            .collect();

        let orphan_ids: Vec<String> = all_embeddings
            .into_iter()
            .filter(|e| {
                let full_id = e.message_id.as_ref().map(|t| format!("{}:{}", t.tb, t.id.to_raw()));
                match full_id {
                    Some(id) => !real_msg_ids.contains(&id),
                    None => true, // entry_type='message' but no message_id at all — also junk
                }
            })
            .map(|e| e.id.id.to_raw())
            .collect();

        if !orphan_ids.is_empty() {
            info!("[backfill] Purging {} orphaned message embedding(s) from deleted messages", orphan_ids.len());
            for orphan_id in &orphan_ids {
                let _ = db
                    .query("DELETE type::thing('message_embeddings', $id)")
                    .bind(("id", orphan_id.clone()))
                    .await;
            }
        }
    }

    // 1) Get all already-embedded message IDs. entry_type='message' matters
    // here beyond just correctness of the diff below — memory rows have no
    // message_id at all (NULL), and a NULL mixed into this Vec<Thing> would
    // fail deserialization entirely, silently degrading via
    // .unwrap_or_default() below to an empty "already embedded" set (which
    // would make backfill re-request embeddings for every message, every
    // time it runs).
    let embedded_ids_query = match &conversation_id {
        Some(_) => "SELECT VALUE message_id FROM message_embeddings WHERE conversation_id = type::thing('conversations', $conv_id) AND entry_type = 'message'",
        None => "SELECT VALUE message_id FROM message_embeddings WHERE entry_type = 'message'",
    };
    let mut embedded_result = db.query(embedded_ids_query).bind(("conv_id", conv_bind.clone())).await?;
    let embedded_things: Vec<surrealdb::sql::Thing> = embedded_result.take(0).unwrap_or_else(|e| {
        warn!("[backfill] Failed to deserialize already-embedded message ids — treating as none embedded, which will re-request embeddings for everything: {}", e);
        Vec::new()
    });
    let embedded_ids: std::collections::HashSet<String> = embedded_things
        .into_iter()
        .map(|t| format!("{}:{}", t.tb, t.id.to_raw()))
        .collect();

    // 2) Get all user/assistant messages
    let all_msgs_query = match &conversation_id {
        Some(_) => "SELECT id, conversation_id, character_id, content, created_at FROM messages \
             WHERE conversation_id = type::thing('conversations', $conv_id) \
             AND role IN ['user', 'assistant'] \
             AND content != '' \
             ORDER BY created_at",
        None => "SELECT id, conversation_id, character_id, content, created_at FROM messages \
             WHERE role IN ['user', 'assistant'] \
             AND content != '' \
             ORDER BY created_at",
    };

    let mut result = db.query(all_msgs_query).bind(("conv_id", conv_bind)).await?;

    #[derive(serde::Deserialize)]
    struct MsgRow {
        id: surrealdb::sql::Thing,
        conversation_id: surrealdb::sql::Thing,
        character_id: Option<surrealdb::sql::Thing>,
        content: String,
    }

    let all_messages: Vec<MsgRow> = result.take(0)?;

    // 3) Filter to only un-embedded messages
    let missing: Vec<MsgRow> = all_messages
        .into_iter()
        .filter(|m| {
            let full_id = format!("{}:{}", m.id.tb, m.id.id.to_raw());
            !embedded_ids.contains(&full_id)
        })
        .collect();

    let total_missing = missing.len();

    if total_missing == 0 {
        info!("[backfill] No missing embeddings found — index is complete");
        return get_embedding_index_status_inner(&db, conversation_id, Some(embedding_model)).await;
    }

    info!(
        "[backfill] Found {} un-embedded messages, processing in batches",
        total_missing
    );

    // Process in batches of 10
    let batch_size = 10;
    let mut embedded = 0;

    for chunk in missing.chunks(batch_size) {
        let texts: Vec<String> = chunk.iter().map(|m| m.content.clone()).collect();

        match provider.generate_embedding(&embedding_model, texts).await {
            Ok(embeddings) => {
                for (msg, embedding) in chunk.iter().zip(embeddings.iter()) {
                    let msg_id = msg.id.id.to_raw();
                    let conv_id = msg.conversation_id.id.to_raw();
                    let char_id = msg.character_id.as_ref().map(|c| c.id.to_raw());
                    if let Err(e) = EmbeddingRepo::store(
                        &db,
                        &msg_id,
                        &conv_id,
                        embedding,
                        &embedding_model,
                        char_id.as_deref(),
                    ).await {
                        tracing::warn!("[backfill] Failed to store embedding for message {}: {}", msg_id, e);
                        continue;
                    }
                    embedded += 1;
                }
                // Notify frontend after each batch
                let _ = app.emit("embedding_updated", ());
            }
            Err(e) => {
                tracing::warn!("[backfill] Batch embedding failed: {}", e);
            }
        }
    }

    info!(
        "[backfill] Done: {}/{} missing messages embedded",
        embedded, total_missing
    );

    get_embedding_index_status_inner(&db, conversation_id, Some(embedding_model)).await
}

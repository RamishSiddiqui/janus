use crate::events::EmitScoped;
use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use crate::auth::access::{
    ensure_character, ensure_conversation, ensure_message, ensure_owned, ensure_persona,
    stamp_owner,
};
use crate::commands::actor::acting;
use crate::db::conversations::ConversationRepo;
use crate::error::MythicError;
use crate::models::conversation::{Conversation, Message, SearchResult};
use crate::AppState;

/// Creates a new conversation for a character.
#[tauri::command]
#[specta::specta]
pub async fn create_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: Option<String>,
    title: Option<String>,
    persona_id: Option<String>,
) -> Result<Conversation, MythicError> {
    let (db, actor) = acting(&state).await?;
    if let Some(cid) = character_id.as_deref() {
        ensure_character(&db, &actor, cid).await?;
    }
    if let Some(pid) = persona_id.as_deref() {
        ensure_persona(&db, &actor, pid).await?;
    }
    let conversation = ConversationRepo::create(
        &db,
        character_id.as_deref(),
        title.as_deref(),
        persona_id.as_deref(),
    )
    .await?;
    stamp_owner(
        &db,
        &actor,
        "conversations",
        &crate::db::value_bridge::record_id_to_string(&conversation.id),
    )
    .await?;
    info!(
        "Created conversation: {} ({:?})",
        conversation.title, conversation.id
    );
    Ok(conversation)
}

/// Retrieves a single conversation by ID.
#[tauri::command]
#[specta::specta]
pub async fn get_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Conversation, MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &id).await?;
    ConversationRepo::get(&db, &id).await
}

/// Lists conversations with pagination, ordered by most recently updated.
#[tauri::command]
#[specta::specta]
pub async fn list_conversations(
    state: State<'_, Arc<RwLock<AppState>>>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<Conversation>, MythicError> {
    info!(
        "[CMD] list_conversations called (limit={:?}, offset={:?})",
        limit, offset
    );
    let (db, actor) = acting(&state).await?;
    let limit = limit.unwrap_or(50).min(200);
    let offset = offset.unwrap_or(0);
    match ConversationRepo::list(&db, limit, offset, actor.owner_filter()).await {
        Ok(convos) => {
            info!(
                "[CMD] list_conversations OK — returned {} conversations",
                convos.len()
            );
            Ok(convos)
        }
        Err(e) => {
            info!("[CMD] list_conversations FAILED: {:?}", e);
            Err(e)
        }
    }
}

/// Returns the total number of conversations (for pagination).
#[tauri::command]
#[specta::specta]
pub async fn count_conversations(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<u32, MythicError> {
    info!("[CMD] count_conversations called");
    let (db, actor) = acting(&state).await?;
    match ConversationRepo::count(&db, actor.owner_filter()).await {
        Ok(count) => {
            info!("[CMD] count_conversations OK — count={}", count);
            Ok(count)
        }
        Err(e) => {
            info!("[CMD] count_conversations FAILED: {:?}", e);
            Err(e)
        }
    }
}

/// Permanently deletes a conversation and all its messages (cascade). Only
/// the Trash view should call this — normal deletion from the chat list
/// should call `trash_conversation` instead.
#[tauri::command]
#[specta::specta]
pub async fn delete_conversation(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &id).await?;
    ConversationRepo::delete(&db, &id).await?;
    info!("Deleted conversation: {}", id);
    // The cascade event wipes this conversation's message_embeddings along
    // with everything else, but nothing tells the Settings page's Memory
    // panel that happened — reuse the same event the live embed path emits
    // so its index counts refresh instead of showing a stale total.
    let _ = app.emit_scoped("embedding_updated", ());
    Ok(())
}

/// Moves a conversation to Trash (soft delete) — reversible via
/// `restore_conversation`. Instant and durable the moment it returns, unlike
/// the old client-side undo-window that silently lost the delete if the app
/// reloaded before its timer fired.
#[tauri::command]
#[specta::specta]
pub async fn trash_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Conversation, MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &id).await?;
    let conv = ConversationRepo::trash(&db, &id).await?;
    info!("Trashed conversation: {}", id);
    Ok(conv)
}

/// Restores a trashed conversation.
#[tauri::command]
#[specta::specta]
pub async fn restore_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Conversation, MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &id).await?;
    let conv = ConversationRepo::restore(&db, &id).await?;
    info!("Restored conversation: {}", id);
    Ok(conv)
}

/// Retrieves all messages in a conversation, ordered chronologically.
/// Returns the linear message chain following the active branch.
#[tauri::command]
#[specta::specta]
pub async fn get_conversation_messages(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<Vec<Message>, MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &conversation_id).await?;
    ConversationRepo::get_messages(&db, &conversation_id).await
}

/// Updates the active message pointer for branch navigation.
#[tauri::command]
#[specta::specta]
pub async fn set_active_message(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    message_id: String,
) -> Result<(), MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &conversation_id).await?;
    ensure_message(&db, &actor, &message_id).await?;
    ConversationRepo::set_active_message(&db, &conversation_id, &message_id).await
}

/// Sets (or clears, passing `null`) this conversation's chosen
/// image-generation preset.
#[tauri::command]
#[specta::specta]
pub async fn set_conversation_image_preset(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    preset_id: Option<String>,
) -> Result<(), MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &conversation_id).await?;
    if let Some(pid) = preset_id.as_deref() {
        ensure_owned(&db, &actor, "image_presets", pid).await?;
    }
    ConversationRepo::set_image_preset(&db, &conversation_id, preset_id.as_deref()).await
}

/// Sets (or clears, passing `null`) this conversation's chosen persona.
#[tauri::command]
#[specta::specta]
pub async fn set_conversation_persona(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    persona_id: Option<String>,
) -> Result<(), MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &conversation_id).await?;
    if let Some(pid) = persona_id.as_deref() {
        ensure_persona(&db, &actor, pid).await?;
    }
    ConversationRepo::set_persona(&db, &conversation_id, persona_id.as_deref()).await
}

/// Updates a conversation's title.
#[tauri::command]
#[specta::specta]
pub async fn update_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    title: String,
) -> Result<Conversation, MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &id).await?;
    let conversation = ConversationRepo::update_title(&db, &id, &title).await?;
    info!("Updated conversation title: {} -> {}", id, title);
    Ok(conversation)
}

/// Updates the memory scope for a conversation.
#[tauri::command]
#[specta::specta]
pub async fn set_memory_scope(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    scope: String,
) -> Result<(), MythicError> {
    // Validate scope value
    if !matches!(scope.as_str(), "character" | "conversation" | "none") {
        return Err(MythicError::Config(format!(
            "Invalid memory scope '{}'. Must be 'character', 'conversation', or 'none'",
            scope
        )));
    }

    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &conversation_id).await?;
    ConversationRepo::set_memory_scope(&db, &conversation_id, &scope).await?;
    info!(
        "Set memory scope for conversation {} to '{}'",
        conversation_id, scope
    );
    Ok(())
}

/// Creates a new conversation that is a branch of an existing one.
///
/// The new conversation contains a full copy of all messages up to and including
/// `branch_point_message_id`, preserving the parent→child chain with fresh IDs.
///
/// All memories from the parent conversation are bulk-copied into the new conversation
/// using `copy` links, which render as dashed arrows in MemoryGraph/MemoryTimeline.
#[tauri::command]
#[specta::specta]
pub async fn branch_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    parent_conversation_id: String,
    branch_point_message_id: String,
    new_title: Option<String>,
) -> Result<Conversation, MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &parent_conversation_id).await?;
    ensure_message(&db, &actor, &branch_point_message_id).await?;
    let branch = ConversationRepo::branch(
        &db,
        &parent_conversation_id,
        &branch_point_message_id,
        new_title.as_deref(),
    )
    .await?;
    stamp_owner(
        &db,
        &actor,
        "conversations",
        &crate::db::value_bridge::record_id_to_string(&branch.id),
    )
    .await?;
    Ok(branch)
}

/// Searches message content using SurrealDB full-text search.
///
/// Returns results with highlighted snippets, conversation titles,
/// and character names for display in the search overlay.
#[tauri::command]
#[specta::specta]
pub async fn search_messages(
    state: State<'_, Arc<RwLock<AppState>>>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<SearchResult>, MythicError> {
    let (db, actor) = acting(&state).await?;
    let limit = limit.unwrap_or(20).min(100);

    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    ConversationRepo::search_messages(&db, &query, limit, actor.owner_filter()).await
}

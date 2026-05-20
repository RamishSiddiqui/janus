use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use crate::db::conversations::ConversationRepo;
use crate::error::MythicError;
use crate::models::conversation::{Conversation, Message, SearchResult};
use crate::AppState;

/// Creates a new conversation for a character.
#[tauri::command]
pub async fn create_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: Option<String>,
    title: Option<String>,
) -> Result<Conversation, MythicError> {
    let state = state.read().await;
    let conversation = ConversationRepo::create(
        &state.db,
        character_id.as_deref(),
        title.as_deref(),
    )
    .await?;
    info!("Created conversation: {} ({:?})", conversation.title, conversation.id);
    Ok(conversation)
}

/// Retrieves a single conversation by ID.
#[tauri::command]
pub async fn get_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Conversation, MythicError> {
    let state = state.read().await;
    ConversationRepo::get(&state.db, &id).await
}

/// Lists conversations with pagination, ordered by most recently updated.
#[tauri::command]
pub async fn list_conversations(
    state: State<'_, Arc<RwLock<AppState>>>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<Conversation>, MythicError> {
    info!("[CMD] list_conversations called (limit={:?}, offset={:?})", limit, offset);
    let state = state.read().await;
    let limit = limit.unwrap_or(50).min(200);
    let offset = offset.unwrap_or(0);
    match ConversationRepo::list(&state.db, limit, offset).await {
        Ok(convos) => {
            info!("[CMD] list_conversations OK — returned {} conversations", convos.len());
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
pub async fn count_conversations(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<u32, MythicError> {
    info!("[CMD] count_conversations called");
    let state = state.read().await;
    match ConversationRepo::count(&state.db).await {
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

/// Deletes a conversation and all its messages (cascade).
#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;
    ConversationRepo::delete(&state.db, &id).await?;
    info!("Deleted conversation: {}", id);
    Ok(())
}

/// Retrieves all messages in a conversation, ordered chronologically.
/// Returns the linear message chain following the active branch.
#[tauri::command]
pub async fn get_conversation_messages(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<Vec<Message>, MythicError> {
    let state = state.read().await;
    ConversationRepo::get_messages(&state.db, &conversation_id).await
}

/// Updates the active message pointer for branch navigation.
#[tauri::command]
pub async fn set_active_message(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    message_id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;
    ConversationRepo::set_active_message(&state.db, &conversation_id, &message_id).await
}

/// Updates a conversation's title.
#[tauri::command]
pub async fn update_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    title: String,
) -> Result<Conversation, MythicError> {
    let state = state.read().await;
    let conversation = ConversationRepo::update_title(&state.db, &id, &title).await?;
    info!("Updated conversation title: {} -> {}", id, title);
    Ok(conversation)
}

/// Updates the memory scope for a conversation.
#[tauri::command]
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

    let state = state.read().await;
    ConversationRepo::set_memory_scope(&state.db, &conversation_id, &scope).await?;
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
pub async fn branch_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    parent_conversation_id: String,
    branch_point_message_id: String,
    new_title: Option<String>,
) -> Result<Conversation, MythicError> {
    let state = state.read().await;
    ConversationRepo::branch(
        &state.db,
        &parent_conversation_id,
        &branch_point_message_id,
        new_title.as_deref(),
    )
    .await
}

/// Searches message content using SurrealDB full-text search.
///
/// Returns results with highlighted snippets, conversation titles,
/// and character names for display in the search overlay.
#[tauri::command]
pub async fn search_messages(
    state: State<'_, Arc<RwLock<AppState>>>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<SearchResult>, MythicError> {
    let state = state.read().await;
    let limit = limit.unwrap_or(20).min(100);

    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    ConversationRepo::search_messages(&state.db, &query, limit).await
}

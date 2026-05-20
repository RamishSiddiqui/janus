//! Memory management commands — CRUD + sharing + graph for the multiverse memory system.
//!
//! All database operations are delegated to `MemoryRepo`. This layer handles
//! Tauri state extraction and input validation only.

use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use crate::db::memories::MemoryRepo;
use crate::error::MythicError;
use crate::models::memory::{Memory, MemoryGraph, MemoryLink};
use crate::AppState;

/// Lists memories for a character and/or conversation.
#[tauri::command]
pub async fn list_memories(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: Option<String>,
    conversation_id: Option<String>,
) -> Result<Vec<Memory>, MythicError> {
    let state = state.read().await;
    MemoryRepo::list(
        &state.db,
        character_id.as_deref(),
        conversation_id.as_deref(),
    )
    .await
}

/// Creates a new memory entry.
#[tauri::command]
pub async fn create_memory(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: Option<String>,
    conversation_id: Option<String>,
    content: String,
    source: Option<String>,
) -> Result<Memory, MythicError> {
    let state = state.read().await;
    let source = source.unwrap_or_else(|| "user".to_string());
    let memory = MemoryRepo::create(
        &state.db,
        character_id.as_deref(),
        conversation_id.as_deref(),
        &content,
        &source,
    )
    .await?;
    info!("Created memory: {} (source: {})", memory.id, source);
    Ok(memory)
}

/// Updates a memory's content and increments its version.
#[tauri::command]
pub async fn update_memory(
    state: State<'_, Arc<RwLock<AppState>>>,
    memory_id: String,
    content: String,
) -> Result<Memory, MythicError> {
    let state = state.read().await;
    let memory = MemoryRepo::update(&state.db, &memory_id, &content).await?;
    info!("Updated memory: {} (version incremented)", memory_id);
    Ok(memory)
}

/// Deletes a memory entry.
#[tauri::command]
pub async fn delete_memory(
    state: State<'_, Arc<RwLock<AppState>>>,
    memory_id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;
    MemoryRepo::delete(&state.db, &memory_id).await?;
    info!("Deleted memory: {}", memory_id);
    Ok(())
}

/// Promotes a conversation-level memory to character-level canon.
/// The memory becomes visible across all conversations with this character.
#[tauri::command]
pub async fn promote_to_canon(
    state: State<'_, Arc<RwLock<AppState>>>,
    memory_id: String,
) -> Result<Memory, MythicError> {
    let state = state.read().await;
    let memory = MemoryRepo::promote_to_canon(&state.db, &memory_id).await?;
    info!("Promoted memory {} to canon", memory_id);
    Ok(memory)
}

/// Shares a memory to another conversation by creating a link.
///
/// For 'copy' links: creates a duplicate memory in the target conversation.
/// For 'sync' links: just creates the link (no duplicate).
#[tauri::command]
pub async fn share_memory(
    state: State<'_, Arc<RwLock<AppState>>>,
    source_memory_id: String,
    target_conversation_id: String,
    link_type: Option<String>,
    direction: Option<String>,
    sync_mode: Option<String>,
) -> Result<MemoryLink, MythicError> {
    let link_type = link_type.unwrap_or_else(|| "copy".to_string());
    let direction = direction.unwrap_or_else(|| "one_way".to_string());
    let sync_mode = sync_mode.unwrap_or_else(|| "manual".to_string());

    // Validate link parameters
    if !matches!(link_type.as_str(), "copy" | "sync") {
        return Err(MythicError::Config("link_type must be 'copy' or 'sync'".to_string()));
    }
    if !matches!(direction.as_str(), "one_way" | "two_way") {
        return Err(MythicError::Config("direction must be 'one_way' or 'two_way'".to_string()));
    }
    if !matches!(sync_mode.as_str(), "auto" | "manual") {
        return Err(MythicError::Config("sync_mode must be 'auto' or 'manual'".to_string()));
    }

    let state = state.read().await;
    let link = MemoryRepo::share(
        &state.db,
        &source_memory_id,
        &target_conversation_id,
        &link_type,
        &direction,
        &sync_mode,
    )
    .await?;

    info!(
        "Shared memory {} to conversation {} (type: {}, direction: {}, sync: {})",
        source_memory_id, target_conversation_id, link_type, direction, sync_mode
    );
    Ok(link)
}

/// Removes a sharing link between conversations.
#[tauri::command]
pub async fn unlink_memory(
    state: State<'_, Arc<RwLock<AppState>>>,
    link_id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;
    MemoryRepo::unlink(&state.db, &link_id).await?;
    info!("Removed memory link: {}", link_id);
    Ok(())
}

/// Returns the full memory graph for a character — all memories, links,
/// and conversations, structured for the frontend graph canvas.
#[tauri::command]
pub async fn get_memory_graph(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
) -> Result<MemoryGraph, MythicError> {
    let state = state.read().await;
    MemoryRepo::get_graph(&state.db, &character_id).await
}

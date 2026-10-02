use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use crate::auth::access::{ensure_conversation, ensure_message};
use crate::commands::actor::acting;
use crate::db::messages::MessageRepo;
use crate::error::MythicError;
use crate::models::conversation::Message;
use crate::models::DynamicJson;
use crate::AppState;

/// Creates a new message in a conversation.
#[tauri::command]
#[specta::specta]
pub async fn create_message(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    role: String,
    content: String,
    parent_id: Option<String>,
    metadata: Option<DynamicJson>,
) -> Result<Message, MythicError> {
    let role_str = match role.as_str() {
        "user" | "assistant" | "system" => role.as_str(),
        _ => return Err(MythicError::Validation(format!("Invalid role: {}", role))),
    };

    let (db, actor) = acting(&state).await?;
    ensure_conversation(&db, &actor, &conversation_id).await?;
    if let Some(pid) = parent_id.as_deref() {
        ensure_message(&db, &actor, pid).await?;
    }
    let message = MessageRepo::create(
        &db,
        &conversation_id,
        role_str,
        &content,
        parent_id.as_deref(),
        metadata.map(|m| m.0),
    )
    .await?;

    info!(
        "Created {} message in conversation {}",
        role_str, conversation_id
    );
    Ok(message)
}

/// Updates a message's content (for edits).
#[tauri::command]
#[specta::specta]
pub async fn update_message(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    content: String,
) -> Result<Message, MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_message(&db, &actor, &id).await?;
    let message = MessageRepo::update(&db, &id, &content).await?;
    info!("Updated message: {}", id);
    Ok(message)
}

/// Deletes a message by ID.
#[tauri::command]
#[specta::specta]
pub async fn delete_message(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_message(&db, &actor, &id).await?;
    MessageRepo::delete(&db, &id).await?;
    info!("Deleted message: {}", id);
    Ok(())
}

/// Walks the parent_id chain to reconstruct the linear message history
/// from root to the given message. Used for building the LLM prompt.
#[tauri::command]
#[specta::specta]
pub async fn get_message_branch(
    state: State<'_, Arc<RwLock<AppState>>>,
    message_id: String,
) -> Result<Vec<Message>, MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_message(&db, &actor, &message_id).await?;
    MessageRepo::get_branch(&db, &message_id).await
}

/// Returns all sibling messages (messages sharing the same parent_id).
/// Used for branch navigation — shows alternates at the same conversation point.
#[tauri::command]
#[specta::specta]
pub async fn get_message_siblings(
    state: State<'_, Arc<RwLock<AppState>>>,
    message_id: String,
) -> Result<Vec<Message>, MythicError> {
    let (db, actor) = acting(&state).await?;
    ensure_message(&db, &actor, &message_id).await?;
    MessageRepo::get_siblings(&db, &message_id).await
}

//! Conversation character management — add, remove, list, configure characters in a conversation.

use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

use crate::db::conversation_characters::ConversationCharacterRepo;
use crate::error::MythicError;
use crate::models::conversation_character::ConversationCharacter;
use crate::AppState;

/// Lists all characters in a conversation.
#[tauri::command]
#[specta::specta]
pub async fn list_conversation_characters(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<Vec<ConversationCharacter>, MythicError> {
    let g = state.read().await;
    ConversationCharacterRepo::list(&g.db, &conversation_id).await
}

/// Adds a character to a conversation.
#[tauri::command]
#[specta::specta]
pub async fn add_conversation_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    character_id: String,
    character_name: String,
    role: Option<String>,
    talkativeness: Option<i32>,
) -> Result<ConversationCharacter, MythicError> {
    let role = role.unwrap_or_else(|| "secondary".to_string());
    let talkativeness = talkativeness.unwrap_or(50);
    let g = state.read().await;
    ConversationCharacterRepo::add(
        &g.db, &conversation_id, &character_id, &character_name, &role, talkativeness,
    ).await
}

/// Removes a character from a conversation.
#[tauri::command]
#[specta::specta]
pub async fn remove_conversation_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    character_id: String,
) -> Result<(), MythicError> {
    let g = state.read().await;
    ConversationCharacterRepo::remove(&g.db, &conversation_id, &character_id).await
}

/// Updates a character's talkativeness in a conversation.
#[tauri::command]
#[specta::specta]
pub async fn update_character_talkativeness(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    character_id: String,
    talkativeness: i32,
) -> Result<(), MythicError> {
    let g = state.read().await;
    ConversationCharacterRepo::update_talkativeness(
        &g.db, &conversation_id, &character_id, talkativeness,
    ).await
}

/// Toggles whether a character is active (unmuted) in a conversation.
#[tauri::command]
#[specta::specta]
pub async fn toggle_character_active(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    character_id: String,
    is_active: bool,
) -> Result<(), MythicError> {
    let g = state.read().await;
    ConversationCharacterRepo::set_active(
        &g.db, &conversation_id, &character_id, is_active,
    ).await
}

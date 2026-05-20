use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;

use crate::db::lorebook::LorebookRepo;
use crate::error::MythicError;
use crate::models::lorebook::LorebookEntry;
use crate::AppState;

/// Lists all lorebook entries for a character (plus global entries).
#[tauri::command]
pub async fn list_lorebook_entries(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
) -> Result<Vec<LorebookEntry>, MythicError> {
    let state_guard = state.read().await;
    LorebookRepo::list(&state_guard.db, &character_id).await
}

/// Creates a new lorebook entry.
#[tauri::command]
pub async fn create_lorebook_entry(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: Option<String>,
    name: String,
    keys: Vec<String>,
    content: String,
    always_active: bool,
) -> Result<LorebookEntry, MythicError> {
    let state_guard = state.read().await;
    LorebookRepo::create(
        &state_guard.db,
        character_id.as_deref(),
        &name,
        keys,
        &content,
        always_active,
    )
    .await
}

/// Toggles a lorebook entry's enabled state.
#[tauri::command]
pub async fn toggle_lorebook_entry(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    enabled: bool,
) -> Result<(), MythicError> {
    let state_guard = state.read().await;
    LorebookRepo::toggle(&state_guard.db, &id, enabled).await
}

/// Deletes a lorebook entry.
#[tauri::command]
pub async fn delete_lorebook_entry(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state_guard = state.read().await;
    LorebookRepo::delete(&state_guard.db, &id).await
}

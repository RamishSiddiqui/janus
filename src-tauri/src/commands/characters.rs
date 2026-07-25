use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use crate::db::characters::CharacterRepo;
use crate::error::{MythicError, validate_required_string};
use crate::models::character::Character;
use crate::models::DynamicJson;
use crate::AppState;

/// Creates a new character from a Character Card V2 payload.
#[tauri::command]
#[specta::specta]
pub async fn create_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    name: String,
    data: DynamicJson,
) -> Result<Character, MythicError> {
    validate_required_string("Character name", &name, 200)?;
    let state = state.read().await;
    let character = CharacterRepo::create(&state.db, &name, data.0).await?;
    info!("Created character: {} ({})", name, character.id);
    Ok(character)
}

/// Retrieves a single character by ID.
#[tauri::command]
#[specta::specta]
pub async fn get_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Character, MythicError> {
    if id.is_empty() {
        return Err(MythicError::Validation("Character ID is required".into()));
    }
    let state = state.read().await;
    CharacterRepo::get(&state.db, &id).await
}

/// Lists all characters, ordered by most recently updated.
#[tauri::command]
#[specta::specta]
pub async fn list_characters(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<Character>, MythicError> {
    let state = state.read().await;
    CharacterRepo::list(&state.db).await
}

/// Updates an existing character's data.
#[tauri::command]
#[specta::specta]
pub async fn update_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    name: Option<String>,
    data: Option<DynamicJson>,
    avatar_path: Option<String>,
) -> Result<Character, MythicError> {
    if id.is_empty() {
        return Err(MythicError::Validation("Character ID is required".into()));
    }
    if let Some(ref name) = name {
        validate_required_string("Character name", name, 200)?;
    }
    let state = state.read().await;
    let character = CharacterRepo::update(
        &state.db,
        &id,
        name.as_deref(),
        data.map(|d| d.0),
        avatar_path.as_deref(),
    )
    .await?;
    info!("Updated character: {}", id);
    Ok(character)
}

/// Deletes a character by ID. Cascades are handled by SurrealDB events.
#[tauri::command]
#[specta::specta]
pub async fn delete_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    if id.is_empty() {
        return Err(MythicError::Validation("Character ID is required".into()));
    }
    let state = state.read().await;
    CharacterRepo::delete(&state.db, &id).await?;
    info!("Deleted character: {}", id);
    Ok(())
}

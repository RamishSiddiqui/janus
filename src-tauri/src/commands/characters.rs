use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
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

/// Permanently deletes a character by ID. Cascades are handled by SurrealDB
/// events. Only the Trash view should call this — normal deletion from
/// Gallery should call `trash_character` instead.
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

/// Moves a character to Trash (soft delete) — reversible via `restore_character`.
#[tauri::command]
#[specta::specta]
pub async fn trash_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Character, MythicError> {
    if id.is_empty() {
        return Err(MythicError::Validation("Character ID is required".into()));
    }
    let state = state.read().await;
    let character = CharacterRepo::trash(&state.db, &id).await?;
    info!("Trashed character: {}", id);
    Ok(character)
}

/// Restores a trashed character.
#[tauri::command]
#[specta::specta]
pub async fn restore_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Character, MythicError> {
    if id.is_empty() {
        return Err(MythicError::Validation("Character ID is required".into()));
    }
    let state = state.read().await;
    let character = CharacterRepo::restore(&state.db, &id).await?;
    info!("Restored character: {}", id);
    Ok(character)
}

/// Sets a character's portrait directly from a user-picked image file,
/// bypassing AI generation entirely — the "Upload Portrait" counterpart to
/// `generate_npc_portrait`. Always marks the result "approved" (a manually
/// chosen image needs no review gate). Copies the file into the app data
/// dir's `portraits/` folder, same location and naming (`{character_id}.png`)
/// AI-generated portraits use, so both paths are interchangeable afterward.
#[tauri::command]
#[specta::specta]
pub async fn upload_character_avatar(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
    file_path: String,
) -> Result<Character, MythicError> {
    if character_id.is_empty() {
        return Err(MythicError::Validation("Character ID is required".into()));
    }
    let source = std::path::PathBuf::from(&file_path);
    if !source.exists() {
        return Err(MythicError::NotFound(format!("File not found: {}", file_path)));
    }
    let image_bytes = tokio::fs::read(&source).await?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    let portraits_dir = app_data_dir.join("portraits");
    tokio::fs::create_dir_all(&portraits_dir).await?;
    let filename = format!("{}.png", character_id);
    let dest = portraits_dir.join(&filename);
    tokio::fs::write(&dest, &image_bytes).await?;
    let relative_path = format!("portraits/{}", filename);

    let state = state.read().await;
    let updated = CharacterRepo::set_portrait(&state.db, &character_id, Some(&relative_path), "approved").await?;
    info!("Uploaded portrait for character: {}", character_id);
    Ok(updated)
}

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;
use tracing::info;

use crate::auth::access::{ensure_character, stamp_owner};
use crate::commands::actor::acting;
use crate::db::characters::CharacterRepo;
use crate::error::{validate_required_string, MythicError};
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
    let (db, actor) = acting(&state).await?;
    let character = CharacterRepo::create(&db, &name, data.0).await?;
    let id = crate::db::value_bridge::record_id_to_string(&character.id);
    stamp_owner(&db, &actor, "characters", &id).await?;
    info!("Created character: {} ({})", name, id);
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
    let (db, actor) = acting(&state).await?;
    ensure_character(&db, &actor, &id).await?;
    CharacterRepo::get(&db, &id).await
}

/// Lists all characters, ordered by most recently updated.
#[tauri::command]
#[specta::specta]
pub async fn list_characters(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<Character>, MythicError> {
    let (db, actor) = acting(&state).await?;
    CharacterRepo::list(&db, actor.owner_filter()).await
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
    let (db, actor) = acting(&state).await?;
    ensure_character(&db, &actor, &id).await?;
    let character = CharacterRepo::update(
        &db,
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
    let (db, actor) = acting(&state).await?;
    ensure_character(&db, &actor, &id).await?;
    CharacterRepo::delete(&db, &id).await?;
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
    let (db, actor) = acting(&state).await?;
    ensure_character(&db, &actor, &id).await?;
    let character = CharacterRepo::trash(&db, &id).await?;
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
    let (db, actor) = acting(&state).await?;
    ensure_character(&db, &actor, &id).await?;
    let character = CharacterRepo::restore(&db, &id).await?;
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
    let (db, actor) = acting(&state).await?;
    ensure_character(&db, &actor, &character_id).await?;
    let source = std::path::PathBuf::from(&file_path);
    if !source.exists() {
        return Err(MythicError::NotFound(format!(
            "File not found: {}",
            file_path
        )));
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

    let updated =
        CharacterRepo::set_portrait(&db, &character_id, Some(&relative_path), "approved").await?;
    info!("Uploaded portrait for character: {}", character_id);
    Ok(updated)
}

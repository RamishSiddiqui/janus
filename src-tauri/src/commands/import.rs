//! Character card import from PNG files.
//!
//! Supports the TavernAI/SillyTavern Character Card V2 format,
//! where character data is embedded as base64-encoded JSON in
//! the PNG's tEXt metadata chunk under the "chara" key.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;
use tracing::info;

use crate::db::characters::CharacterRepo;
use crate::error::MythicError;
use crate::models::character::{Character, CharacterCardV2};
use crate::AppState;

/// Internal helper — reads PNG tEXt chunks and extracts the "chara" key.
///
/// PNG tEXt chunks store key-value metadata. TavernAI cards use:
///   key:   "chara"
///   value: base64-encoded JSON of CharacterCardV2
fn extract_chara_from_png(png_bytes: &[u8]) -> Result<String, MythicError> {
    use std::io::Cursor;

    let cursor = Cursor::new(png_bytes);
    let decoder = png::Decoder::new(cursor);
    let reader = decoder
        .read_info()
        .map_err(|e| MythicError::Validation(format!("Invalid PNG file: {}", e)))?;

    let info = reader.info();

    // Search through all text chunks for the "chara" key
    for text_chunk in &info.uncompressed_latin1_text {
        if text_chunk.keyword == "chara" {
            let decoded = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &text_chunk.text,
            )
            .map_err(|e| {
                MythicError::Validation(format!("Failed to decode base64 chara data: {}", e))
            })?;

            let json_str = String::from_utf8(decoded)
                .map_err(|e| MythicError::Validation(format!("Invalid UTF-8 in chara data: {}", e)))?;

            return Ok(json_str);
        }
    }

    // Also check compressed text chunks (iTXt/zTXt)
    for text_chunk in &info.compressed_latin1_text {
        if text_chunk.keyword == "chara" {
            let decompressed = text_chunk.get_text()
                .map_err(|e| MythicError::Validation(format!("Failed to decompress chara data: {}", e)))?;

            let decoded = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &decompressed,
            )
            .map_err(|e| {
                MythicError::Validation(format!("Failed to decode base64 chara data: {}", e))
            })?;

            let json_str = String::from_utf8(decoded)
                .map_err(|e| MythicError::Validation(format!("Invalid UTF-8 in chara data: {}", e)))?;

            return Ok(json_str);
        }
    }

    Err(MythicError::Validation(
        "No 'chara' metadata found in PNG. This may not be a character card.".into(),
    ))
}

/// Imports a character from a PNG file containing embedded Character Card V2 data.
///
/// Flow:
/// 1. User selects a PNG via the file dialog
/// 2. Read the PNG bytes
/// 3. Extract the "chara" tEXt chunk and decode base64
/// 4. Parse as CharacterCardV2 JSON
/// 5. Save the character to the database
/// 6. Copy the PNG as the character's avatar
#[tauri::command]
#[specta::specta]
pub async fn import_character_card(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    file_path: String,
) -> Result<Character, MythicError> {
    info!("Importing character card from: {}", file_path);

    // Read the PNG file
    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Err(MythicError::NotFound(format!("File not found: {}", file_path)));
    }

    let png_bytes = tokio::fs::read(&path).await?;

    // Extract and parse the character data
    let json_str = extract_chara_from_png(&png_bytes)?;

    let card: CharacterCardV2 = serde_json::from_str(&json_str)
        .map_err(|e| MythicError::Validation(format!("Invalid character card JSON: {}", e)))?;

    let character_name = card.data.name.clone();

    info!("Parsed character card: {} (spec: {})", character_name, card.spec);

    // Convert card data to JSON value for storage
    let data_value = serde_json::to_value(&card.data)?;

    // Create the character via repo (spec is hardcoded in create, avatar set via update)
    let state_guard = state.read().await;
    let character = CharacterRepo::create(
        &state_guard.db,
        &character_name,
        data_value,
    ).await?;

    // Extract the character ID for the avatar filename
    let character_id = character.id.id.to_raw();

    // Save the avatar PNG to the app data directory
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;

    let avatars_dir = app_data_dir.join("avatars");
    tokio::fs::create_dir_all(&avatars_dir).await?;

    let avatar_filename = format!("{}.png", character_id);
    let avatar_path = avatars_dir.join(&avatar_filename);
    tokio::fs::write(&avatar_path, &png_bytes).await?;

    let relative_avatar = format!("avatars/{}", avatar_filename);

    // Update the character with the avatar path
    let updated = CharacterRepo::update(
        &state_guard.db,
        &character_id,
        None,  // name unchanged
        None,  // data unchanged
        Some(&relative_avatar),
    ).await?;

    info!("Imported character: {} ({}) with avatar at {}", character_name, character_id, relative_avatar);

    Ok(updated)
}

/// Serves an avatar image from the app data directory.
/// Returns the absolute path to the avatar file for the frontend to load.
#[tauri::command]
#[specta::specta]
pub async fn get_avatar_path(
    app: AppHandle,
    avatar_relative: String,
) -> Result<String, MythicError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;

    let full_path = crate::error::resolve_within(&app_data_dir, &avatar_relative)?;

    if !full_path.exists() {
        return Err(MythicError::NotFound(format!(
            "Avatar not found: {}",
            avatar_relative
        )));
    }

    Ok(full_path.to_string_lossy().to_string())
}

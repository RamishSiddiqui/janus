use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use crate::commands::npc::gather_character_dialogue;
use crate::context::npc::profile_generator;
use crate::db::characters::CharacterRepo;
use crate::db::lorebook::LorebookRepo;
use crate::db::memories::MemoryRepo;
use crate::providers::resolve::{create_rig_provider, get_default_llm_provider, resolve_model_id};
use crate::error::MythicError;
use crate::models::character::CharacterData;
use crate::models::lorebook::LorebookEntry;
use crate::AppState;

/// Lists all lorebook entries for a character (plus global entries).
#[tauri::command]
#[specta::specta]
pub async fn list_lorebook_entries(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
) -> Result<Vec<LorebookEntry>, MythicError> {
    let state_guard = state.read().await;
    LorebookRepo::list(&state_guard.db, &character_id).await
}

/// Creates a new lorebook entry.
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
pub async fn delete_lorebook_entry(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state_guard = state.read().await;
    LorebookRepo::delete(&state_guard.db, &id).await
}

/// Updates a lorebook entry's editable fields — previously entries could
/// only be toggled on/off or deleted, never actually edited after creation.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn update_lorebook_entry(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    name: String,
    keys: Vec<String>,
    content: String,
    always_active: bool,
    priority: i32,
    insertion_order: i32,
) -> Result<LorebookEntry, MythicError> {
    let state_guard = state.read().await;
    LorebookRepo::update(
        &state_guard.db, &id, &name, keys, &content, always_active, priority, insertion_order,
    ).await
}

/// Imports a character's embedded Character Card V2 `character_book` (if
/// any) as real, persisted lorebook entries. PNG import already does this
/// automatically now — this manual trigger exists for characters that were
/// imported before that existed, whose card-embedded lorebook the UI could
/// only ever *display*, never actually use during chat generation.
///
/// Returns an empty list (not an error) if the character has no embedded
/// character_book, or it has zero entries.
#[tauri::command]
#[specta::specta]
pub async fn import_character_book_entries(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
) -> Result<Vec<LorebookEntry>, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    drop(state_guard);

    let character = CharacterRepo::get(&db, &character_id).await?;
    let data: CharacterData = serde_json::from_value(character.data)
        .map_err(|e| MythicError::Validation(format!("Failed to parse character data: {}", e)))?;

    match data.character_book {
        Some(book) if !book.entries.is_empty() => {
            LorebookRepo::import_from_character_book(&db, &character_id, &book).await
        }
        _ => Ok(Vec::new()),
    }
}

/// Generates new lorebook entries for a character via the LLM — the
/// "Generate from Story" action. Grounds the generation in the character's
/// profile, canon/conversation facts, and recent dialogue (same context
/// `refresh_character_profile` uses), and tells the model which entries
/// already exist so it doesn't produce near-duplicates on a re-run.
/// Newly generated entries are persisted immediately, same as a manual add.
#[tauri::command]
#[specta::specta]
pub async fn generate_character_lorebook(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
    conversation_id: String,
) -> Result<Vec<LorebookEntry>, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    drop(state_guard);

    let provider_config = get_default_llm_provider(&db).await?;
    let provider = create_rig_provider(&provider_config)?;
    let model_id = resolve_model_id(None, &provider_config, &db).await?;

    let character = CharacterRepo::get(&db, &character_id).await?;
    let description = character.data.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let personality = character.data.get("personality").and_then(|v| v.as_str()).unwrap_or("");
    let scenario = character.data.get("scenario").and_then(|v| v.as_str()).unwrap_or("");

    let known_facts: Vec<String> = MemoryRepo::list_for_character_in_conv(&db, &character_id, &conversation_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.content)
        .collect();

    let recent_dialogue = gather_character_dialogue(&db, &conversation_id, &character.name).await;

    let existing = LorebookRepo::list(&db, &character_id).await.unwrap_or_default();
    let existing_names: Vec<String> = existing.iter().map(|e| e.name.clone().unwrap_or_default()).filter(|n| !n.is_empty()).collect();

    let generated = profile_generator::generate_lorebook_entries(
        &provider, &model_id, &character.name, description, personality, scenario,
        &known_facts, &recent_dialogue, &existing_names,
    ).await?;

    let mut created = Vec::with_capacity(generated.len());
    for entry in generated {
        if entry.name.trim().is_empty() || entry.content.trim().is_empty() {
            continue;
        }
        let new_entry = LorebookRepo::create(
            &db, Some(&character_id), &entry.name, entry.keys.clone(), &entry.content, entry.always_active,
        ).await?;
        let updated = LorebookRepo::update(
            &db, &new_entry.id.id.to_raw(), &entry.name, entry.keys, &entry.content,
            entry.always_active, entry.priority, 100,
        ).await?;
        created.push(updated);
    }

    info!("Generated {} lorebook entries for character '{}' ({}) from conversation {}", created.len(), character.name, character_id, conversation_id);
    Ok(created)
}

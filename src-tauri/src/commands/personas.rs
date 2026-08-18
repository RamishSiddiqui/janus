//! CRUD + AI portrait generation for user Personas — the player's own
//! stand-in profile, selectable per-conversation. Mirrors the Character
//! Gallery's `commands::characters` CRUD exactly, and adapts
//! `commands::npc::generate_npc_portrait` for portrait generation (no
//! `identity_concealed` branch, no pending-review gate — personas are
//! always user-controlled, so a generated portrait auto-approves).

use std::sync::Arc;

use tauri::{Manager, State};
use tokio::sync::RwLock;
use tracing::info;

use crate::commands::scenes::generate_via_generic_provider;
use crate::db::image_presets::ImagePresetRepo;
use crate::db::personas::PersonaRepo;
use crate::db::providers::ProviderRepo;
use crate::error::{truncate_at_char_boundary, validate_required_string, MythicError};
use crate::models::persona::Persona;
use crate::models::provider::{ImageGenParams, ProviderAdapter};
use crate::models::DynamicJson;
use crate::providers::ai_horde::generate_via_ai_horde;
use crate::AppState;

/// Creates a new persona from a Character Card V2-shaped payload.
#[tauri::command]
#[specta::specta]
pub async fn create_persona(
    state: State<'_, Arc<RwLock<AppState>>>,
    name: String,
    data: DynamicJson,
) -> Result<Persona, MythicError> {
    validate_required_string("Persona name", &name, 200)?;
    let state = state.read().await;
    let persona = PersonaRepo::create(&state.db, &name, data.0).await?;
    info!("Created persona: {} ({})", name, persona.id);
    Ok(persona)
}

/// Retrieves a single persona by ID.
#[tauri::command]
#[specta::specta]
pub async fn get_persona(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Persona, MythicError> {
    if id.is_empty() {
        return Err(MythicError::Validation("Persona ID is required".into()));
    }
    let state = state.read().await;
    PersonaRepo::get(&state.db, &id).await
}

/// Lists all personas, ordered by most recently updated.
#[tauri::command]
#[specta::specta]
pub async fn list_personas(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<Persona>, MythicError> {
    let state = state.read().await;
    PersonaRepo::list(&state.db).await
}

/// Updates an existing persona's data.
#[tauri::command]
#[specta::specta]
pub async fn update_persona(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    name: Option<String>,
    data: Option<DynamicJson>,
    avatar_path: Option<String>,
) -> Result<Persona, MythicError> {
    if id.is_empty() {
        return Err(MythicError::Validation("Persona ID is required".into()));
    }
    if let Some(ref name) = name {
        validate_required_string("Persona name", name, 200)?;
    }
    let state = state.read().await;
    let persona = PersonaRepo::update(
        &state.db,
        &id,
        name.as_deref(),
        data.map(|d| d.0),
        avatar_path.as_deref(),
    )
    .await?;
    info!("Updated persona: {}", id);
    Ok(persona)
}

/// Permanently deletes a persona by ID. Cascade (clearing
/// `conversations.persona_id`) is handled by the `cascade_persona_delete`
/// SurrealDB event. Only the Trash view should call this — normal deletion
/// should call `trash_persona` instead.
#[tauri::command]
#[specta::specta]
pub async fn delete_persona(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    if id.is_empty() {
        return Err(MythicError::Validation("Persona ID is required".into()));
    }
    let state = state.read().await;
    PersonaRepo::delete(&state.db, &id).await?;
    info!("Deleted persona: {}", id);
    Ok(())
}

/// Moves a persona to Trash (soft delete) — reversible via `restore_persona`.
#[tauri::command]
#[specta::specta]
pub async fn trash_persona(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Persona, MythicError> {
    if id.is_empty() {
        return Err(MythicError::Validation("Persona ID is required".into()));
    }
    let state = state.read().await;
    let persona = PersonaRepo::trash(&state.db, &id).await?;
    info!("Trashed persona: {}", id);
    Ok(persona)
}

/// Restores a trashed persona.
#[tauri::command]
#[specta::specta]
pub async fn restore_persona(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Persona, MythicError> {
    if id.is_empty() {
        return Err(MythicError::Validation("Persona ID is required".into()));
    }
    let state = state.read().await;
    let persona = PersonaRepo::restore(&state.db, &id).await?;
    info!("Restored persona: {}", id);
    Ok(persona)
}

/// Generates a portrait for a persona via the configured image provider,
/// framed from its description. Silently skips — returns the persona
/// unchanged, no error — if no image provider is configured, same as NPC
/// portrait generation.
///
/// `conversation_id` is optional (personas aren't conversation-scoped —
/// one persona may be used across many conversations): when given, the
/// AI-Horde image preset is resolved for that conversation; otherwise the
/// global default preset is used.
#[tauri::command]
#[specta::specta]
pub async fn generate_persona_portrait(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    persona_id: String,
    conversation_id: Option<String>,
) -> Result<Persona, MythicError> {
    let state_guard = state.read().await;
    let persona = PersonaRepo::get(&state_guard.db, &persona_id).await?;

    let provider = ProviderRepo::get_default(&state_guard.db, "image").await?;
    let Some(provider) = provider else {
        return Ok(persona);
    };

    let description = persona
        .data
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let truncated_desc = truncate_at_char_boundary(description, 400);
    let prompt = format!(
        "portrait of {}, {}, character portrait, detailed face, upper body",
        persona.name, truncated_desc,
    );
    let params = ImageGenParams {
        prompt,
        width: 512,
        height: 512,
        ..Default::default()
    };

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    let personas_dir = app_data_dir.join("personas");
    tokio::fs::create_dir_all(&personas_dir).await?;
    let filename = format!("{}.png", persona_id);
    let file_path = personas_dir.join(&filename);
    let relative_path = format!("personas/{}", filename);

    let image_bytes = if provider.adapter == ProviderAdapter::AiHorde {
        let preset = match &conversation_id {
            Some(conv_id) => {
                ImagePresetRepo::resolve_for_conversation(&state_guard.db, conv_id).await?
            }
            None => ImagePresetRepo::get_default(&state_guard.db).await?,
        };
        // Namespaced key so this never collides with scene generation or NPC
        // portrait generation, which use their own key prefixes on the same
        // `active_scene_generations` map.
        let portrait_key = format!("persona-portrait-{}", persona_id);
        let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let mut active = state_guard.active_scene_generations.lock().await;
            if active.contains_key(&portrait_key) {
                return Err(MythicError::Provider(
                    "A portrait generation is already in progress for this persona".to_string(),
                ));
            }
            active.insert(portrait_key.clone(), cancel_flag.clone());
        }

        let result = generate_via_ai_horde(
            &app,
            &portrait_key,
            &state_guard.http_client,
            &state_guard.db,
            &provider,
            &params,
            preset.as_ref(),
            None,
            None,
            None,
            &cancel_flag,
        )
        .await;

        state_guard
            .active_scene_generations
            .lock()
            .await
            .remove(&portrait_key);
        result?.0
    } else {
        generate_via_generic_provider(&state_guard.http_client, &provider, &params)
            .await?
            .0
    };

    tokio::fs::write(&file_path, &image_bytes).await?;

    let updated =
        PersonaRepo::set_avatar(&state_guard.db, &persona_id, Some(&relative_path)).await?;
    info!("Generated persona portrait for {}", updated.name);
    Ok(updated)
}

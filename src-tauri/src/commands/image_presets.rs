//! CRUD commands for image-generation presets — reusable sampler/cfg/steps/
//! karras/style/negative-prompt bundles, selectable per-conversation or
//! applied globally via a default.

use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use crate::db::image_presets::ImagePresetRepo;
use crate::error::MythicError;
use crate::models::image_preset::ImagePreset;
use crate::AppState;

/// Bundled fields for `create_image_preset` — kept as a struct rather than
/// individual params since AI Horde quality knobs (clip_skip, post_processing,
/// hires_fix) pushed this past tauri-specta's ~10-argument function limit.
#[derive(Debug, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateImagePresetFields {
    pub model: Option<String>,
    pub sampler_name: String,
    pub cfg_scale: f64,
    pub steps: u32,
    pub karras: bool,
    pub style: Option<String>,
    pub negative_prompt: Option<String>,
    pub is_default: bool,
    pub clip_skip: Option<u32>,
    pub post_processing: Vec<String>,
    pub hires_fix: bool,
    pub hires_fix_denoising_strength: Option<f64>,
}

/// Bundled fields for `update_image_preset` — same "None (unsent) means
/// leave as-is" convention as `ImagePresetRepo::update`; for `clip_skip`,
/// `0` clears it back to "no override" (valid range is 1-12).
#[derive(Debug, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateImagePresetFields {
    pub name: Option<String>,
    pub model: Option<String>,
    pub sampler_name: Option<String>,
    pub cfg_scale: Option<f64>,
    pub steps: Option<u32>,
    pub karras: Option<bool>,
    pub style: Option<String>,
    pub negative_prompt: Option<String>,
    pub clip_skip: Option<u32>,
    pub post_processing: Option<Vec<String>>,
    pub hires_fix: Option<bool>,
    pub hires_fix_denoising_strength: Option<f64>,
}

#[tauri::command]
#[specta::specta]
pub async fn list_image_presets(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<ImagePreset>, MythicError> {
    let state = state.read().await;
    ImagePresetRepo::list(&state.db).await
}

#[tauri::command]
#[specta::specta]
pub async fn create_image_preset(
    state: State<'_, Arc<RwLock<AppState>>>,
    name: String,
    fields: CreateImagePresetFields,
) -> Result<ImagePreset, MythicError> {
    let state = state.read().await;
    let preset = ImagePresetRepo::create(
        &state.db,
        &name,
        fields.model.as_deref(),
        &fields.sampler_name,
        fields.cfg_scale,
        fields.steps,
        fields.karras,
        fields.style.as_deref(),
        fields.negative_prompt.as_deref(),
        fields.is_default,
        fields.clip_skip,
        &fields.post_processing,
        fields.hires_fix,
        fields.hires_fix_denoising_strength,
    )
    .await?;
    info!("Created image preset: {}", preset.name);
    Ok(preset)
}

#[tauri::command]
#[specta::specta]
pub async fn update_image_preset(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    fields: UpdateImagePresetFields,
) -> Result<ImagePreset, MythicError> {
    let state = state.read().await;
    ImagePresetRepo::update(
        &state.db,
        &id,
        fields.name.as_deref(),
        fields.model.as_deref(),
        fields.sampler_name.as_deref(),
        fields.cfg_scale,
        fields.steps,
        fields.karras,
        fields.style.as_deref(),
        fields.negative_prompt.as_deref(),
        fields.clip_skip,
        fields.post_processing.as_deref(),
        fields.hires_fix,
        fields.hires_fix_denoising_strength,
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_image_preset(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;
    ImagePresetRepo::delete(&state.db, &id).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_default_image_preset(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;
    ImagePresetRepo::set_default(&state.db, &id).await
}

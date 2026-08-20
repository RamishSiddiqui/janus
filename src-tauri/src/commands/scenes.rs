//! Scene generation and management commands.
//!
//! Provides CRUD operations for scenes (generated images/videos) and
//! wires into the image generation provider pipeline. Generated media
//! files are persisted to the app data directory under `scenes/`.

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use crate::db::characters::CharacterRepo;
use crate::db::conversation_characters::ConversationCharacterRepo;
use crate::db::conversations::ConversationRepo;
use crate::db::image_presets::ImagePresetRepo;
use crate::db::providers::ProviderRepo;
use crate::db::scenes::SceneRepo;
use crate::error::MythicError;
use crate::models::provider::{
    CharacterImageRef, ImageGenParams, ProviderAdapter, ProviderConfig, VideoGenParams,
};
use crate::models::scene::Scene;
use crate::providers::ai_horde::generate_via_ai_horde;
use crate::providers::comfyui::generate_via_comfyui;
use crate::providers::wangp;
use crate::AppState;

/// Optional knobs for `generate_scene`, bundled into one struct because
/// tauri-specta's command macro caps out around 10 top-level parameters.
#[derive(Debug, Default, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerateSceneOptions {
    pub negative_prompt: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Takes precedence over both the resolved preset's model and the
    /// provider's own default. Lets the user pick a different model for one
    /// generation without editing the preset.
    pub model_override: Option<String>,
    /// Path (relative to app_data_dir, e.g. a character's avatar_path) to
    /// use as an img2img reference — anchors the generation to that image
    /// instead of generating from the text prompt alone.
    pub reference_image_path: Option<String>,
    /// img2img strength when reference_image_path is set: lower keeps the
    /// reference closer, higher lets the new scene diverge more. Defaults
    /// to 0.6 (a moderate anchor) if unset.
    pub denoising_strength: Option<f64>,
    /// Mirrors the user's "Allow Mature Content" setting — sent as AI
    /// Horde's `nsfw` request flag so an ordinary (non-explicit) character
    /// description doesn't get false-positive censored by an overzealous
    /// worker-side classifier. Defaults to false (strict) if unset.
    pub allow_nsfw: Option<bool>,
    /// Cast portraits to feed into a ComfyUI workflow's `{{CHARACTER_IMAGE_n}}`
    /// tokens (see `providers::comfyui`) — ignored by every other adapter,
    /// which has no multi-image mechanism. Unlike `reference_image_path`
    /// (AI Horde's single img2img anchor), this supports any number of
    /// characters, limited only by how many `{{CHARACTER_IMAGE_n}}` tokens
    /// the user's own workflow references.
    #[serde(default)]
    pub character_images: Option<Vec<CharacterImageRef>>,
}

/// Optional knobs for `generate_video_scene` — same shape/intent as
/// `GenerateSceneOptions`, minus the image-only fields (`reference_image_path`/
/// `denoising_strength` are AI Horde's img2img mechanism, which has no video
/// equivalent here) and plus video-specific ones (`duration_seconds`, `fps`).
#[derive(Debug, Default, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerateVideoOptions {
    pub negative_prompt: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_seconds: Option<f32>,
    pub fps: Option<u32>,
    pub model_override: Option<String>,
    pub allow_nsfw: Option<bool>,
    /// Cast portraits for multi-character reference — see `GenerateSceneOptions::character_images`.
    /// WanGP is currently the only adapter that acts on this for video.
    #[serde(default)]
    pub character_images: Option<Vec<CharacterImageRef>>,
}

/// One cast member available as a portrait-reference source for scene
/// generation — the primary character plus everyone in this conversation's
/// `conversation_characters` roster (any role, including still-"Unconfirmed"
/// transients; the frontend decides what to show/select).
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SceneCastMember {
    pub character_id: String,
    pub name: String,
    pub avatar_path: Option<String>,
    /// "primary" for the conversation's own character, otherwise whatever
    /// `conversation_characters.role` holds ("secondary" | "npc" | "transient").
    pub role: String,
}

/// Lists everyone available as a portrait-reference source for this
/// conversation's scene generation — the single source of truth the
/// frontend's cast-portrait picker uses, instead of re-deriving it from
/// several different props/stores that don't all carry raw avatar paths.
#[tauri::command]
#[specta::specta]
pub async fn list_scene_cast_members(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<Vec<SceneCastMember>, MythicError> {
    let state = state.read().await;
    let mut members = Vec::new();

    if let Ok(conv) = ConversationRepo::get(&state.db, &conversation_id).await {
        if let Some(char_id) = conv.character_id {
            if let Ok(primary) = CharacterRepo::get(
                &state.db,
                &crate::db::value_bridge::record_id_to_string(&char_id),
            )
            .await
            {
                members.push(SceneCastMember {
                    character_id: crate::db::value_bridge::record_id_to_string(&primary.id),
                    name: primary.name,
                    avatar_path: primary.avatar_path,
                    role: "primary".to_string(),
                });
            }
        }
    }

    let cast = ConversationCharacterRepo::list(&state.db, &conversation_id)
        .await
        .unwrap_or_default();
    for member in cast {
        let char_id = crate::db::value_bridge::record_id_to_string(&member.character_id);
        if let Ok(character) = CharacterRepo::get(&state.db, &char_id).await {
            members.push(SceneCastMember {
                character_id: char_id,
                name: character.name,
                avatar_path: character.avatar_path,
                role: member.role,
            });
        }
    }

    Ok(members)
}

/// Generates a scene image from a prompt and saves it to the database + filesystem.
///
/// This command:
/// 1. Looks up the configured image provider
/// 2. Calls generate_image with the provided prompt
/// 3. Saves the resulting PNG to `scenes/{id}.png`
/// 4. Creates a database record linking the scene to the conversation
#[tauri::command]
#[specta::specta]
pub async fn generate_scene(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    message_id: Option<String>,
    prompt: String,
    options: GenerateSceneOptions,
) -> Result<Scene, MythicError> {
    info!(
        "Generating scene for conversation {}: {}",
        conversation_id, prompt
    );

    let GenerateSceneOptions {
        negative_prompt,
        width,
        height,
        model_override,
        reference_image_path,
        denoising_strength,
        allow_nsfw,
        character_images,
    } = options;
    let character_images = character_images.unwrap_or_default();

    let scene_id = Uuid::new_v4().to_string();

    // Set up the image generation parameters
    let params = ImageGenParams {
        prompt: prompt.clone(),
        negative_prompt: negative_prompt.unwrap_or_default(),
        width: width.unwrap_or(1024),
        height: height.unwrap_or(1024),
        allow_nsfw: allow_nsfw.unwrap_or(false),
        ..Default::default()
    };

    // Resolve the scenes directory
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;

    let scenes_dir = app_data_dir.join("scenes");
    tokio::fs::create_dir_all(&scenes_dir).await?;

    let filename = format!("{}.png", scene_id);
    let file_path = scenes_dir.join(&filename);
    let relative_path = format!("scenes/{}", filename);

    // If an img2img reference was requested, read + re-encode it as PNG now
    // (base64, ready to hand to the provider). Best-effort: a missing or
    // unreadable reference falls back to a normal text-to-image generation
    // rather than failing the whole request.
    let reference_image_b64: Option<String> = match &reference_image_path {
        Some(rel_path) => match crate::error::resolve_within(&app_data_dir, rel_path)
            .map_err(|e| e.to_string())
            .and_then(|abs| std::fs::read(&abs).map_err(|e| e.to_string()))
            .and_then(|bytes| image::load_from_memory(&bytes).map_err(|e| e.to_string()))
        {
            Ok(img) => {
                let mut buf = std::io::Cursor::new(Vec::new());
                match img.write_to(&mut buf, image::ImageFormat::Png) {
                    Ok(()) => Some(base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        buf.into_inner(),
                    )),
                    Err(e) => {
                        warn!("Failed to re-encode reference image: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                warn!("Failed to load reference image {}: {}", rel_path, e);
                None
            }
        },
        None => None,
    };

    // Look up the default image provider
    let state_guard = state.read().await;

    // Try to find a configured image provider and generate the image
    let provider = ProviderRepo::get_default(&state_guard.db, "image").await?;
    // This conversation's chosen preset, falling back to the global default —
    // `None` means "no presets configured at all", so the AI Horde path
    // falls further back to the provider's own raw config fields.
    let preset =
        ImagePresetRepo::resolve_for_conversation(&state_guard.db, &conversation_id).await?;

    let (caption, metadata) = match &provider {
        Some(p) if p.adapter == ProviderAdapter::AiHorde => {
            let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            {
                let mut active = state_guard.active_scene_generations.lock().await;
                if active.contains_key(&conversation_id) {
                    return Err(MythicError::Provider(
                        "A generation is already in progress for this conversation".to_string(),
                    ));
                }
                active.insert(conversation_id.clone(), cancel_flag.clone());
            }

            let result = generate_via_ai_horde(
                &app,
                &conversation_id,
                &state_guard.http_client,
                &state_guard.db,
                p,
                &params,
                preset.as_ref(),
                model_override.as_deref(),
                reference_image_b64.as_deref(),
                denoising_strength,
                &cancel_flag,
            )
            .await;

            state_guard
                .active_scene_generations
                .lock()
                .await
                .remove(&conversation_id);

            let (image_bytes, meta) = result?;
            tokio::fs::write(&file_path, &image_bytes).await?;
            let caption = format!("{} — generated via {}", prompt, p.name);
            (caption, meta)
        }
        Some(p) if p.adapter == ProviderAdapter::ComfyUi => {
            // Same single-flight guard + cancel_flag plumbing as the AiHorde
            // arm above, so Stop/Cancel Generation works identically here.
            let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            {
                let mut active = state_guard.active_scene_generations.lock().await;
                if active.contains_key(&conversation_id) {
                    return Err(MythicError::Provider(
                        "A generation is already in progress for this conversation".to_string(),
                    ));
                }
                active.insert(conversation_id.clone(), cancel_flag.clone());
            }

            let result = generate_via_comfyui(
                &state_guard.http_client,
                p,
                &params,
                &character_images,
                &app_data_dir,
                &cancel_flag,
            )
            .await;

            state_guard
                .active_scene_generations
                .lock()
                .await
                .remove(&conversation_id);

            let (image_bytes, meta) = result?;
            tokio::fs::write(&file_path, &image_bytes).await?;
            let caption = format!("{} — generated via {}", prompt, p.name);
            (caption, meta)
        }
        Some(p) if p.adapter == ProviderAdapter::WanGp => {
            // Same single-flight guard + cancel_flag plumbing as ComfyUI/AiHorde above.
            let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            {
                let mut active = state_guard.active_scene_generations.lock().await;
                if active.contains_key(&conversation_id) {
                    return Err(MythicError::Provider(
                        "A generation is already in progress for this conversation".to_string(),
                    ));
                }
                active.insert(conversation_id.clone(), cancel_flag.clone());
            }

            let result = wangp::generate_image_via_wangp(
                &app,
                &conversation_id,
                p,
                &params,
                model_override.as_deref(),
                &character_images,
                &app_data_dir,
                &cancel_flag,
            )
            .await;

            state_guard
                .active_scene_generations
                .lock()
                .await
                .remove(&conversation_id);

            let (image_bytes, meta) = result?;
            tokio::fs::write(&file_path, &image_bytes).await?;
            let caption = format!("{} — generated via {}", prompt, p.name);
            (caption, meta)
        }
        Some(provider) => {
            let (image_bytes, metadata) =
                generate_via_generic_provider(&state_guard.http_client, provider, &params).await?;
            tokio::fs::write(&file_path, &image_bytes).await?;
            let caption = format!("{} — generated via {}", prompt, provider.name);
            (caption, metadata)
        }
        None => {
            // No image provider configured — create a placeholder scene
            // Generate a gradient placeholder PNG
            let placeholder = generate_placeholder_png(params.width, params.height)?;
            tokio::fs::write(&file_path, &placeholder).await?;

            let caption = format!("{} — no image provider configured", prompt);
            let metadata = serde_json::json!({
                "placeholder": true,
                "width": params.width,
                "height": params.height,
            });

            (caption, metadata)
        }
    };

    // Save to database via SceneRepo
    let scene = SceneRepo::create(
        &state_guard.db,
        &scene_id,
        &conversation_id,
        message_id.as_deref(),
        "image",
        &prompt,
        &relative_path,
        Some(&caption),
        Some(metadata),
    )
    .await?;

    info!("Scene generated: {} saved to {}", scene_id, relative_path);

    Ok(scene)
}

/// Generates a scene video from a prompt and saves it to the database +
/// filesystem — same shape as `generate_scene`, but for the video pipeline.
/// Unlike image generation, there's no generic/placeholder fallback: video
/// generation currently only exists via WanGP, so anything else (including
/// no video provider configured at all) is a clear, immediate error rather
/// than a silent placeholder (a placeholder *video* doesn't make sense).
#[tauri::command]
#[specta::specta]
pub async fn generate_video_scene(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    message_id: Option<String>,
    prompt: String,
    options: GenerateVideoOptions,
) -> Result<Scene, MythicError> {
    info!(
        "Generating video scene for conversation {}: {}",
        conversation_id, prompt
    );

    let GenerateVideoOptions {
        negative_prompt,
        width,
        height,
        duration_seconds,
        fps,
        model_override,
        allow_nsfw,
        character_images,
    } = options;
    let character_images = character_images.unwrap_or_default();

    let params = VideoGenParams {
        prompt: prompt.clone(),
        negative_prompt: negative_prompt.unwrap_or_default(),
        width: width.unwrap_or(1280),
        height: height.unwrap_or(720),
        duration_seconds: duration_seconds.unwrap_or(4.0),
        fps: fps.unwrap_or(24),
        seed: None,
        allow_nsfw: allow_nsfw.unwrap_or(false),
    };

    let scene_id = Uuid::new_v4().to_string();
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    let scenes_dir = app_data_dir.join("scenes");
    tokio::fs::create_dir_all(&scenes_dir).await?;

    let state_guard = state.read().await;
    let provider = ProviderRepo::get_default(&state_guard.db, "video").await?;

    let Some(p) = provider.filter(|p| p.adapter == ProviderAdapter::WanGp) else {
        return Err(MythicError::Validation(
            "No video provider configured. Add a WanGP provider (Video type) in Settings → Providers.".to_string(),
        ));
    };

    let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut active = state_guard.active_scene_generations.lock().await;
        if active.contains_key(&conversation_id) {
            return Err(MythicError::Provider(
                "A generation is already in progress for this conversation".to_string(),
            ));
        }
        active.insert(conversation_id.clone(), cancel_flag.clone());
    }

    let result = wangp::generate_video_via_wangp(
        &app,
        &conversation_id,
        &p,
        &params,
        model_override.as_deref(),
        &character_images,
        &app_data_dir,
        &cancel_flag,
    )
    .await;

    state_guard
        .active_scene_generations
        .lock()
        .await
        .remove(&conversation_id);

    let (video_bytes, metadata) = result?;

    let filename = format!("{}.mp4", scene_id);
    let file_path = scenes_dir.join(&filename);
    let relative_path = format!("scenes/{}", filename);
    tokio::fs::write(&file_path, &video_bytes).await?;

    let caption = format!("{} — generated via {}", prompt, p.name);
    let scene = SceneRepo::create(
        &state_guard.db,
        &scene_id,
        &conversation_id,
        message_id.as_deref(),
        "video",
        &prompt,
        &relative_path,
        Some(&caption),
        Some(metadata),
    )
    .await?;

    info!(
        "Video scene generated: {} saved to {}",
        scene_id, relative_path
    );

    Ok(scene)
}

/// Signals a running scene generation for this conversation to stop — the
/// poll loop notices within one tick (up to `AI_HORDE_POLL_INTERVAL`) and
/// issues a best-effort cancel to AI Horde before returning an error.
/// A no-op if nothing is currently generating for this conversation.
#[tauri::command]
#[specta::specta]
pub async fn cancel_scene_generation(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<(), MythicError> {
    let state_guard = state.read().await;
    if let Some(flag) = state_guard
        .active_scene_generations
        .lock()
        .await
        .get(&conversation_id)
    {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

/// Lists all scenes for a given conversation.
#[tauri::command]
#[specta::specta]
pub async fn list_scenes(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<Vec<Scene>, MythicError> {
    let state_guard = state.read().await;
    SceneRepo::list(&state_guard.db, &conversation_id).await
}

/// Deletes a scene and its media file.
#[tauri::command]
#[specta::specta]
pub async fn delete_scene(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    scene_id: String,
) -> Result<(), MythicError> {
    let state_guard = state.read().await;

    // Get the file path before deleting
    if let Some(file_path) = SceneRepo::get_file_path(&state_guard.db, &scene_id).await? {
        // Delete the file
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
        if let Ok(full_path) = crate::error::resolve_within(&app_data_dir, &file_path) {
            if full_path.exists() {
                if let Err(e) = tokio::fs::remove_file(&full_path).await {
                    tracing::warn!("Failed to delete scene file {}: {}", full_path.display(), e);
                }
            }
        }
    }

    SceneRepo::delete(&state_guard.db, &scene_id).await?;

    info!("Deleted scene: {}", scene_id);
    Ok(())
}

/// Returns the absolute file path for a scene's media file.
#[tauri::command]
#[specta::specta]
pub async fn get_scene_path(app: AppHandle, file_relative: String) -> Result<String, MythicError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;

    let full_path = crate::error::resolve_within(&app_data_dir, &file_relative)?;
    if !full_path.exists() {
        return Err(MythicError::NotFound(format!(
            "Scene file not found: {}",
            file_relative
        )));
    }

    Ok(full_path.to_string_lossy().to_string())
}

// --- Internal helpers ---

/// Generates via a generic OpenAI-images-compatible provider (SiliconFlow,
/// self-hosted, etc.). Returns raw image bytes + metadata — the caller
/// (`generate_scene`, or NPC portrait generation) is responsible for
/// writing the bytes to a file.
pub(crate) async fn generate_via_generic_provider(
    http_client: &reqwest::Client,
    provider: &ProviderConfig,
    params: &ImageGenParams,
) -> Result<(Vec<u8>, serde_json::Value), MythicError> {
    // With SurrealDB, provider.config is already serde_json::Value — no parsing needed.
    let base_url = provider.config["base_url"]
        .as_str()
        .unwrap_or("http://localhost:8188");
    let api_key = provider.config["api_key"].as_str().unwrap_or("");
    let model = provider.config["model"].as_str().unwrap_or("default");

    // Call the image generation API (OpenAI-compatible /v1/images/generations)
    let response = http_client
        .post(format!("{}/v1/images/generations", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "prompt": params.prompt,
            "negative_prompt": params.negative_prompt,
            "size": format!("{}x{}", params.width, params.height),
            "n": 1,
            "response_format": "b64_json",
        }))
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("Image generation request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "Image generation failed ({}): {}",
            status, body
        )));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| MythicError::Provider(format!("Failed to parse image response: {}", e)))?;

    // Extract the base64 image data
    let b64_data = result["data"][0]["b64_json"]
        .as_str()
        .ok_or_else(|| MythicError::Provider("No image data in response".into()))?;

    let image_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64_data)
        .map_err(|e| MythicError::Provider(format!("Failed to decode image data: {}", e)))?;

    let metadata = serde_json::json!({
        "model": model,
        "provider": provider.name,
        "width": params.width,
        "height": params.height,
        "steps": params.steps,
        "guidance_scale": params.guidance_scale,
    });

    Ok((image_bytes, metadata))
}

/// Generates a simple gradient placeholder PNG when no image provider is configured.
fn generate_placeholder_png(width: u32, height: u32) -> Result<Vec<u8>, MythicError> {
    use image::{ImageBuffer, Rgba};

    let img = ImageBuffer::from_fn(width, height, |x, y| {
        let fx = x as f32 / width as f32;
        let fy = y as f32 / height as f32;
        let r = (26.0 + fx * 100.0) as u8;
        let g = (10.0 + fy * 50.0) as u8;
        let b = (46.0 + (fx + fy) * 60.0) as u8;
        Rgba([r, g, b, 255])
    });

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| MythicError::Provider(format!("Failed to encode placeholder: {}", e)))?;

    Ok(buf.into_inner())
}

//! Scene generation and management commands.
//!
//! Provides CRUD operations for scenes (generated images/videos) and
//! wires into the image generation provider pipeline. Generated media
//! files are persisted to the app data directory under `scenes/`.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, State};
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
use crate::models::image_preset::ImagePreset;
use crate::models::provider::{CharacterImageRef, ImageGenParams, ProviderAdapter, ProviderConfig};
use crate::models::scene::Scene;
use crate::providers::comfyui::generate_via_comfyui;
use crate::AppState;

const AI_HORDE_BASE_URL: &str = "https://aihorde.net/api/v2";
/// The anonymous key sits at the back of the queue, and congestion can push
/// wait times well past 9 minutes even for jobs that are still alive and
/// will complete — a job we personally observed take longer than the old
/// 9-minute budget was confirmed `done: true` seconds after we gave up and
/// cancelled it. If a job genuinely expires server-side while still queued,
/// polling just gets a 404 (handled explicitly below) well before this
/// budget is exhausted, so a generous ceiling here costs nothing.
const AI_HORDE_MAX_WAIT: Duration = Duration::from_secs(60 * 60);
/// The check endpoint is cached for 1 second server-side; polling faster
/// than that just burns requests for no benefit.
const AI_HORDE_POLL_INTERVAL: Duration = Duration::from_secs(4);

/// Generic quality/negative baseline for Stable-Diffusion-family models —
/// only used when no AI Horde `style` is configured (a style brings its own
/// curated negative prompt via its `{np}` template, so ours would just be
/// redundant noise on top of it). This list is the widely-used community
/// baseline for keeping SD1.5-era models away from the usual artifacts
/// (mangled hands, watermarks, low-effort output) — user-editable via the
/// provider's `negative_prompt` config field.
const AI_HORDE_DEFAULT_NEGATIVE_PROMPT: &str =
    "lowres, bad anatomy, bad hands, missing fingers, extra digit, fewer digits, \
     cropped, worst quality, low quality, normal quality, jpeg artifacts, \
     signature, watermark, username, blurry, deformed, disfigured, mutated, \
     extra limbs, poorly drawn face, poorly drawn hands, out of frame, duplicate";

/// Well-regarded, general-purpose AI Horde models (Civitai/community
/// consensus) — used to auto-pick a live default when nothing pins a model.
/// Matched as substrings against lowercased model names.
const RECOMMENDED_MODEL_PATTERNS: &[&str] = &["pony", "aam", "juggernaut", "realvis", "albedo"];

/// When no preset/provider pins a model, picks the best currently-available
/// one instead of leaving AI Horde to route to *any* live worker — querying
/// the live roster directly shows it skews heavily toward niche/NSFW-trained
/// anime checkpoints, which often ignore a prompt's specific details in
/// favor of their own trained look (this is what was actually happening:
/// the prompt itself was fine, but an unpredictable model was answering it).
/// Filters to well-regarded, general-purpose names, excludes explicitly
/// NSFW-branded ones (a neutral default shouldn't opt into that), and picks
/// whichever has the most workers online right now — falling back to `None`
/// (Horde free-picks, today's behavior) if nothing on the list is currently
/// up, so this can never block or hang a generation.
async fn pick_default_model(http_client: &reqwest::Client) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct HordeModelStatus {
        name: String,
        count: i64,
    }

    let resp = http_client
        .get(format!("{}/status/models?type=image", AI_HORDE_BASE_URL))
        .send()
        .await
        .ok()?;
    let models: Vec<HordeModelStatus> = resp.json().await.ok()?;

    models
        .into_iter()
        .filter(|m| m.count > 0)
        .filter(|m| {
            let lower = m.name.to_lowercase();
            !lower.contains("nsfw")
                && !lower.contains("hentai")
                && RECOMMENDED_MODEL_PATTERNS.iter().any(|p| lower.contains(p))
        })
        .max_by_key(|m| m.count)
        .map(|m| m.name)
}

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
            if let Ok(primary) = CharacterRepo::get(&state.db, &char_id.id.to_raw()).await {
                members.push(SceneCastMember {
                    character_id: primary.id.id.to_raw(),
                    name: primary.name,
                    avatar_path: primary.avatar_path,
                    role: "primary".to_string(),
                });
            }
        }
    }

    let cast = ConversationCharacterRepo::list(&state.db, &conversation_id).await.unwrap_or_default();
    for member in cast {
        let char_id = member.character_id.id.to_raw();
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
    info!("Generating scene for conversation {}: {}", conversation_id, prompt);

    let GenerateSceneOptions {
        negative_prompt, width, height, model_override, reference_image_path, denoising_strength,
        allow_nsfw, character_images,
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
                    Ok(()) => Some(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, buf.into_inner())),
                    Err(e) => { warn!("Failed to re-encode reference image: {}", e); None }
                }
            }
            Err(e) => { warn!("Failed to load reference image {}: {}", rel_path, e); None }
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
    let preset = ImagePresetRepo::resolve_for_conversation(&state_guard.db, &conversation_id).await?;

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
                &app, &conversation_id, &state_guard.http_client, p, &params,
                preset.as_ref(), model_override.as_deref(),
                reference_image_b64.as_deref(), denoising_strength,
                &cancel_flag,
            ).await;

            state_guard.active_scene_generations.lock().await.remove(&conversation_id);

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
                &state_guard.http_client, p, &params, &character_images, &app_data_dir, &cancel_flag,
            ).await;

            state_guard.active_scene_generations.lock().await.remove(&conversation_id);

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
    if let Some(flag) = state_guard.active_scene_generations.lock().await.get(&conversation_id) {
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
pub async fn get_scene_path(
    app: AppHandle,
    file_relative: String,
) -> Result<String, MythicError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;

    let full_path = crate::error::resolve_within(&app_data_dir, &file_relative)?;
    if !full_path.exists() {
        return Err(MythicError::NotFound(format!("Scene file not found: {}", file_relative)));
    }

    Ok(full_path.to_string_lossy().to_string())
}

// --- Internal helpers ---

/// The sampler/model/style knobs for one generation, resolved from either a
/// preset or the raw provider config — the two sources have the same shape,
/// so this replaces what used to be two parallel blocks of seven parallel
/// assignments (easy to update one branch and forget the other).
struct ResolvedGenParams<'a> {
    model: Option<&'a str>,
    sampler_name: &'a str,
    cfg_scale: f64,
    steps: u64,
    karras: bool,
    /// A named/shared Horde style (see aihorde.net styles, browsable at
    /// artbot.site) bundles a curated prompt template, model, sampler and
    /// resolution tuned for a specific look — when set, it takes over all of
    /// that and we only supply the raw prompt/negative for its {p}/{np} slots.
    style: Option<&'a str>,
    negative_prompt: Option<&'a str>,
    /// CLIP layers to skip (1-12) — anime/illustration checkpoints (Pony
    /// Diffusion V6 XL, AAM XL AnimeMix) are typically trained expecting 2.
    clip_skip: Option<u32>,
    /// AI Horde post-processors — face-fixers (GFPGAN/CodeFormers) and/or
    /// upscalers (RealESRGAN variants, 4x_AnimeSharp), applied in order.
    post_processing: &'a [String],
    /// Re-processes at higher resolution after the base generation — the
    /// biggest lever for composition/anatomy fixes, at roughly double the
    /// generation time and kudos cost.
    hires_fix: bool,
    hires_fix_denoising_strength: Option<f64>,
}

impl<'a> ResolvedGenParams<'a> {
    fn from_preset(preset: &'a ImagePreset) -> Self {
        Self {
            model: preset.model.as_deref().filter(|s| !s.is_empty()),
            sampler_name: &preset.sampler_name,
            cfg_scale: preset.cfg_scale,
            steps: preset.steps as u64,
            karras: preset.karras,
            style: preset.style.as_deref().filter(|s| !s.is_empty()),
            negative_prompt: preset.negative_prompt.as_deref().filter(|s| !s.is_empty()),
            clip_skip: preset.clip_skip,
            post_processing: &preset.post_processing,
            hires_fix: preset.hires_fix,
            hires_fix_denoising_strength: preset.hires_fix_denoising_strength,
        }
    }

    fn from_provider(provider: &'a ProviderConfig) -> Self {
        Self {
            model: provider.config["model"].as_str().filter(|s| !s.is_empty()),
            sampler_name: provider.config["sampler_name"].as_str().unwrap_or("k_euler_a"),
            cfg_scale: provider.config["cfg_scale"].as_f64().unwrap_or(7.5),
            steps: provider.config["steps"].as_u64().unwrap_or(30),
            // The bare API default is `false`, but community consensus
            // favors `true` for smoother results at the same step count.
            karras: provider.config["karras"].as_bool().unwrap_or(true),
            style: provider.config["style"].as_str().filter(|s| !s.is_empty()),
            negative_prompt: provider.config["negative_prompt"].as_str().filter(|s| !s.is_empty()),
            clip_skip: provider.config["clip_skip"].as_u64().map(|v| v as u32),
            post_processing: &[],
            hires_fix: provider.config["hires_fix"].as_bool().unwrap_or(false),
            hires_fix_denoising_strength: provider.config["hires_fix_denoising_strength"].as_f64(),
        }
    }
}

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
    let base_url = provider.config["base_url"].as_str().unwrap_or("http://localhost:8188");
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

    let result: serde_json::Value = response.json().await
        .map_err(|e| MythicError::Provider(format!("Failed to parse image response: {}", e)))?;

    // Extract the base64 image data
    let b64_data = result["data"][0]["b64_json"]
        .as_str()
        .ok_or_else(|| MythicError::Provider("No image data in response".into()))?;

    let image_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        b64_data,
    )
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

/// Generates an image via AI Horde: submits an async job, polls the
/// lightweight `check` endpoint until done (respecting its 1s server-side
/// cache — no point polling faster), then fetches the result once via
/// `status` (that endpoint is rate-limited to 10/min, hence checking first
/// rather than polling it directly). Returns PNG bytes — the Horde returns
/// base64-encoded WebP, re-encoded here for consistency with the rest of
/// the scenes pipeline — plus metadata describing what was actually used.
pub(crate) async fn generate_via_ai_horde(
    app: &AppHandle,
    conversation_id: &str,
    http_client: &reqwest::Client,
    provider: &ProviderConfig,
    params: &ImageGenParams,
    preset: Option<&ImagePreset>,
    model_override: Option<&str>,
    source_image_b64: Option<&str>,
    denoising_strength: Option<f64>,
    cancel_flag: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<(Vec<u8>, serde_json::Value), MythicError> {
    // The well-known anonymous key works with zero registration (lowest
    // queue priority); a free registered account starts with 25 kudos and
    // gets real priority. This is the "excellent default" — it works out of
    // the box, and users can paste their own key in Settings for priority.
    let api_key = provider.config["api_key"].as_str().filter(|s| !s.is_empty()).unwrap_or("0000000000");

    // A resolved preset (this conversation's own choice, or the global
    // default) fully overrides the provider's raw config fields — presets
    // exist precisely so different chats can use different sampler/style
    // bundles without editing the provider itself.
    let mut resolved = match preset {
        Some(preset) => ResolvedGenParams::from_preset(preset),
        None => ResolvedGenParams::from_provider(provider),
    };
    if let Some(m) = model_override.filter(|s| !s.is_empty()) {
        resolved.model = Some(m);
    }
    let ResolvedGenParams {
        model,
        sampler_name,
        cfg_scale,
        steps,
        karras,
        style,
        negative_prompt: configured_negative,
        clip_skip,
        post_processing,
        hires_fix,
        hires_fix_denoising_strength,
    } = resolved;

    let negative_prompt: &str = if !params.negative_prompt.is_empty() {
        &params.negative_prompt
    } else if style.is_none() {
        configured_negative.unwrap_or(AI_HORDE_DEFAULT_NEGATIVE_PROMPT)
    } else {
        // The style's own template already embeds a curated negative prompt;
        // stacking our generic one on top would just dilute it.
        ""
    };

    let mut body = serde_json::json!({
        "prompt": if negative_prompt.is_empty() {
            params.prompt.clone()
        } else {
            // AI Horde's convention: negative prompt appended after "###"
            format!("{} ### {}", params.prompt, negative_prompt)
        },
        "nsfw": params.allow_nsfw,
        "r2": false,
    });

    if let Some(style) = style {
        body["style"] = serde_json::json!(style);
    } else {
        body["params"] = serde_json::json!({
            "sampler_name": sampler_name,
            "cfg_scale": cfg_scale,
            "steps": steps,
            "width": params.width,
            "height": params.height,
            "karras": karras,
            "n": 1,
        });
        if let Some(model) = model {
            body["models"] = serde_json::json!([model]);
        } else if let Some(picked) = pick_default_model(http_client).await {
            info!("[ai_horde] No model pinned — auto-selected '{}' from the live roster", picked);
            body["models"] = serde_json::json!([picked]);
        }
    }

    // Quality knobs coexist with either an explicit `params` block or a named
    // style (a style only preconfigures sampler/model/resolution — these
    // supplement it rather than conflict), so they're applied unconditionally
    // after the branch above, creating `params` if a style left it unset.
    if !post_processing.is_empty() || clip_skip.is_some() || hires_fix {
        if body.get("params").is_none() {
            body["params"] = serde_json::json!({});
        }
        if !post_processing.is_empty() {
            body["params"]["post_processing"] = serde_json::json!(post_processing);
        }
        if let Some(cs) = clip_skip {
            body["params"]["clip_skip"] = serde_json::json!(cs);
        }
        if hires_fix {
            body["params"]["hires_fix"] = serde_json::json!(true);
            body["params"]["hires_fix_denoising_strength"] =
                serde_json::json!(hires_fix_denoising_strength.unwrap_or(0.65));
        }
    }

    if let Some(img_b64) = source_image_b64 {
        body["source_image"] = serde_json::json!(img_b64);
        body["source_processing"] = serde_json::json!("img2img");
        // A style skips the `params` block above (the style owns it), so
        // there may be no object to merge denoising_strength into yet.
        // 0.6 kept too much of the avatar's exact portrait pose/crop locked
        // in, overriding the actual described scene's composition; 0.75 lets
        // the prompt drive the scene while the avatar still lends likeness.
        let ds = denoising_strength.unwrap_or(0.75);
        if body.get("params").is_none() {
            body["params"] = serde_json::json!({});
        }
        body["params"]["denoising_strength"] = serde_json::json!(ds);
    }

    let submit_resp = http_client
        .post(format!("{}/generate/async", AI_HORDE_BASE_URL))
        .header("apikey", api_key)
        .header("Client-Agent", concat!("Janus:", env!("CARGO_PKG_VERSION"), ":github.com/janus"))
        .json(&body)
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("AI Horde submit failed: {}", e)))?;

    if !submit_resp.status().is_success() {
        let status = submit_resp.status();
        let text = submit_resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!("AI Horde rejected the request ({}): {}", status, text)));
    }

    let submit_json: serde_json::Value = submit_resp.json().await
        .map_err(|e| MythicError::Provider(format!("Failed to parse AI Horde submit response: {}", e)))?;
    let job_id = submit_json["id"].as_str()
        .ok_or_else(|| MythicError::Provider("AI Horde did not return a job ID".to_string()))?
        .to_string();

    info!("[ai_horde] Submitted job {} (estimated kudos cost: {:?})", job_id, submit_json.get("kudos"));
    let _ = app.emit("ai_horde_progress", serde_json::json!({
        "conversation_id": conversation_id,
        "phase": "queued",
        "kudos": submit_json.get("kudos"),
    }));

    // Poll the lightweight `check` endpoint until done, faulted, cancelled, or timeout.
    let started = std::time::Instant::now();
    loop {
        if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
            // Best-effort cancellation so the job stops consuming a worker slot.
            let _ = http_client.delete(format!("{}/generate/status/{}", AI_HORDE_BASE_URL, job_id)).send().await;
            return Err(MythicError::Provider("Generation cancelled".to_string()));
        }

        if started.elapsed() > AI_HORDE_MAX_WAIT {
            // Best-effort cancellation so the job stops consuming a worker slot.
            let _ = http_client.delete(format!("{}/generate/status/{}", AI_HORDE_BASE_URL, job_id)).send().await;
            return Err(MythicError::Provider("AI Horde generation timed out".to_string()));
        }

        tokio::time::sleep(AI_HORDE_POLL_INTERVAL).await;

        let check_resp = http_client
            .get(format!("{}/generate/check/{}", AI_HORDE_BASE_URL, job_id))
            .send()
            .await
            .map_err(|e| MythicError::Provider(format!("AI Horde check failed: {}", e)))?;

        if !check_resp.status().is_success() {
            let status = check_resp.status();
            let body = check_resp.text().await.unwrap_or_default();
            if status.as_u16() == 404 {
                return Err(MythicError::Provider(format!(
                    "AI Horde job {} not found (it may have expired): {}",
                    job_id, body
                )));
            }
            // Transient error (rate limit, momentary 5xx) — a missing `done` key
            // here would otherwise be silently read as "still processing" for
            // the rest of the wait budget, masking the real failure. Log and
            // keep polling instead.
            warn!("[ai_horde] check for job {} returned {}: {} — retrying", job_id, status, body);
            continue;
        }

        let check: serde_json::Value = check_resp.json().await
            .map_err(|e| MythicError::Provider(format!("Failed to parse AI Horde check response: {}", e)))?;

        let _ = app.emit("ai_horde_progress", serde_json::json!({
            "conversation_id": conversation_id,
            "phase": if check["processing"].as_i64().unwrap_or(0) > 0 { "processing" } else { "waiting" },
            "queue_position": check["queue_position"],
            "wait_time": check["wait_time"],
            "is_possible": check["is_possible"].as_bool().unwrap_or(true),
        }));

        if check["faulted"].as_bool().unwrap_or(false) {
            return Err(MythicError::Provider("AI Horde generation faulted (no worker could complete it)".to_string()));
        }
        if !check["is_possible"].as_bool().unwrap_or(true) {
            warn!("[ai_horde] Job {} reported as not currently possible with the available worker pool — continuing to wait in case it recovers", job_id);
        }
        if check["done"].as_bool().unwrap_or(false) {
            break;
        }
    }

    let _ = app.emit("ai_horde_progress", serde_json::json!({
        "conversation_id": conversation_id,
        "phase": "finalizing",
    }));

    // Only `status` returns the actual image(s), and it's rate-limited to
    // 10/min — safe to call once now that `check` has confirmed completion.
    let status_resp = http_client
        .get(format!("{}/generate/status/{}", AI_HORDE_BASE_URL, job_id))
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("AI Horde status fetch failed: {}", e)))?;

    if !status_resp.status().is_success() {
        let status = status_resp.status();
        let body = status_resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "AI Horde status fetch failed ({}): {}",
            status, body
        )));
    }

    let status_json: serde_json::Value = status_resp.json().await
        .map_err(|e| MythicError::Provider(format!("Failed to parse AI Horde status response: {}", e)))?;

    let generation = status_json["generations"].get(0)
        .ok_or_else(|| MythicError::Provider("AI Horde returned no generations".to_string()))?;

    let img_b64 = generation["img"].as_str()
        .ok_or_else(|| MythicError::Provider("AI Horde generation had no image data".to_string()))?;

    let webp_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, img_b64)
        .map_err(|e| MythicError::Provider(format!("Failed to decode AI Horde image: {}", e)))?;

    // AI Horde returns WebP — re-encode to PNG for consistency with the rest
    // of the scenes pipeline (and every other image path in this app).
    let decoded = image::load_from_memory(&webp_bytes)
        .map_err(|e| MythicError::Provider(format!("Failed to decode AI Horde WebP image: {}", e)))?;
    let mut png_buf = std::io::Cursor::new(Vec::new());
    decoded.write_to(&mut png_buf, image::ImageFormat::Png)
        .map_err(|e| MythicError::Provider(format!("Failed to re-encode AI Horde image as PNG: {}", e)))?;

    // Full generation details, captured so a scene can be replicated later
    // from the gallery — everything actually sent to AI Horde for this job.
    let mut metadata = serde_json::json!({
        "provider": "AI Horde",
        "model": generation.get("model"),
        "worker_name": generation.get("worker_name"),
        "seed": generation.get("seed"),
        "negative_prompt": if negative_prompt.is_empty() { None } else { Some(negative_prompt) },
    });
    if let Some(style) = style {
        // The style controlled model/sampler/resolution — the actual model
        // used is already captured above from the generation response.
        metadata["style"] = serde_json::json!(style);
    } else {
        metadata["sampler_name"] = serde_json::json!(sampler_name);
        metadata["cfg_scale"] = serde_json::json!(cfg_scale);
        metadata["steps"] = serde_json::json!(steps);
        metadata["karras"] = serde_json::json!(karras);
        metadata["width"] = serde_json::json!(params.width);
        metadata["height"] = serde_json::json!(params.height);
    }
    if let Some(cs) = clip_skip {
        metadata["clip_skip"] = serde_json::json!(cs);
    }
    if !post_processing.is_empty() {
        metadata["post_processing"] = serde_json::json!(post_processing);
    }
    if hires_fix {
        metadata["hires_fix"] = serde_json::json!(true);
    }
    if source_image_b64.is_some() {
        metadata["img2img"] = serde_json::json!(true);
        metadata["denoising_strength"] = serde_json::json!(denoising_strength.unwrap_or(0.75));
    }

    Ok((png_buf.into_inner(), metadata))
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

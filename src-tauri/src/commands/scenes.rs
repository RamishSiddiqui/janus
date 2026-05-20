//! Scene generation and management commands.
//!
//! Provides CRUD operations for scenes (generated images/videos) and
//! wires into the image generation provider pipeline. Generated media
//! files are persisted to the app data directory under `scenes/`.

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::db::providers::ProviderRepo;
use crate::db::scenes::SceneRepo;
use crate::error::MythicError;
use crate::models::provider::ImageGenParams;
use crate::models::scene::Scene;
use crate::AppState;

/// Generates a scene image from a prompt and saves it to the database + filesystem.
///
/// This command:
/// 1. Looks up the configured image provider
/// 2. Calls generate_image with the provided prompt
/// 3. Saves the resulting PNG to `scenes/{id}.png`
/// 4. Creates a database record linking the scene to the conversation
#[tauri::command]
pub async fn generate_scene(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    message_id: Option<String>,
    prompt: String,
    negative_prompt: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<Scene, MythicError> {
    info!("Generating scene for conversation {}: {}", conversation_id, prompt);

    let scene_id = Uuid::new_v4().to_string();

    // Set up the image generation parameters
    let params = ImageGenParams {
        prompt: prompt.clone(),
        negative_prompt: negative_prompt.unwrap_or_default(),
        width: width.unwrap_or(1024),
        height: height.unwrap_or(1024),
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

    // Look up the default image provider
    let state_guard = state.read().await;

    // Try to find a configured image provider and generate the image
    let provider = ProviderRepo::get_default(&state_guard.db, "image").await?;

    let (caption, metadata) = if let Some(provider) = provider {
        // We have a configured image provider — use it.
        // With SurrealDB, provider.config is already serde_json::Value — no parsing needed.
        let base_url = provider.config["base_url"].as_str().unwrap_or("http://localhost:8188");
        let api_key = provider.config["api_key"].as_str().unwrap_or("");
        let model = provider.config["model"].as_str().unwrap_or("default");

        // Call the image generation API (OpenAI-compatible /v1/images/generations)
        let response = state_guard.http_client
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

        tokio::fs::write(&file_path, &image_bytes).await?;

        let caption = format!("{} — generated via {}", prompt, provider.name);
        let metadata = serde_json::json!({
            "model": model,
            "provider": provider.name,
            "width": params.width,
            "height": params.height,
            "steps": params.steps,
            "guidance_scale": params.guidance_scale,
        });

        (caption, metadata)
    } else {
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

/// Lists all scenes for a given conversation.
#[tauri::command]
pub async fn list_scenes(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<Vec<Scene>, MythicError> {
    let state_guard = state.read().await;
    SceneRepo::list(&state_guard.db, &conversation_id).await
}

/// Deletes a scene and its media file.
#[tauri::command]
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
        let full_path = app_data_dir.join(&file_path);
        if full_path.exists() {
            let _ = tokio::fs::remove_file(full_path).await;
        }
    }

    SceneRepo::delete(&state_guard.db, &scene_id).await?;

    info!("Deleted scene: {}", scene_id);
    Ok(())
}

/// Returns the absolute file path for a scene's media file.
#[tauri::command]
pub async fn get_scene_path(
    app: AppHandle,
    file_relative: String,
) -> Result<String, MythicError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;

    let full_path = app_data_dir.join(&file_relative);
    if !full_path.exists() {
        return Err(MythicError::NotFound(format!("Scene file not found: {}", file_relative)));
    }

    Ok(full_path.to_string_lossy().to_string())
}

// --- Internal helpers ---

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

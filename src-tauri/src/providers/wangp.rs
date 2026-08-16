//! WanGP (Wan2GP) provider — image/video generation against a local WanGP
//! instance, "a fast AI Video Generator for the GPU Poor". Unlike ComfyUI or
//! AI Horde, WanGP has no plain REST API — its only network-facing interface
//! is an MCP server (`python wgp.py --mcp --mcp-transport streamable-http`),
//! exposing tools `wangp_generate`, `wangp_get_job`, `wangp_cancel_job`,
//! `wangp_get_model_schema`, etc. This module talks to it as an MCP client
//! via the `rmcp` crate instead of `reqwest` HTTP calls.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientInfo, Implementation};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ServiceExt;
use tauri::{AppHandle, Emitter};
use tracing::info;

use crate::error::MythicError;
use crate::models::provider::{CharacterImageRef, ImageGenParams, ProviderConfig, VideoGenParams};

/// WanGP jobs run locally; polling faster than this just wastes cycles.
const WANGP_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Local video generation on modest hardware (the whole reason someone picks
/// WanGP over ComfyUI) can genuinely take a long time — far more generous
/// than ComfyUI's 10-minute ceiling, closer to AI Horde's.
const WANGP_MAX_WAIT: Duration = Duration::from_secs(30 * 60);

fn mcp_url(base_url: &str) -> String {
    format!("{}/mcp", base_url.trim_end_matches('/'))
}

fn client_info() -> ClientInfo {
    ClientInfo::new(ClientCapabilities::default(), Implementation::new("Janus", env!("CARGO_PKG_VERSION")))
}

/// Pulls the JSON payload out of a tool call's result — prefers
/// `structured_content` (the modern MCP way for a tool to return typed
/// data), falling back to parsing the first text content block as JSON
/// (how a FastMCP-style server without an inferred output schema often
/// still returns rich data in practice).
fn extract_tool_json(result: &CallToolResult) -> Result<serde_json::Value, MythicError> {
    if let Some(structured) = &result.structured_content {
        return Ok(structured.clone());
    }
    let text = result
        .content
        .iter()
        .find_map(|c| c.as_text())
        .map(|t| t.text.as_str())
        .ok_or_else(|| MythicError::Provider("WanGP tool call returned no parseable content".to_string()))?;
    serde_json::from_str(text).map_err(|e| MythicError::Provider(format!("Failed to parse WanGP's response: {}", e)))
}

/// Recursively searches a `wangp_get_model_schema` response for a field name
/// that looks like a reference-image slot (`image_refs` is the name found in
/// WanGP's own source, but this doesn't hardcode that blindly — a real
/// schema mismatch should still be caught here rather than silently sending
/// a field WanGP ignores). Returns the *first* matching key found anywhere
/// in the schema, however it's nested.
fn find_reference_field_name(schema: &serde_json::Value) -> Option<String> {
    match schema {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let lower = key.to_lowercase();
                if lower.contains("image_ref") || lower.contains("imageref") {
                    return Some(key.clone());
                }
                if let Some(found) = find_reference_field_name(value) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => arr.iter().find_map(find_reference_field_name),
        _ => None,
    }
}

/// Progress/result payload shared by the image and video job runner —
/// connects, submits, polls until done (emitting `wangp_progress` events),
/// and returns the local filesystem path WanGP wrote its output to (no HTTP
/// fetch needed, unlike ComfyUI/AI Horde — WanGP runs on this same machine).
#[allow(clippy::too_many_arguments)]
async fn run_wangp_job(
    app: &AppHandle,
    conversation_id: &str,
    base_url: &str,
    model_type: &str,
    mut settings: serde_json::Map<String, serde_json::Value>,
    character_images: &[CharacterImageRef],
    app_data_dir: &Path,
    cancel_flag: &Arc<AtomicBool>,
) -> Result<PathBuf, MythicError> {
    let transport = StreamableHttpClientTransport::from_uri(mcp_url(base_url));
    let client = client_info()
        .serve(transport)
        .await
        .map_err(|e| MythicError::Provider(format!("Failed to connect to WanGP at {}: {}", base_url, e)))?;

    if !character_images.is_empty() {
        let schema_result = client
            .call_tool(
                CallToolRequestParams::new("wangp_get_model_schema")
                    .with_arguments(serde_json::json!({ "model_type": model_type }).as_object().cloned().unwrap_or_default()),
            )
            .await
            .map_err(|e| MythicError::Provider(format!("Failed to look up WanGP model schema for {}: {}", model_type, e)))?;
        let schema = extract_tool_json(&schema_result)?;
        let field_name = find_reference_field_name(&schema).ok_or_else(|| {
            MythicError::Validation(format!(
                "WanGP model '{}' doesn't appear to support character reference images — pick a VACE-family model instead.",
                model_type
            ))
        })?;

        let mut abs_paths = Vec::with_capacity(character_images.len());
        for char_image in character_images {
            let abs_path = crate::error::resolve_within(app_data_dir, &char_image.relative_path)?;
            abs_paths.push(abs_path.to_string_lossy().to_string());
        }
        settings.insert(field_name, serde_json::json!(abs_paths));
    }

    let generate_result = client
        .call_tool(
            CallToolRequestParams::new("wangp_generate").with_arguments(
                serde_json::json!({ "source": serde_json::Value::Object(settings), "wait": false })
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            ),
        )
        .await
        .map_err(|e| MythicError::Provider(format!("WanGP rejected the generation request: {}", e)))?;
    let generate_json = extract_tool_json(&generate_result)?;
    let job_id = generate_json
        .get("job_id")
        .and_then(|v| v.as_str().map(String::from).or_else(|| v.as_i64().map(|n| n.to_string())))
        .ok_or_else(|| MythicError::Provider("WanGP did not return a job_id".to_string()))?;

    info!("[wangp] Job {} submitted at {} (model_type={})", job_id, base_url, model_type);

    let started = std::time::Instant::now();
    loop {
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = client
                .call_tool(
                    CallToolRequestParams::new("wangp_cancel_job")
                        .with_arguments(serde_json::json!({ "job_id": job_id }).as_object().cloned().unwrap_or_default()),
                )
                .await;
            let _ = client.cancel().await;
            return Err(MythicError::Provider("Generation cancelled".to_string()));
        }
        if started.elapsed() > WANGP_MAX_WAIT {
            let _ = client
                .call_tool(
                    CallToolRequestParams::new("wangp_cancel_job")
                        .with_arguments(serde_json::json!({ "job_id": job_id }).as_object().cloned().unwrap_or_default()),
                )
                .await;
            return Err(MythicError::Provider("WanGP generation timed out".to_string()));
        }

        tokio::time::sleep(WANGP_POLL_INTERVAL).await;

        let job_result = client
            .call_tool(
                CallToolRequestParams::new("wangp_get_job")
                    .with_arguments(serde_json::json!({ "job_id": job_id, "event_limit": 5 }).as_object().cloned().unwrap_or_default()),
            )
            .await
            .map_err(|e| MythicError::Provider(format!("Failed to poll WanGP job {}: {}", job_id, e)))?;
        let job_json = extract_tool_json(&job_result)?;

        if let Some(progress) = job_json.get("progress") {
            let _ = app.emit(
                "wangp_progress",
                serde_json::json!({
                    "conversation_id": conversation_id,
                    "phase": progress.get("phase"),
                    "status": progress.get("status"),
                    "progress": progress.get("progress"),
                    "current_step": progress.get("current_step"),
                    "total_steps": progress.get("total_steps"),
                }),
            );
        }

        let Some(result) = job_json.get("result") else { continue };
        let success = result.get("success").and_then(|s| s.as_bool()).unwrap_or(false);
        if !success {
            let msg = result
                .get("errors")
                .and_then(|e| e.as_array())
                .and_then(|a| a.first())
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("WanGP generation failed");
            return Err(MythicError::Provider(msg.to_string()));
        }
        let file_path = result
            .get("generated_files")
            .and_then(|f| f.as_array())
            .and_then(|a| a.first())
            .and_then(|f| f.as_str())
            .ok_or_else(|| MythicError::Provider("WanGP reported success but returned no generated file".to_string()))?;

        let _ = client.cancel().await;
        return Ok(PathBuf::from(file_path));
    }
}

/// Generates a scene image via a WanGP provider. `model_override`, when set,
/// takes precedence over the provider's own configured `model_type` — same
/// convention as AI Horde's `model_override` in `generate_scene`.
pub async fn generate_image_via_wangp(
    app: &AppHandle,
    conversation_id: &str,
    provider: &ProviderConfig,
    params: &ImageGenParams,
    model_override: Option<&str>,
    character_images: &[CharacterImageRef],
    app_data_dir: &Path,
    cancel_flag: &Arc<AtomicBool>,
) -> Result<(Vec<u8>, serde_json::Value), MythicError> {
    let base_url = provider.config["base_url"].as_str().unwrap_or("http://127.0.0.1:7866");
    let model_type = model_override
        .filter(|s| !s.trim().is_empty())
        // "model" (not "model_type") is the DB config key here — it's the
        // same generic "Default Model" field the Providers page already
        // shows for every non-LLM adapter (AI Horde, ComfyUI); WanGP just
        // interprets whatever's in it as its own model_type identifier
        // (e.g. "qwen_image_20B").
        .or_else(|| provider.config["model"].as_str().filter(|s| !s.trim().is_empty()))
        .ok_or_else(|| {
            MythicError::Validation(
                "This WanGP provider has no model set. Add one (its model_type, e.g. qwen_image_20B) in Settings → Providers.".to_string(),
            )
        })?;

    let mut settings = serde_json::Map::new();
    settings.insert("model_type".to_string(), serde_json::json!(model_type));
    settings.insert("prompt".to_string(), serde_json::json!(params.prompt));
    settings.insert("resolution".to_string(), serde_json::json!(format!("{}x{}", params.width, params.height)));
    settings.insert("num_inference_steps".to_string(), serde_json::json!(params.steps));
    settings.insert("image_mode".to_string(), serde_json::json!(1));
    if !params.negative_prompt.is_empty() {
        settings.insert("negative_prompt".to_string(), serde_json::json!(params.negative_prompt));
    }
    if let Some(seed) = params.seed {
        settings.insert("seed".to_string(), serde_json::json!(seed));
    }

    let file_path =
        run_wangp_job(app, conversation_id, base_url, model_type, settings, character_images, app_data_dir, cancel_flag).await?;
    let bytes = tokio::fs::read(&file_path)
        .await
        .map_err(|e| MythicError::Provider(format!("Failed to read WanGP's generated image at {}: {}", file_path.display(), e)))?;

    let metadata = serde_json::json!({
        "provider": "wangp",
        "provider_name": provider.name,
        "model_type": model_type,
        "character_images_used": character_images.len(),
    });
    Ok((bytes, metadata))
}

/// Generates a scene video via a WanGP provider. See
/// `generate_image_via_wangp` for the `model_override` convention.
pub async fn generate_video_via_wangp(
    app: &AppHandle,
    conversation_id: &str,
    provider: &ProviderConfig,
    params: &VideoGenParams,
    model_override: Option<&str>,
    character_images: &[CharacterImageRef],
    app_data_dir: &Path,
    cancel_flag: &Arc<AtomicBool>,
) -> Result<(Vec<u8>, serde_json::Value), MythicError> {
    let base_url = provider.config["base_url"].as_str().unwrap_or("http://127.0.0.1:7866");
    let model_type = model_override
        .filter(|s| !s.trim().is_empty())
        // "model" (not "model_type") is the DB config key here — it's the
        // same generic "Default Model" field the Providers page already
        // shows for every non-LLM adapter (AI Horde, ComfyUI); WanGP just
        // interprets whatever's in it as its own model_type identifier
        // (e.g. "qwen_image_20B").
        .or_else(|| provider.config["model"].as_str().filter(|s| !s.trim().is_empty()))
        .ok_or_else(|| {
            MythicError::Validation(
                "This WanGP provider has no model set. Add one (its model_type, e.g. qwen_image_20B) in Settings → Providers.".to_string(),
            )
        })?;

    let video_length = (params.duration_seconds * params.fps as f32).round().max(1.0) as u32;
    let mut settings = serde_json::Map::new();
    settings.insert("model_type".to_string(), serde_json::json!(model_type));
    settings.insert("prompt".to_string(), serde_json::json!(params.prompt));
    settings.insert("resolution".to_string(), serde_json::json!(format!("{}x{}", params.width, params.height)));
    settings.insert("video_length".to_string(), serde_json::json!(video_length));
    settings.insert("force_fps".to_string(), serde_json::json!(params.fps));
    settings.insert("duration_seconds".to_string(), serde_json::json!(params.duration_seconds));
    if !params.negative_prompt.is_empty() {
        settings.insert("negative_prompt".to_string(), serde_json::json!(params.negative_prompt));
    }
    if let Some(seed) = params.seed {
        settings.insert("seed".to_string(), serde_json::json!(seed));
    }

    let file_path =
        run_wangp_job(app, conversation_id, base_url, model_type, settings, character_images, app_data_dir, cancel_flag).await?;
    let bytes = tokio::fs::read(&file_path)
        .await
        .map_err(|e| MythicError::Provider(format!("Failed to read WanGP's generated video at {}: {}", file_path.display(), e)))?;

    let metadata = serde_json::json!({
        "provider": "wangp",
        "provider_name": provider.name,
        "model_type": model_type,
        "character_images_used": character_images.len(),
    });
    Ok((bytes, metadata))
}

/// Bare connectivity check for Settings → Providers' "Test Connection" —
/// connects and asks for the tool list, no generation involved. A plain HTTP
/// GET to `/mcp` (what every other adapter's health check does) doesn't
/// reliably signal an MCP server is alive without real protocol framing.
pub async fn test_connection(base_url: &str) -> Result<(), MythicError> {
    let transport = StreamableHttpClientTransport::from_uri(mcp_url(base_url));
    let client = client_info()
        .serve(transport)
        .await
        .map_err(|e| MythicError::Provider(format!("Failed to connect to WanGP at {}: {}", base_url, e)))?;
    client.list_tools(Default::default()).await.map_err(|e| MythicError::Provider(format!("WanGP didn't respond: {}", e)))?;
    let _ = client.cancel().await;
    Ok(())
}

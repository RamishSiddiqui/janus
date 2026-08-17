//! ComfyUI provider — general-purpose scene generation against a
//! user-supplied ComfyUI workflow (API-format JSON).
//!
//! ComfyUI workflows are arbitrary node graphs — there's no fixed set of
//! node IDs or types this code could assume. The only way to support *any*
//! workflow generically (not hardcoded to one specific graph) is
//! placeholder-token substitution: the user drops small documented tokens
//! into whichever node field values they want Janus to fill in dynamically
//! when they export their workflow ("Save (API Format)" in ComfyUI). See
//! [`substitute_placeholders`] for the token contract.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reqwest::multipart;
use tracing::{info, warn};

use crate::error::MythicError;
use crate::models::provider::{CharacterImageRef, ImageGenParams, ProviderConfig};

/// How often to poll `/history/{prompt_id}`. ComfyUI has no server-side
/// cache guard like AI Horde's `check` endpoint, but polling much faster
/// than this just wastes cycles on typically multi-second local jobs.
const COMFYUI_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// A local ComfyUI instance is normally fast; a much shorter ceiling than AI
/// Horde's (which queues on a shared, often-congested public cluster) is
/// appropriate — a hung/misconfigured local server shouldn't leave the UI
/// "generating" for up to an hour.
const COMFYUI_MAX_WAIT: Duration = Duration::from_secs(10 * 60);

/// Values needed to fill in a workflow's placeholder tokens.
pub struct ComfyWorkflowContext<'a> {
    pub prompt: &'a str,
    pub negative_prompt: &'a str,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    /// Uploaded (server-side) filenames, in the same order the user
    /// selected the source cast members in — index 0 fills
    /// `{{CHARACTER_IMAGE_1}}`, index 1 fills `{{CHARACTER_IMAGE_2}}`, etc.
    pub character_image_filenames: &'a [String],
}

/// Finds the highest `N` referenced by a `{{CHARACTER_IMAGE_N}}` token
/// anywhere in the workflow. Used to validate the request has enough
/// portraits *before* anything is sent to ComfyUI — without this check, a
/// workflow needing more images than were selected would silently send a
/// literal unsubstituted `{{CHARACTER_IMAGE_2}}` string into a `LoadImage`
/// node's `image` field, failing deep inside ComfyUI with an opaque "file
/// not found" instead of a clear, actionable error here.
pub fn find_max_character_image_index(workflow: &serde_json::Value) -> u32 {
    let mut max = 0u32;
    scan_for_max_index(workflow, &mut max);
    max
}

fn scan_for_max_index(value: &serde_json::Value, max: &mut u32) {
    match value {
        serde_json::Value::String(s) => {
            let mut rest = s.as_str();
            while let Some(start) = rest.find("{{CHARACTER_IMAGE_") {
                let after = &rest[start + "{{CHARACTER_IMAGE_".len()..];
                if let Some(end) = after.find("}}") {
                    if let Ok(n) = after[..end].parse::<u32>() {
                        if n > *max {
                            *max = n;
                        }
                    }
                    rest = &after[end + 2..];
                } else {
                    break;
                }
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                scan_for_max_index(v, max);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                scan_for_max_index(v, max);
            }
        }
        _ => {}
    }
}

/// Recursively substitutes every known placeholder token in `workflow`, in
/// place. All tokens are optional — a workflow that only uses
/// `{{POSITIVE_PROMPT}}` still works, and one with none at all just runs
/// as a fixed workflow every time.
///
/// Numeric tokens (`{{SEED}}`, `{{WIDTH}}`, `{{HEIGHT}}`) only replace the
/// *whole* value, and only when the string is exactly the token — a
/// ComfyUI numeric field can't hold a partial string. Every other token
/// also supports the token being embedded inside a larger string (e.g. a
/// user writing `"{{POSITIVE_PROMPT}}, best quality"` in a CLIPTextEncode
/// node to append fixed quality tags around the dynamic prompt).
pub fn substitute_placeholders(workflow: &mut serde_json::Value, ctx: &ComfyWorkflowContext) {
    match workflow {
        serde_json::Value::String(s) => {
            if let Some(replaced) = substitute_string(s, ctx) {
                *workflow = replaced;
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                substitute_placeholders(v, ctx);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                substitute_placeholders(v, ctx);
            }
        }
        _ => {}
    }
}

fn substitute_string(s: &str, ctx: &ComfyWorkflowContext) -> Option<serde_json::Value> {
    match s {
        "{{SEED}}" => return Some(serde_json::json!(ctx.seed)),
        "{{WIDTH}}" => return Some(serde_json::json!(ctx.width)),
        "{{HEIGHT}}" => return Some(serde_json::json!(ctx.height)),
        _ => {}
    }

    if s.contains("{{CHARACTER_IMAGE_") {
        let mut result = s.to_string();
        for (i, filename) in ctx.character_image_filenames.iter().enumerate() {
            let token = format!("{{{{CHARACTER_IMAGE_{}}}}}", i + 1);
            if result.contains(&token) {
                result = result.replace(&token, filename);
            }
        }
        return if result != s {
            Some(serde_json::Value::String(result))
        } else {
            None
        };
    }

    if s.contains("{{POSITIVE_PROMPT}}") || s.contains("{{NEGATIVE_PROMPT}}") {
        let result = s
            .replace("{{POSITIVE_PROMPT}}", ctx.prompt)
            .replace("{{NEGATIVE_PROMPT}}", ctx.negative_prompt);
        return Some(serde_json::Value::String(result));
    }

    None
}

/// Derives a random u64 without pulling in a `rand` dependency just for
/// this — a v4 UUID's bytes already come from a CSPRNG, which is more than
/// enough entropy for an image generation seed.
fn random_seed() -> u64 {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    u64::from_le_bytes(bytes[0..8].try_into().unwrap())
}

/// Uploads one portrait to ComfyUI's `input/` folder via `POST
/// /upload/image` so it can be referenced by filename in a `LoadImage`
/// node. Uses a deterministic per-character filename so repeat generations
/// overwrite the same file server-side instead of accumulating one PNG per
/// generation forever.
async fn upload_image(
    http_client: &reqwest::Client,
    base_url: &str,
    character_id: &str,
    bytes: Vec<u8>,
) -> Result<String, MythicError> {
    let filename = format!("janus_char_{}.png", character_id);
    let part = multipart::Part::bytes(bytes)
        .file_name(filename.clone())
        .mime_str("image/png")
        .map_err(|e| {
            MythicError::Provider(format!("Failed to prepare upload for {}: {}", filename, e))
        })?;
    let form = multipart::Form::new()
        .part("image", part)
        .text("type", "input")
        .text("overwrite", "true");

    let resp = http_client
        .post(format!("{}/upload/image", base_url))
        .multipart(form)
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("ComfyUI image upload failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "ComfyUI rejected the image upload ({}): {}",
            status, body
        )));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| {
        MythicError::Provider(format!("Failed to parse ComfyUI upload response: {}", e))
    })?;
    json["name"].as_str().map(|s| s.to_string()).ok_or_else(|| {
        MythicError::Provider("ComfyUI's upload response had no filename".to_string())
    })
}

async fn queue_prompt(
    http_client: &reqwest::Client,
    base_url: &str,
    workflow: serde_json::Value,
) -> Result<String, MythicError> {
    let client_id = uuid::Uuid::new_v4().to_string();
    let resp = http_client
        .post(format!("{}/prompt", base_url))
        .json(&serde_json::json!({ "prompt": workflow, "client_id": client_id }))
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("ComfyUI queue request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "ComfyUI rejected the workflow ({}): {}",
            status, body
        )));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| {
        MythicError::Provider(format!("Failed to parse ComfyUI queue response: {}", e))
    })?;
    json["prompt_id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| {
            let node_errors = json.get("node_errors").cloned().unwrap_or_default();
            MythicError::Provider(format!(
                "ComfyUI did not return a prompt_id (node_errors: {})",
                node_errors
            ))
        })
}

/// Polls `/history/{prompt_id}` until the job completes (successfully or
/// not), is cancelled, or times out, then fetches the first output image's
/// raw bytes via `/view`.
async fn poll_and_fetch(
    http_client: &reqwest::Client,
    base_url: &str,
    prompt_id: &str,
    cancel_flag: &Arc<AtomicBool>,
) -> Result<Vec<u8>, MythicError> {
    let started = std::time::Instant::now();
    loop {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err(MythicError::Provider("Generation cancelled".to_string()));
        }
        if started.elapsed() > COMFYUI_MAX_WAIT {
            return Err(MythicError::Provider(
                "ComfyUI generation timed out".to_string(),
            ));
        }

        tokio::time::sleep(COMFYUI_POLL_INTERVAL).await;

        let resp = http_client
            .get(format!("{}/history/{}", base_url, prompt_id))
            .send()
            .await
            .map_err(|e| MythicError::Provider(format!("ComfyUI history check failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!(
                "[comfyui] history check for {} returned {}: {} — retrying",
                prompt_id, status, body
            );
            continue;
        }

        let history: serde_json::Value = resp.json().await.map_err(|e| {
            MythicError::Provider(format!("Failed to parse ComfyUI history response: {}", e))
        })?;

        // Keyed by prompt_id; an empty `{}` (missing key) means still queued/running.
        let Some(entry) = history.get(prompt_id) else {
            continue;
        };

        let image_ref = entry
            .get("outputs")
            .and_then(|o| o.as_object())
            .and_then(|outputs| {
                outputs.values().find_map(|node_output| {
                    node_output
                        .get("images")
                        .and_then(|imgs| imgs.as_array())
                        .and_then(|imgs| imgs.first())
                })
            });

        let Some(image_ref) = image_ref else {
            // Present in history but no image found yet in any node's
            // outputs — either still finishing up, or a real failure with
            // no SaveImage/PreviewImage node in the workflow at all.
            let status_completed = entry
                .get("status")
                .and_then(|s| s.get("completed"))
                .and_then(|c| c.as_bool())
                .unwrap_or(false);
            if status_completed {
                return Err(MythicError::Provider(
                    "ComfyUI finished the workflow but produced no image — make sure it has a SaveImage or PreviewImage node".to_string(),
                ));
            }
            continue;
        };

        let filename = image_ref["filename"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let subfolder = image_ref["subfolder"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let img_type = image_ref["type"].as_str().unwrap_or("output").to_string();

        let view_resp = http_client
            .get(format!("{}/view", base_url))
            .query(&[
                ("filename", &filename),
                ("subfolder", &subfolder),
                ("type", &img_type),
            ])
            .send()
            .await
            .map_err(|e| MythicError::Provider(format!("ComfyUI image fetch failed: {}", e)))?;

        if !view_resp.status().is_success() {
            return Err(MythicError::Provider(format!(
                "ComfyUI image fetch failed ({})",
                view_resp.status()
            )));
        }

        let bytes = view_resp.bytes().await.map_err(|e| {
            MythicError::Provider(format!("Failed to read the image ComfyUI returned: {}", e))
        })?;
        return Ok(bytes.to_vec());
    }
}

/// Generates a scene via a user-configured ComfyUI provider: uploads
/// whichever cast portraits were selected, substitutes every placeholder
/// token the workflow actually references, queues it, and waits for the
/// resulting image. Returns raw bytes (whatever format the workflow's own
/// SaveImage node produces, typically PNG) + metadata.
pub async fn generate_via_comfyui(
    http_client: &reqwest::Client,
    provider: &ProviderConfig,
    params: &ImageGenParams,
    character_images: &[CharacterImageRef],
    app_data_dir: &Path,
    cancel_flag: &Arc<AtomicBool>,
) -> Result<(Vec<u8>, serde_json::Value), MythicError> {
    let base_url = provider.config["base_url"]
        .as_str()
        .unwrap_or("http://localhost:8188");
    let workflow_str = provider.config["workflow"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            MythicError::Validation("This ComfyUI provider has no workflow configured. Add one in Settings → Providers.".to_string())
        })?;
    let mut workflow: serde_json::Value = serde_json::from_str(workflow_str).map_err(|e| {
        MythicError::Validation(format!(
            "This ComfyUI provider's workflow isn't valid JSON: {}",
            e
        ))
    })?;

    let required = find_max_character_image_index(&workflow);
    if required as usize > character_images.len() {
        return Err(MythicError::Validation(format!(
            "This workflow expects {} character image{}, but only {} {} selected.",
            required,
            if required == 1 { "" } else { "s" },
            character_images.len(),
            if character_images.len() == 1 {
                "was"
            } else {
                "were"
            },
        )));
    }

    let mut character_image_filenames = Vec::with_capacity(character_images.len());
    for char_image in character_images {
        let abs_path = crate::error::resolve_within(app_data_dir, &char_image.relative_path)?;
        let bytes = tokio::fs::read(&abs_path).await.map_err(|e| {
            MythicError::Provider(format!(
                "Failed to read {}'s portrait: {}",
                char_image.character_name, e
            ))
        })?;
        let uploaded_name =
            upload_image(http_client, base_url, &char_image.character_id, bytes).await?;
        character_image_filenames.push(uploaded_name);
    }

    let seed = params.seed.unwrap_or_else(random_seed);
    let ctx = ComfyWorkflowContext {
        prompt: &params.prompt,
        negative_prompt: &params.negative_prompt,
        width: params.width,
        height: params.height,
        seed,
        character_image_filenames: &character_image_filenames,
    };
    substitute_placeholders(&mut workflow, &ctx);

    info!(
        "[comfyui] Queuing workflow at {} ({} character image(s) substituted)",
        base_url,
        character_image_filenames.len()
    );
    let prompt_id = queue_prompt(http_client, base_url, workflow).await?;
    let image_bytes = poll_and_fetch(http_client, base_url, &prompt_id, cancel_flag).await?;

    let metadata = serde_json::json!({
        "provider": "comfyui",
        "provider_name": provider.name,
        "seed": seed,
        "character_images_used": character_image_filenames.len(),
    });

    Ok((image_bytes, metadata))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(prompt: &'a str, images: &'a [String]) -> ComfyWorkflowContext<'a> {
        ComfyWorkflowContext {
            prompt,
            negative_prompt: "blurry",
            width: 512,
            height: 768,
            seed: 42,
            character_image_filenames: images,
        }
    }

    #[test]
    fn substitutes_text_and_numeric_tokens() {
        let mut wf = serde_json::json!({
            "3": { "inputs": { "seed": "{{SEED}}", "width": "{{WIDTH}}" } },
            "5": { "inputs": { "text": "{{POSITIVE_PROMPT}}, best quality" } },
        });
        substitute_placeholders(&mut wf, &ctx("a cat", &[]));
        assert_eq!(wf["3"]["inputs"]["seed"], serde_json::json!(42));
        assert_eq!(wf["3"]["inputs"]["width"], serde_json::json!(512));
        assert_eq!(wf["5"]["inputs"]["text"], "a cat, best quality");
    }

    #[test]
    fn substitutes_character_images_by_index() {
        let images = vec![
            "janus_char_a.png".to_string(),
            "janus_char_b.png".to_string(),
        ];
        let mut wf = serde_json::json!({
            "10": { "inputs": { "image": "{{CHARACTER_IMAGE_1}}" } },
            "11": { "inputs": { "image": "{{CHARACTER_IMAGE_2}}" } },
        });
        substitute_placeholders(&mut wf, &ctx("x", &images));
        assert_eq!(wf["10"]["inputs"]["image"], "janus_char_a.png");
        assert_eq!(wf["11"]["inputs"]["image"], "janus_char_b.png");
    }

    #[test]
    fn finds_highest_character_image_index() {
        let wf = serde_json::json!({
            "a": "{{CHARACTER_IMAGE_1}}",
            "b": { "nested": "{{CHARACTER_IMAGE_3}}" },
            "c": ["{{CHARACTER_IMAGE_2}}"],
        });
        assert_eq!(find_max_character_image_index(&wf), 3);
    }

    #[test]
    fn workflow_with_no_tokens_is_untouched() {
        let mut wf = serde_json::json!({ "fixed": "always the same prompt" });
        let before = wf.clone();
        substitute_placeholders(&mut wf, &ctx("ignored", &[]));
        assert_eq!(wf, before);
    }
}

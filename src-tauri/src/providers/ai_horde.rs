//! AI Horde image generation adapter — submits an async job, polls until
//! done, and fetches the result. Mirrors the shape of `comfyui.rs`/`wangp.rs`
//! (raw bytes + metadata, no DB writes/file writes here — the caller does
//! that; the one DB access is the read-only enabled-model check below).

use std::time::Duration;

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

use crate::db::providers::ProviderRepo;
use crate::error::MythicError;
use crate::models::image_preset::ImagePreset;
use crate::models::provider::{ImageGenParams, ProviderConfig};

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
            sampler_name: provider.config["sampler_name"]
                .as_str()
                .unwrap_or("k_euler_a"),
            cfg_scale: provider.config["cfg_scale"].as_f64().unwrap_or(7.5),
            steps: provider.config["steps"].as_u64().unwrap_or(30),
            // The bare API default is `false`, but community consensus
            // favors `true` for smoother results at the same step count.
            karras: provider.config["karras"].as_bool().unwrap_or(true),
            style: provider.config["style"].as_str().filter(|s| !s.is_empty()),
            negative_prompt: provider.config["negative_prompt"]
                .as_str()
                .filter(|s| !s.is_empty()),
            clip_skip: provider.config["clip_skip"].as_u64().map(|v| v as u32),
            post_processing: &[],
            hires_fix: provider.config["hires_fix"].as_bool().unwrap_or(false),
            hires_fix_denoising_strength: provider.config["hires_fix_denoising_strength"].as_f64(),
        }
    }
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
    db: &Surreal<Db>,
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
    let api_key = provider.config["api_key"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("0000000000");

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

    // An explicitly-configured model (Default Model field or an Image
    // Preset override) must actually be enabled locally — without this, the
    // Image/Video Models page's toggle was purely cosmetic for AI Horde:
    // turning a model off there had zero effect on what generation actually
    // used, unlike every other provider type where "enabled" is the real
    // gate. The no-model-pinned auto-pick-from-the-live-roster path below is
    // intentionally left alone — it never claimed to respect local enable
    // state and picks from AI Horde's live public roster, not a local catalog.
    if let Some(m) = model {
        let provider_id = provider.id.id.to_raw();
        let enabled = ProviderRepo::list_enabled_models(db, Some(&provider_id)).await?;
        if !enabled.iter().any(|row| row.model_id == m) {
            return Err(MythicError::Validation(format!(
                "Model '{}' is set as this provider's Default Model but isn't enabled — enable it on the Image/Video Models page first.",
                m
            )));
        }
    }

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

    // Surfaced in the returned metadata so the frontend can toast the user
    // when no default was configured — this only logs server-side otherwise,
    // and silently auto-picking a model with no visible signal was the whole
    // "is 'Deliberate' actually set as default or not?" confusion.
    let mut model_was_auto_selected = false;

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
            info!(
                "[ai_horde] No model pinned — auto-selected '{}' from the live roster",
                picked
            );
            body["models"] = serde_json::json!([picked]);
            model_was_auto_selected = true;
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
        .header(
            "Client-Agent",
            concat!("Janus:", env!("CARGO_PKG_VERSION"), ":github.com/janus"),
        )
        .json(&body)
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("AI Horde submit failed: {}", e)))?;

    if !submit_resp.status().is_success() {
        let status = submit_resp.status();
        let text = submit_resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "AI Horde rejected the request ({}): {}",
            status, text
        )));
    }

    let submit_json: serde_json::Value = submit_resp.json().await.map_err(|e| {
        MythicError::Provider(format!("Failed to parse AI Horde submit response: {}", e))
    })?;
    let job_id = submit_json["id"]
        .as_str()
        .ok_or_else(|| MythicError::Provider("AI Horde did not return a job ID".to_string()))?
        .to_string();

    info!(
        "[ai_horde] Submitted job {} (estimated kudos cost: {:?})",
        job_id,
        submit_json.get("kudos")
    );
    let _ = app.emit(
        "ai_horde_progress",
        serde_json::json!({
            "conversation_id": conversation_id,
            "phase": "queued",
            "kudos": submit_json.get("kudos"),
        }),
    );

    // Poll the lightweight `check` endpoint until done, faulted, cancelled, or timeout.
    let started = std::time::Instant::now();
    loop {
        if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
            // Best-effort cancellation so the job stops consuming a worker slot.
            let _ = http_client
                .delete(format!("{}/generate/status/{}", AI_HORDE_BASE_URL, job_id))
                .send()
                .await;
            return Err(MythicError::Provider("Generation cancelled".to_string()));
        }

        if started.elapsed() > AI_HORDE_MAX_WAIT {
            // Best-effort cancellation so the job stops consuming a worker slot.
            let _ = http_client
                .delete(format!("{}/generate/status/{}", AI_HORDE_BASE_URL, job_id))
                .send()
                .await;
            return Err(MythicError::Provider(
                "AI Horde generation timed out".to_string(),
            ));
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
            warn!(
                "[ai_horde] check for job {} returned {}: {} — retrying",
                job_id, status, body
            );
            continue;
        }

        let check: serde_json::Value = check_resp.json().await.map_err(|e| {
            MythicError::Provider(format!("Failed to parse AI Horde check response: {}", e))
        })?;

        let _ = app.emit("ai_horde_progress", serde_json::json!({
            "conversation_id": conversation_id,
            "phase": if check["processing"].as_i64().unwrap_or(0) > 0 { "processing" } else { "waiting" },
            "queue_position": check["queue_position"],
            "wait_time": check["wait_time"],
            "is_possible": check["is_possible"].as_bool().unwrap_or(true),
        }));

        if check["faulted"].as_bool().unwrap_or(false) {
            return Err(MythicError::Provider(
                "AI Horde generation faulted (no worker could complete it)".to_string(),
            ));
        }
        if !check["is_possible"].as_bool().unwrap_or(true) {
            warn!("[ai_horde] Job {} reported as not currently possible with the available worker pool — continuing to wait in case it recovers", job_id);
        }
        if check["done"].as_bool().unwrap_or(false) {
            break;
        }
    }

    let _ = app.emit(
        "ai_horde_progress",
        serde_json::json!({
            "conversation_id": conversation_id,
            "phase": "finalizing",
        }),
    );

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

    let status_json: serde_json::Value = status_resp.json().await.map_err(|e| {
        MythicError::Provider(format!("Failed to parse AI Horde status response: {}", e))
    })?;

    let generation = status_json["generations"]
        .get(0)
        .ok_or_else(|| MythicError::Provider("AI Horde returned no generations".to_string()))?;

    let img_b64 = generation["img"].as_str().ok_or_else(|| {
        MythicError::Provider("AI Horde generation had no image data".to_string())
    })?;

    let webp_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, img_b64)
        .map_err(|e| MythicError::Provider(format!("Failed to decode AI Horde image: {}", e)))?;

    // AI Horde returns WebP — re-encode to PNG for consistency with the rest
    // of the scenes pipeline (and every other image path in this app).
    let decoded = image::load_from_memory(&webp_bytes).map_err(|e| {
        MythicError::Provider(format!("Failed to decode AI Horde WebP image: {}", e))
    })?;
    let mut png_buf = std::io::Cursor::new(Vec::new());
    decoded
        .write_to(&mut png_buf, image::ImageFormat::Png)
        .map_err(|e| {
            MythicError::Provider(format!("Failed to re-encode AI Horde image as PNG: {}", e))
        })?;

    // Full generation details, captured so a scene can be replicated later
    // from the gallery — everything actually sent to AI Horde for this job.
    let mut metadata = serde_json::json!({
        "provider": "AI Horde",
        "model": generation.get("model"),
        "model_was_auto_selected": model_was_auto_selected,
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

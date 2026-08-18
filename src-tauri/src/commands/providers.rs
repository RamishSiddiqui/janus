use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use crate::db::ai_horde_models::AiHordeModelRepo;
use crate::db::providers::ProviderRepo;
use crate::error::{validate_required_string, MythicError};
use crate::models::ai_horde_model::AiHordeModelInfo;
use crate::models::provider::{ProviderAdapter, ProviderConfig, ProviderType};
use crate::models::DynamicJson;
use crate::AppState;

/// In-process cache for the (large — hundreds of models) static Haidra-Org
/// model reference. Model baselines/capabilities essentially never change
/// within a session, but `fetch_ai_horde_model_info` used to be called on
/// every Models-page load and every "Add Provider" adapter switch, re-
/// downloading the whole file from raw.githubusercontent.com each time.
/// Live worker counts (a separate, cheap request) are always fetched fresh.
static REFERENCE_CACHE: std::sync::OnceLock<
    tokio::sync::Mutex<Option<(std::time::Instant, HashMap<String, serde_json::Value>)>>,
> = std::sync::OnceLock::new();
const REFERENCE_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(60 * 60);

async fn fetch_model_reference(http: &reqwest::Client) -> HashMap<String, serde_json::Value> {
    let cache = REFERENCE_CACHE.get_or_init(|| tokio::sync::Mutex::new(None));
    {
        let guard = cache.lock().await;
        if let Some((fetched_at, data)) = guard.as_ref() {
            if fetched_at.elapsed() < REFERENCE_CACHE_TTL {
                return data.clone();
            }
        }
    }

    let fresh: HashMap<String, serde_json::Value> = match http
        .get("https://raw.githubusercontent.com/Haidra-Org/AI-Horde-image-model-reference/main/stable_diffusion.json")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) => r.json().await.unwrap_or_default(),
        Err(_) => HashMap::new(),
    };

    if !fresh.is_empty() {
        *cache.lock().await = Some((std::time::Instant::now(), fresh.clone()));
    }
    fresh
}

/// Fetches AI Horde's live worker-availability list and merges it with the
/// static Haidra-Org model reference (baseline/inpainting/nsfw) — best
/// effort: if the reference fetch fails, live models are still returned
/// with unknown capability info rather than failing the whole call.
async fn fetch_ai_horde_model_info(http: &reqwest::Client) -> Vec<AiHordeModelInfo> {
    let live: Vec<serde_json::Value> = match http
        .get("https://aihorde.net/api/v2/status/models?type=image")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) => r.json().await.unwrap_or_default(),
        Err(_) => return Vec::new(),
    };
    if live.is_empty() {
        return Vec::new();
    }

    let reference = fetch_model_reference(http).await;

    live.into_iter()
        .filter_map(|m| {
            let name = m.get("name")?.as_str()?.to_string();
            let worker_count = m.get("count").and_then(|v| v.as_i64()).unwrap_or(0);

            // The reference is keyed by model name matching AI Horde's own
            // naming, but fall back to scanning by each entry's own `name`
            // field in case keying ever diverges from display names.
            let ref_entry = reference.get(&name).or_else(|| {
                reference
                    .values()
                    .find(|v| v.get("name").and_then(|n| n.as_str()) == Some(name.as_str()))
            });

            let baseline = ref_entry
                .and_then(|e| e.get("baseline"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let inpainting = ref_entry
                .and_then(|e| e.get("inpainting"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let nsfw = ref_entry
                .and_then(|e| e.get("nsfw"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let style = ref_entry
                .and_then(|e| e.get("style"))
                .and_then(|v| v.as_str())
                .map(String::from);

            // img2img is a generic diffusion capability, not gated by a
            // dedicated per-model API flag — but newer architectures are
            // documented as unreliable/unsupported for it on AI Horde: Flux
            // img2img produces blurred/oversaturated results, and Stable
            // Cascade originally launched text2img-only.
            let baseline_lower = baseline.as_deref().unwrap_or_default().to_lowercase();
            let img2img_supported =
                !(baseline_lower.contains("flux") || baseline_lower.contains("cascade"));

            Some(AiHordeModelInfo {
                name,
                baseline,
                inpainting,
                nsfw,
                style,
                img2img_supported,
                worker_count,
            })
        })
        .collect()
}

/// Runs `fetch_ai_horde_model_info` and caches the result — best effort,
/// cache failures are logged but never fail the caller.
async fn refresh_ai_horde_model_info(
    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    http: &reqwest::Client,
) -> Vec<AiHordeModelInfo> {
    let models = fetch_ai_horde_model_info(http).await;
    if !models.is_empty() {
        if let Err(e) = AiHordeModelRepo::upsert_many(db, &models).await {
            tracing::warn!("Failed to cache AI Horde model info: {}", e);
        }
    }
    models
}

/// Creates a new provider configuration.
#[tauri::command]
#[specta::specta]
pub async fn create_provider(
    state: State<'_, Arc<RwLock<AppState>>>,
    name: String,
    provider_type: String,
    adapter: String,
    config: DynamicJson,
    is_default: Option<bool>,
) -> Result<ProviderConfig, MythicError> {
    validate_required_string("Provider name", &name, 100)?;

    // Validate type and adapter strings
    let _ptype = parse_provider_type(&provider_type)?;
    let _padapter = parse_adapter(&adapter)?;
    let is_default = is_default.unwrap_or(false);

    let state = state.read().await;
    let provider = ProviderRepo::create(
        &state.db,
        &name,
        &provider_type,
        &adapter,
        config.0,
        is_default,
    )
    .await?;

    info!(
        "Created provider: {} ({}) [{}]",
        name, adapter, provider_type
    );
    Ok(provider)
}

/// Retrieves a single provider by ID.
#[tauri::command]
#[specta::specta]
pub async fn get_provider(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<ProviderConfig, MythicError> {
    let state = state.read().await;
    ProviderRepo::get(&state.db, &id).await
}

/// Lists all providers, optionally filtered by type.
#[tauri::command]
#[specta::specta]
pub async fn list_providers(
    state: State<'_, Arc<RwLock<AppState>>>,
    provider_type: Option<String>,
) -> Result<Vec<ProviderConfig>, MythicError> {
    let state = state.read().await;
    ProviderRepo::list(&state.db, provider_type.as_deref()).await
}

/// Updates an existing provider configuration.
#[tauri::command]
#[specta::specta]
pub async fn update_provider(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    name: Option<String>,
    config: Option<DynamicJson>,
) -> Result<ProviderConfig, MythicError> {
    if let Some(ref name) = name {
        validate_required_string("Provider name", name, 100)?;
    }

    let state = state.read().await;
    let provider =
        ProviderRepo::update(&state.db, &id, name.as_deref(), config.map(|c| c.0)).await?;
    info!("Updated provider: {}", id);
    Ok(provider)
}

/// Deletes a provider configuration.
#[tauri::command]
#[specta::specta]
pub async fn delete_provider(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;
    ProviderRepo::delete(&state.db, &id).await?;
    info!("Deleted provider: {}", id);
    Ok(())
}

/// Sets a provider as the default for its type. Unsets all others of the same type.
#[tauri::command]
#[specta::specta]
pub async fn set_default_provider(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;
    ProviderRepo::set_default(&state.db, &id).await?;
    info!("Set default provider: {}", id);
    Ok(())
}

/// Tests connectivity to a provider by attempting a health check.
/// Result of a provider connection test — `ok: false` always comes with a
/// `detail` explaining *why*, since a bare "unreachable" collapses several
/// very different failure modes (bad API key, wrong Base URL, the actual
/// server being down, a timeout, DNS failure...) into one meaningless word
/// that gives the user nothing to act on.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub detail: Option<String>,
}

fn ok_result() -> ConnectionTestResult {
    ConnectionTestResult {
        ok: true,
        detail: None,
    }
}

fn fail_result(detail: impl Into<String>) -> ConnectionTestResult {
    ConnectionTestResult {
        ok: false,
        detail: Some(detail.into()),
    }
}

/// Turns an HTTP response/error into a `ConnectionTestResult`, reading the
/// response body on a non-2xx status so the actual provider-side rejection
/// reason (e.g. "invalid_api_key", "model not found") makes it back to the
/// user instead of just a status code.
async fn summarize_http_result(
    resp: Result<reqwest::Response, reqwest::Error>,
) -> ConnectionTestResult {
    match resp {
        Ok(r) => {
            let status = r.status();
            if status.is_success() {
                ok_result()
            } else {
                let body = r.text().await.unwrap_or_default();
                let body_trimmed: String = body.trim().chars().take(300).collect();
                if body_trimmed.is_empty() {
                    fail_result(format!(
                        "HTTP {} {}",
                        status.as_u16(),
                        status.canonical_reason().unwrap_or("")
                    ))
                } else {
                    fail_result(format!("HTTP {}: {}", status.as_u16(), body_trimmed))
                }
            }
        }
        Err(e) => {
            let detail = if e.is_timeout() {
                "Request timed out after 5 seconds — the server didn't respond.".to_string()
            } else if e.is_connect() {
                "Could not connect — check the Base URL and that the server is running and reachable.".to_string()
            } else {
                e.to_string()
            };
            fail_result(detail)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn test_provider_connection(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<ConnectionTestResult, MythicError> {
    let state = state.read().await;
    let provider = ProviderRepo::get(&state.db, &id).await?;

    // Extract base_url from config and attempt a simple HTTP request. Trimmed
    // of any trailing slash — the stored value already includes any version
    // segment the provider needs (e.g. NVIDIA's is
    // "https://integrate.api.nvidia.com/v1"), matching exactly how the real
    // chat client uses it (RigProvider::from_config passes it straight
    // through to rig-core's OpenAI-compatible client, unmodified) — so
    // endpoint paths below must be appended directly, never with an extra
    // "/v1/" prefix.
    let base_url = provider
        .config
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_end_matches('/');

    let api_key = provider
        .config
        .get("api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if base_url.is_empty() {
        // Cloud providers with API keys — check by trying to list models
        if api_key.is_empty() {
            return Ok(fail_result("No API key configured for this provider."));
        }

        // Use a direct HTTP check for cloud providers
        return match provider.adapter {
            ProviderAdapter::OpenRouter => {
                let resp = state
                    .http_client
                    .get("https://openrouter.ai/api/v1/models")
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("HTTP-Referer", "https://janus.app")
                    .header("X-Title", "Mythic")
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await;
                Ok(summarize_http_result(resp).await)
            }
            ProviderAdapter::AiHorde => {
                // No auth needed for a heartbeat — just confirms the service is up.
                // The configured key (even the anonymous default) is always "valid"
                // in the sense that AI Horde never rejects it outright.
                let resp = state
                    .http_client
                    .get("https://aihorde.net/api/v2/status/heartbeat")
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await;
                Ok(summarize_http_result(resp).await)
            }
            _ => Ok(ok_result()),
        };
    }

    // WanGP is MCP-only — there's no REST route a plain GET can hit to
    // signal liveness the way ComfyUI's /system_stats or Ollama's /api/tags
    // do, so this does a real (bare) MCP connect instead of building a
    // health_url below.
    if provider.adapter == ProviderAdapter::WanGp {
        return match crate::providers::wangp::test_connection(base_url).await {
            Ok(()) => Ok(ok_result()),
            Err(e) => Ok(fail_result(e.to_string())),
        };
    }

    // Local providers — check if the base URL is reachable. Also covers any
    // generic OpenAI-compatible endpoint with a configured base_url (e.g. a
    // cloud provider like NVIDIA's NIM API, not just genuinely local
    // servers like Ollama/LM Studio) — those require an API key on every
    // request, including `/models`, so the key (when configured) is
    // attached the same way real chat completions send it. Without this,
    // a perfectly valid cloud endpoint+key always reported "unreachable"
    // because the health check itself got a 401, not because anything was
    // actually down.
    let health_url = match provider.adapter {
        ProviderAdapter::Ollama => format!("{}/api/tags", base_url),
        ProviderAdapter::ComfyUi => format!("{}/system_stats", base_url),
        // OpenAI-compatible base_urls already include the version segment
        // (e.g. NVIDIA's "https://integrate.api.nvidia.com/v1", LM Studio's
        // own documented "http://localhost:1234/v1") — appending another
        // "/v1/" here produced a broken .../v1/v1/models URL for every
        // provider on this adapter, not just NVIDIA.
        _ => format!("{}/models", base_url),
    };

    let mut req = state
        .http_client
        .get(&health_url)
        .timeout(std::time::Duration::from_secs(5));
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let resp = req.send().await;

    Ok(summarize_http_result(resp).await)
}

/// Lists every model a WanGP provider knows about (with local download
/// availability), for the Default Model picker on that provider. WanGP is
/// MCP-only, so this is separate from `list_provider_models`'s plain-REST
/// model listing below (which doesn't apply here).
#[tauri::command]
#[specta::specta]
pub async fn list_wangp_models(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Vec<crate::providers::wangp::WangpModelInfo>, MythicError> {
    let state = state.read().await;
    let provider = ProviderRepo::get(&state.db, &id).await?;
    let base_url = provider
        .config
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("http://127.0.0.1:7866");
    crate::providers::wangp::list_models(base_url).await
}

/// Lists available models from a provider's API.
/// Supports Ollama (/api/tags), OpenRouter (/api/v1/models), and OpenAI-compatible (/v1/models).
#[tauri::command]
#[specta::specta]
pub async fn list_provider_models(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Vec<String>, MythicError> {
    let state = state.read().await;
    let provider = ProviderRepo::get(&state.db, &id).await?;

    let base_url = provider
        .config
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_end_matches('/');

    let api_key = provider
        .config
        .get("api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if provider.adapter == ProviderAdapter::AiHorde {
        // AI Horde has no per-provider base_url — model availability is a
        // live, shared fact about the whole Horde. Also refreshes the local
        // capability cache (baseline/img2img support) as a side effect.
        let mut infos = refresh_ai_horde_model_info(&state.db, &state.http_client).await;
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        let models: Vec<String> = infos.into_iter().map(|m| m.name).collect();
        info!(
            "Listed {} models from provider {}",
            models.len(),
            provider.name
        );
        return Ok(models);
    }

    let (url, is_ollama) = match provider.adapter {
        ProviderAdapter::Ollama => {
            let base = if base_url.is_empty() {
                "http://localhost:11434"
            } else {
                base_url
            };
            (format!("{}/api/tags", base), true)
        }
        ProviderAdapter::OpenRouter => ("https://openrouter.ai/api/v1/models".to_string(), false),
        _ => {
            // OpenAI-compatible (LM Studio, vLLM, NVIDIA NIM, etc.) —
            // base_url already includes the version segment (see the
            // matching comment in test_provider_connection above); appending
            // another "/v1/" here produced a broken .../v1/v1/models URL.
            if base_url.is_empty() {
                return Err(MythicError::Validation(
                    "Base URL is required to list models".to_string(),
                ));
            }
            (format!("{}/models", base_url), false)
        }
    };

    let mut req = state
        .http_client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10));

    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let resp = req.send().await?;
    if !resp.status().is_success() {
        return Err(MythicError::Provider(format!(
            "Failed to list models: HTTP {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp.json().await?;

    let models: Vec<String> = if is_ollama {
        // Ollama format: { "models": [{ "name": "gemma3:latest", ... }] }
        body.get("models")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        // OpenAI/OpenRouter format: { "data": [{ "id": "gpt-4o", ... }] }
        body.get("data")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };

    info!(
        "Listed {} models from provider {}",
        models.len(),
        provider.name
    );
    Ok(models)
}

// ── Model enable/disable tracking ──────────────────────────────────────────

/// A single model entry returned by `list_all_models`.
#[derive(serde::Serialize, Debug, Clone, specta::Type)]
pub struct ModelEntry {
    pub model_id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub adapter: String,
    pub model_type: String,
    pub context_length: Option<u32>,
    pub enabled: bool,
    // ── Rich metadata (populated from OpenRouter API) ──
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub pricing_prompt: Option<String>,
    pub pricing_completion: Option<String>,
    pub is_free: bool,
    pub max_completion_tokens: Option<u32>,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    /// Embedding vector dimensions (populated for embedding models)
    pub embedding_dimensions: Option<u32>,
    /// True when this model is enabled locally but no longer appears in the
    /// provider's live catalog (e.g. delisted upstream). Stale entries carry
    /// no fetched metadata (pricing, context length, etc.) — just enough to
    /// show the user what's enabled and let them turn it off.
    pub is_stale: bool,
    // ── AI Horde image-model capability info (from ai_horde_model_info) ──
    /// Model architecture (e.g. "stable diffusion 1", "stable_cascade") —
    /// only populated for AI Horde image models.
    pub baseline: Option<String>,
    /// Whether this model reliably supports img2img on AI Horde. Derived
    /// from `baseline`, not a literal per-model API flag — img2img is a
    /// generic diffusion capability, but newer architectures (Flux, Stable
    /// Cascade) are documented as unreliable/unsupported for it.
    pub img2img_supported: Option<bool>,
    /// Whether this is a dedicated inpainting-specialized checkpoint.
    pub inpainting: Option<bool>,
}

/// Fetches models from ALL configured providers in parallel and merges them
/// with their enabled/disabled state from the `enabled_models` table.
///
/// Per-provider fetches time out after 8 seconds. Partial results are returned
/// on timeout or network error rather than failing the whole call.
#[tauri::command]
#[specta::specta]
pub async fn list_all_models(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<ModelEntry>, MythicError> {
    // 1. Fetch all providers and enabled states while holding the lock
    let (providers, enabled_map, http, db) = {
        let state_guard = state.read().await;
        let providers = ProviderRepo::list(&state_guard.db, None).await?;
        let enabled_map = ProviderRepo::get_all_enabled_states(&state_guard.db).await?;
        let http = state_guard.http_client.clone();
        let db = state_guard.db.clone();
        (providers, enabled_map, http, db)
    };

    // 2. Spawn HTTP tasks per provider (only needs http_client + config data, no db)
    let mut tasks = Vec::new();
    for provider in &providers {
        let config = &provider.config;
        let base_url = config
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let adapter = serde_json::to_value(&provider.adapter)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        let provider_id = provider.id.id.to_raw();
        let provider_name = provider.name.clone();
        let provider_type = serde_json::to_value(&provider.provider_type)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        let http_c = http.clone();
        let db_c = db.clone();

        tasks.push(tokio::spawn(async move {
            struct RawModel {
                model_id: String,
                context_length: Option<u32>,
                display_name: Option<String>,
                description: Option<String>,
                pricing_prompt: Option<String>,
                pricing_completion: Option<String>,
                is_free: bool,
                max_completion_tokens: Option<u32>,
                input_modalities: Vec<String>,
                output_modalities: Vec<String>,
                supports_tools: bool,
                supports_vision: bool,
                supports_reasoning: bool,
            }

            // AI Horde: live, shared model availability across the whole
            // Horde (not a per-provider catalog) — a different response
            // shape entirely, so handled separately from the generic
            // OpenAI-compatible/Ollama/OpenRouter paths below.
            if adapter == "ai_horde" {
                let infos = refresh_ai_horde_model_info(&db_c, &http_c).await;
                return (
                    provider_id.clone(),
                    true,
                    infos
                        .into_iter()
                        .map(|info| {
                            let count = info.worker_count;
                            ModelEntry {
                                model_id: info.name,
                                provider_id: provider_id.clone(),
                                provider_name: provider_name.clone(),
                                adapter: adapter.clone(),
                                model_type: provider_type.clone(),
                                context_length: None,
                                enabled: false, // set below
                                display_name: None,
                                description: Some(format!(
                                    "{} worker{} online",
                                    count,
                                    if count == 1 { "" } else { "s" }
                                )),
                                pricing_prompt: None,
                                pricing_completion: None,
                                is_free: true,
                                max_completion_tokens: None,
                                input_modalities: vec![],
                                output_modalities: vec![],
                                supports_tools: false,
                                supports_vision: false,
                                supports_reasoning: false,
                                is_stale: false,
                                embedding_dimensions: None,
                                baseline: info.baseline,
                                img2img_supported: Some(info.img2img_supported),
                                inpainting: Some(info.inpainting),
                            }
                        })
                        .collect::<Vec<_>>(),
                );
            }

            // WanGP: MCP-only, no REST /models endpoint to hit — the generic
            // path below would just 404/timeout and silently contribute 0
            // models. list_models() talks to it over MCP instead, same as
            // the Providers page's Default Model picker.
            if adapter == "wan_gp" {
                let models = crate::providers::wangp::list_models(&base_url)
                    .await
                    .unwrap_or_default();
                return (
                    provider_id.clone(),
                    true,
                    models
                        .into_iter()
                        .map(|m| ModelEntry {
                            model_id: m.model_type,
                            provider_id: provider_id.clone(),
                            provider_name: provider_name.clone(),
                            adapter: adapter.clone(),
                            model_type: provider_type.clone(),
                            context_length: None,
                            enabled: false, // set below
                            display_name: Some(m.name),
                            description: match (m.description, m.availability_status) {
                                (Some(desc), Some(status)) => {
                                    Some(format!("{} — {}", desc, status))
                                }
                                (Some(desc), None) => Some(desc),
                                (None, Some(status)) => Some(status),
                                (None, None) => None,
                            },
                            pricing_prompt: None,
                            pricing_completion: None,
                            is_free: true,
                            max_completion_tokens: None,
                            input_modalities: vec![],
                            output_modalities: vec![],
                            supports_tools: false,
                            supports_vision: false,
                            supports_reasoning: false,
                            embedding_dimensions: None,
                            is_stale: false,
                            baseline: None,
                            img2img_supported: None,
                            inpainting: None,
                        })
                        .collect::<Vec<_>>(),
                );
            }

            let (url, is_ollama) = match adapter.as_str() {
                "ollama" => {
                    let base = if base_url.is_empty() {
                        "http://localhost:11434".to_string()
                    } else {
                        base_url
                    };
                    (format!("{}/api/tags", base), true)
                }
                "open_router" => ("https://openrouter.ai/api/v1/models".to_string(), false),
                _ => {
                    if base_url.is_empty() {
                        return (provider_id.clone(), false, vec![]);
                    }
                    // OpenAI-compatible base_urls already include the version
                    // segment (e.g. NVIDIA's "https://integrate.api.nvidia.com/v1")
                    // — see the matching comment on test_provider_connection.
                    // A third, independent copy of the same "/v1/v1/models"
                    // bug lived here, silently contributing 0 models for any
                    // such provider (errors on this per-provider fetch are
                    // swallowed below via `Err(_) => return vec![]`).
                    (format!("{}/models", base_url.trim_end_matches('/')), false)
                }
            };

            let mut req = http_c.get(&url).timeout(std::time::Duration::from_secs(8));
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key));
                if adapter == "open_router" {
                    req = req
                        .header("HTTP-Referer", "https://janus.app")
                        .header("X-Title", "Janus");
                }
            }

            let body: serde_json::Value = match req.send().await {
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(v) => v,
                    Err(_) => return (provider_id.clone(), false, vec![]),
                },
                Err(_) => return (provider_id.clone(), false, vec![]),
            };

            let entries: Vec<RawModel> = if is_ollama {
                body.get("models")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| {
                                Some(RawModel {
                                    model_id: m.get("name")?.as_str()?.to_string(),
                                    context_length: None,
                                    display_name: None,
                                    description: None,
                                    pricing_prompt: None,
                                    pricing_completion: None,
                                    is_free: false,
                                    max_completion_tokens: None,
                                    input_modalities: vec![],
                                    output_modalities: vec![],
                                    supports_tools: false,
                                    supports_vision: false,
                                    supports_reasoning: false,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                body.get("data")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| {
                                let id = m.get("id")?.as_str()?.to_string();
                                let ctx = m
                                    .get("context_length")
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as u32);
                                let name = m.get("name").and_then(|v| v.as_str()).map(String::from);
                                let desc = m.get("description").and_then(|v| v.as_str()).map(|s| {
                                    let chars: String = s.chars().take(200).collect();
                                    if chars.len() < s.len() {
                                        format!("{}...", chars)
                                    } else {
                                        chars
                                    }
                                });

                                // Pricing
                                let pricing = m.get("pricing");
                                let p_prompt = pricing
                                    .and_then(|p| p.get("prompt"))
                                    .and_then(|v| v.as_str())
                                    .map(String::from);
                                let p_completion = pricing
                                    .and_then(|p| p.get("completion"))
                                    .and_then(|v| v.as_str())
                                    .map(String::from);
                                let is_free = p_prompt.as_deref() == Some("0")
                                    && p_completion.as_deref() == Some("0");

                                // Top provider
                                let max_comp = m
                                    .get("top_provider")
                                    .and_then(|tp| tp.get("max_completion_tokens"))
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as u32);

                                // Architecture / modalities
                                let arch = m.get("architecture");
                                let input_mods: Vec<String> = arch
                                    .and_then(|a| a.get("input_modalities"))
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|x| x.as_str().map(String::from))
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                let output_mods: Vec<String> = arch
                                    .and_then(|a| a.get("output_modalities"))
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|x| x.as_str().map(String::from))
                                            .collect()
                                    })
                                    .unwrap_or_default();

                                // Supported parameters
                                let params: Vec<String> = m
                                    .get("supported_parameters")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|x| x.as_str().map(String::from))
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                let supports_tools = params.iter().any(|p| p == "tools");
                                let supports_vision = input_mods.iter().any(|m| m == "image");
                                let supports_reasoning = params
                                    .iter()
                                    .any(|p| p == "reasoning" || p == "include_reasoning");

                                Some(RawModel {
                                    model_id: id,
                                    context_length: ctx,
                                    display_name: name,
                                    description: desc,
                                    pricing_prompt: p_prompt,
                                    pricing_completion: p_completion,
                                    is_free,
                                    max_completion_tokens: max_comp,
                                    input_modalities: input_mods,
                                    output_modalities: output_mods,
                                    supports_tools,
                                    supports_vision,
                                    supports_reasoning,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };

            let parsed = entries
                .into_iter()
                .map(|raw| {
                    ModelEntry {
                        model_id: raw.model_id,
                        provider_id: provider_id.clone(),
                        provider_name: provider_name.clone(),
                        adapter: adapter.clone(),
                        model_type: provider_type.clone(),
                        context_length: raw.context_length,
                        enabled: false, // will be set below
                        display_name: raw.display_name,
                        description: raw.description,
                        pricing_prompt: raw.pricing_prompt,
                        pricing_completion: raw.pricing_completion,
                        is_free: raw.is_free,
                        max_completion_tokens: raw.max_completion_tokens,
                        input_modalities: raw.input_modalities,
                        output_modalities: raw.output_modalities,
                        supports_tools: raw.supports_tools,
                        supports_vision: raw.supports_vision,
                        supports_reasoning: raw.supports_reasoning,
                        is_stale: false,
                        embedding_dimensions: None,
                        baseline: None,
                        img2img_supported: None,
                        inpainting: None,
                    }
                })
                .collect::<Vec<_>>();
            (provider_id.clone(), true, parsed)
        }));
    }

    // 3. Collect results and merge with enabled states
    let mut all_entries: Vec<ModelEntry> = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    // Providers whose live catalog fetch actually succeeded this round. A
    // model is only ever "stale" (delisted upstream) if we successfully
    // fetched its provider's catalog and it wasn't in there — a fetch that
    // failed outright (timeout, network blip, misconfigured URL) must NOT
    // be treated the same as "provider confirms this model is gone," or a
    // transient hiccup permanently and silently disables the user's chosen
    // models (this exact class of bug is how NVIDIA's models kept getting
    // disabled while its `/v1/v1/models` URL bug was live).
    let mut fetch_ok: std::collections::HashSet<String> = std::collections::HashSet::new();
    for task in tasks {
        if let Ok((provider_id, ok, rows)) = task.await {
            if ok {
                fetch_ok.insert(provider_id);
            }
            for mut entry in rows {
                let key = (entry.provider_id.clone(), entry.model_id.clone());
                entry.enabled = enabled_map
                    .get(&key)
                    .map(|(enabled, _)| *enabled)
                    .unwrap_or(false);
                seen.insert(key);
                all_entries.push(entry);
            }
        }
    }

    // 4. Auto-disable + surface models enabled locally but absent from every
    //    provider's live catalog (e.g. delisted upstream). Without this, a
    //    delisted model would silently vanish from this list (no way to
    //    notice or turn it off) while still being served to
    //    `list_enabled_models` — and thus the chat model selector — forever.
    let stale: Vec<(String, String, String)> = enabled_map
        .iter()
        .filter(|((provider_id, model_id), (enabled, model_type))| {
            *enabled
                && model_type != "embedding"
                && fetch_ok.contains(provider_id)
                && !seen.contains(&(provider_id.clone(), model_id.clone()))
        })
        .map(|((provider_id, model_id), (_, model_type))| {
            (provider_id.clone(), model_id.clone(), model_type.clone())
        })
        .collect();

    if !stale.is_empty() {
        let state_guard = state.read().await;
        for (provider_id, model_id, model_type) in &stale {
            match ProviderRepo::toggle_model(
                &state_guard.db,
                provider_id,
                model_id,
                model_type,
                false,
            )
            .await
            {
                Ok(()) => info!(
                    "Auto-disabled stale model {} on provider {}",
                    model_id, provider_id
                ),
                Err(e) => tracing::warn!(
                    "Failed to auto-disable stale model {} on provider {}: {}",
                    model_id,
                    provider_id,
                    e
                ),
            }
        }
    }

    for (provider_id, model_id, model_type) in &stale {
        if let Some(provider) = providers.iter().find(|p| p.id.id.to_raw() == *provider_id) {
            let adapter_str = serde_json::to_value(&provider.adapter)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            all_entries.push(ModelEntry {
                model_id: model_id.clone(),
                provider_id: provider_id.clone(),
                provider_name: provider.name.clone(),
                adapter: adapter_str,
                model_type: model_type.clone(),
                context_length: None,
                enabled: false,
                display_name: None,
                description: None,
                pricing_prompt: None,
                pricing_completion: None,
                is_free: false,
                max_completion_tokens: None,
                input_modalities: vec![],
                output_modalities: vec![],
                supports_tools: false,
                supports_vision: false,
                supports_reasoning: false,
                embedding_dimensions: None,
                is_stale: true,
                baseline: None,
                img2img_supported: None,
                inpainting: None,
            });
        }
    }

    info!(
        "list_all_models: {} total models across {} providers",
        all_entries.len(),
        providers.len()
    );
    Ok(all_entries)
}

/// Fetches **embedding** models from all configured providers in parallel.
///
/// For OpenRouter: uses the dedicated `/api/v1/embeddings/models` endpoint.
/// For Ollama/LM Studio/OpenAI-compatible: fetches standard model lists and
/// filters for model IDs containing "embed".
#[tauri::command]
#[specta::specta]
pub async fn list_embedding_models(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<ModelEntry>, MythicError> {
    let (providers, enabled_map, http) = {
        let state_guard = state.read().await;
        let providers = ProviderRepo::list(&state_guard.db, None).await?;
        let enabled_map = ProviderRepo::get_all_enabled_states(&state_guard.db).await?;
        let http = state_guard.http_client.clone();
        (providers, enabled_map, http)
    };

    let mut tasks = Vec::new();
    for provider in &providers {
        let config = &provider.config;
        let base_url = config
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let adapter = serde_json::to_value(&provider.adapter)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        let provider_id = provider.id.id.to_raw();
        let provider_name = provider.name.clone();
        let http_c = http.clone();

        tasks.push(tokio::spawn(async move {
            // Determine URL based on adapter
            let (url, is_ollama, is_embedding_endpoint) = match adapter.as_str() {
                "open_router" => (
                    "https://openrouter.ai/api/v1/embeddings/models".to_string(),
                    false,
                    true, // dedicated embedding endpoint — all results are embedding models
                ),
                "ollama" => {
                    let base = if base_url.is_empty() {
                        "http://localhost:11434".to_string()
                    } else {
                        base_url
                    };
                    (format!("{}/api/tags", base), true, false)
                }
                _ => {
                    if base_url.is_empty() {
                        return (provider_id.clone(), false, vec![]);
                    }
                    // Same "/v1/v1/models" bug as the other OpenAI-compatible
                    // model-listing branches in this file — base_url already
                    // includes the version segment.
                    (
                        format!("{}/models", base_url.trim_end_matches('/')),
                        false,
                        false,
                    )
                }
            };

            let mut req = http_c.get(&url).timeout(std::time::Duration::from_secs(8));
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key));
                if adapter == "open_router" {
                    req = req
                        .header("HTTP-Referer", "https://janus.app")
                        .header("X-Title", "Janus");
                }
            }

            let body: serde_json::Value = match req.send().await {
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(v) => v,
                    Err(_) => return (provider_id.clone(), false, vec![]),
                },
                Err(_) => return (provider_id.clone(), false, vec![]),
            };

            // Parse models from response
            let raw_models: Vec<(
                String,
                Option<String>,
                Option<u32>,
                Option<String>,
                Option<String>,
                bool,
            )> = if is_ollama {
                body.get("models")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| {
                                let id = m.get("name")?.as_str()?.to_string();
                                Some((id, None, None, None, None, false))
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                body.get("data")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| {
                                let id = m.get("id")?.as_str()?.to_string();
                                let name = m.get("name").and_then(|v| v.as_str()).map(String::from);
                                let ctx = m
                                    .get("context_length")
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as u32);
                                let pricing = m.get("pricing");
                                let p_prompt = pricing
                                    .and_then(|p| p.get("prompt"))
                                    .and_then(|v| v.as_str())
                                    .map(String::from);
                                let p_completion = pricing
                                    .and_then(|p| p.get("completion"))
                                    .and_then(|v| v.as_str())
                                    .map(String::from);
                                let is_free = p_prompt.as_deref() == Some("0")
                                    && p_completion.as_deref() == Some("0");
                                Some((id, name, ctx, p_prompt, p_completion, is_free))
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };

            // Filter: if this is a dedicated embedding endpoint, keep all;
            // otherwise only keep models with "embed" in their ID
            let parsed = raw_models
                .into_iter()
                .filter(|(id, name, ..)| {
                    if is_embedding_endpoint {
                        return true;
                    }
                    let id_lower = id.to_lowercase();
                    let name_lower = name.as_deref().unwrap_or("").to_lowercase();
                    id_lower.contains("embed") || name_lower.contains("embed")
                })
                .map(
                    |(
                        model_id,
                        display_name,
                        context_length,
                        pricing_prompt,
                        pricing_completion,
                        is_free,
                    )| {
                        let dims =
                            super::embeddings::get_model_dimension(&model_id).map(|d| d as u32);
                        ModelEntry {
                            model_id,
                            provider_id: provider_id.clone(),
                            provider_name: provider_name.clone(),
                            adapter: adapter.clone(),
                            model_type: "embedding".to_string(),
                            context_length,
                            enabled: false,
                            display_name,
                            description: None,
                            pricing_prompt,
                            pricing_completion,
                            is_free,
                            max_completion_tokens: None,
                            input_modalities: vec![],
                            output_modalities: vec![],
                            supports_tools: false,
                            supports_vision: false,
                            supports_reasoning: false,
                            embedding_dimensions: dims,
                            is_stale: false,
                            baseline: None,
                            img2img_supported: None,
                            inpainting: None,
                        }
                    },
                )
                .collect::<Vec<_>>();
            (provider_id.clone(), true, parsed)
        }));
    }

    let mut all_entries: Vec<ModelEntry> = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut fetch_ok: std::collections::HashSet<String> = std::collections::HashSet::new();
    for task in tasks {
        if let Ok((provider_id, ok, rows)) = task.await {
            if ok {
                fetch_ok.insert(provider_id);
            }
            for mut entry in rows {
                let key = (entry.provider_id.clone(), entry.model_id.clone());
                entry.enabled = enabled_map
                    .get(&key)
                    .map(|(enabled, _)| *enabled)
                    .unwrap_or(false);
                seen.insert(key);
                all_entries.push(entry);
            }
        }
    }

    // Auto-disable + synthesize entries for embedding models enabled locally
    // but absent from every provider's live catalog — see the matching
    // comment in list_all_models.
    let stale: Vec<(String, String, String)> = enabled_map
        .iter()
        .filter(|((provider_id, model_id), (enabled, model_type))| {
            *enabled
                && model_type == "embedding"
                && fetch_ok.contains(provider_id)
                && !seen.contains(&(provider_id.clone(), model_id.clone()))
        })
        .map(|((provider_id, model_id), (_, model_type))| {
            (provider_id.clone(), model_id.clone(), model_type.clone())
        })
        .collect();

    if !stale.is_empty() {
        let state_guard = state.read().await;
        for (provider_id, model_id, model_type) in &stale {
            if let Err(e) = ProviderRepo::toggle_model(
                &state_guard.db,
                provider_id,
                model_id,
                model_type,
                false,
            )
            .await
            {
                tracing::warn!(
                    "Failed to auto-disable stale embedding model {} on provider {}: {}",
                    model_id,
                    provider_id,
                    e
                );
            }
        }
    }

    for (provider_id, model_id, model_type) in &stale {
        if let Some(provider) = providers.iter().find(|p| p.id.id.to_raw() == *provider_id) {
            let adapter_str = serde_json::to_value(&provider.adapter)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            all_entries.push(ModelEntry {
                model_id: model_id.clone(),
                provider_id: provider_id.clone(),
                provider_name: provider.name.clone(),
                adapter: adapter_str,
                model_type: model_type.clone(),
                context_length: None,
                enabled: false,
                display_name: None,
                description: None,
                pricing_prompt: None,
                pricing_completion: None,
                is_free: false,
                max_completion_tokens: None,
                input_modalities: vec![],
                output_modalities: vec![],
                supports_tools: false,
                supports_vision: false,
                supports_reasoning: false,
                embedding_dimensions: None,
                is_stale: true,
                baseline: None,
                img2img_supported: None,
                inpainting: None,
            });
        }
    }

    info!(
        "list_embedding_models: {} total across {} providers",
        all_entries.len(),
        providers.len()
    );
    Ok(all_entries)
}

/// Toggles a model's enabled state in the `enabled_models` table.
/// Uses SurrealDB UPSERT for clean first-time toggling.
#[tauri::command]
#[specta::specta]
pub async fn toggle_model_enabled(
    state: State<'_, Arc<RwLock<AppState>>>,
    provider_id: String,
    model_id: String,
    model_type: String,
    enabled: bool,
) -> Result<(), MythicError> {
    let state = state.read().await;
    ProviderRepo::toggle_model(&state.db, &provider_id, &model_id, &model_type, enabled).await?;
    info!(
        "Model {} on provider {} -> enabled={}",
        model_id, provider_id, enabled
    );
    Ok(())
}

/// Returns all enabled models (enabled=true only), enriched with provider name and adapter.
#[tauri::command]
#[specta::specta]
pub async fn list_enabled_models(
    state: State<'_, Arc<RwLock<AppState>>>,
    provider_id: Option<String>,
) -> Result<Vec<ModelEntry>, MythicError> {
    let state = state.read().await;
    let rows = ProviderRepo::list_enabled_models(&state.db, provider_id.as_deref()).await?;

    // Batch-fetch providers and AI Horde capability info once instead of
    // one query per row (was N+1 for both — fine at today's scale, but
    // needlessly so).
    let providers = ProviderRepo::list(&state.db, None).await?;
    let provider_map: HashMap<String, &ProviderConfig> =
        providers.iter().map(|p| (p.id.id.to_raw(), p)).collect();
    let ai_horde_info: HashMap<String, AiHordeModelInfo> = AiHordeModelRepo::list(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|info| (info.name.clone(), info))
        .collect();

    let mut entries = Vec::with_capacity(rows.len());
    for r in rows {
        if let Some(provider) = provider_map.get(&r.provider_id) {
            let adapter_str = serde_json::to_value(&provider.adapter)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();

            // Enrich AI Horde entries with cached capability info so
            // consumers (e.g. the preset editor's model dropdown) know
            // whether img2img is viable for this model without a live fetch.
            let (baseline, img2img_supported, inpainting) = if adapter_str == "ai_horde" {
                match ai_horde_info.get(&r.model_id) {
                    Some(info) => (
                        info.baseline.clone(),
                        Some(info.img2img_supported),
                        Some(info.inpainting),
                    ),
                    None => (None, None, None),
                }
            } else {
                (None, None, None)
            };

            entries.push(ModelEntry {
                model_id: r.model_id,
                provider_id: r.provider_id,
                provider_name: provider.name.clone(),
                adapter: adapter_str,
                model_type: r.model_type,
                context_length: None,
                enabled: true,
                display_name: None,
                description: None,
                pricing_prompt: None,
                pricing_completion: None,
                is_free: false,
                max_completion_tokens: None,
                input_modalities: vec![],
                output_modalities: vec![],
                supports_tools: false,
                supports_vision: false,
                supports_reasoning: false,
                embedding_dimensions: None,
                is_stale: false,
                baseline,
                img2img_supported,
                inpainting,
            });
        }
    }

    Ok(entries)
}

// ── Internal helpers ────────────────────────────────────────────────────────

fn parse_provider_type(s: &str) -> Result<ProviderType, MythicError> {
    match s {
        "llm" => Ok(ProviderType::Llm),
        "image" => Ok(ProviderType::Image),
        "video" => Ok(ProviderType::Video),
        _ => Err(MythicError::Validation(format!(
            "Invalid provider type: {}",
            s
        ))),
    }
}

fn parse_adapter(s: &str) -> Result<ProviderAdapter, MythicError> {
    match s {
        "ollama" => Ok(ProviderAdapter::Ollama),
        "open_router" => Ok(ProviderAdapter::OpenRouter),
        "openai_compatible" | "open_ai_compatible" => Ok(ProviderAdapter::OpenAiCompatible),
        "silicon_flow" => Ok(ProviderAdapter::SiliconFlow),
        "hugging_face" => Ok(ProviderAdapter::HuggingFace),
        "comfy_ui" => Ok(ProviderAdapter::ComfyUi),
        "ai_horde" => Ok(ProviderAdapter::AiHorde),
        "wan_gp" => Ok(ProviderAdapter::WanGp),
        "anthropic" => Ok(ProviderAdapter::Anthropic),
        "gemini" => Ok(ProviderAdapter::Gemini),
        "cohere" => Ok(ProviderAdapter::Cohere),
        "deepseek" => Ok(ProviderAdapter::DeepSeek),
        "groq" => Ok(ProviderAdapter::Groq),
        "perplexity" => Ok(ProviderAdapter::Perplexity),
        "xai" => Ok(ProviderAdapter::Xai),
        "hyperbolic" => Ok(ProviderAdapter::Hyperbolic),
        "moonshot" => Ok(ProviderAdapter::Moonshot),
        "together" => Ok(ProviderAdapter::Together),
        _ => Err(MythicError::Validation(format!("Invalid adapter: {}", s))),
    }
}

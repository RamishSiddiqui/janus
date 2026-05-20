use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use crate::db::providers::ProviderRepo;
use crate::error::{MythicError, validate_required_string};
use crate::models::provider::{ProviderAdapter, ProviderConfig, ProviderType};
use crate::AppState;

/// Creates a new provider configuration.
#[tauri::command]
pub async fn create_provider(
    state: State<'_, Arc<RwLock<AppState>>>,
    name: String,
    provider_type: String,
    adapter: String,
    config: serde_json::Value,
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
        config,
        is_default,
    )
    .await?;

    info!("Created provider: {} ({}) [{}]", name, adapter, provider_type);
    Ok(provider)
}

/// Retrieves a single provider by ID.
#[tauri::command]
pub async fn get_provider(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<ProviderConfig, MythicError> {
    let state = state.read().await;
    ProviderRepo::get(&state.db, &id).await
}

/// Lists all providers, optionally filtered by type.
#[tauri::command]
pub async fn list_providers(
    state: State<'_, Arc<RwLock<AppState>>>,
    provider_type: Option<String>,
) -> Result<Vec<ProviderConfig>, MythicError> {
    let state = state.read().await;
    ProviderRepo::list(&state.db, provider_type.as_deref()).await
}

/// Updates an existing provider configuration.
#[tauri::command]
pub async fn update_provider(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    name: Option<String>,
    config: Option<serde_json::Value>,
) -> Result<ProviderConfig, MythicError> {
    if let Some(ref name) = name {
        validate_required_string("Provider name", name, 100)?;
    }

    let state = state.read().await;
    let provider = ProviderRepo::update(&state.db, &id, name.as_deref(), config).await?;
    info!("Updated provider: {}", id);
    Ok(provider)
}

/// Deletes a provider configuration.
#[tauri::command]
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
#[tauri::command]
pub async fn test_provider_connection(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<bool, MythicError> {
    let state = state.read().await;
    let provider = ProviderRepo::get(&state.db, &id).await?;

    // Extract base_url from config and attempt a simple HTTP request
    let base_url = provider.config
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if base_url.is_empty() {
        // Cloud providers with API keys — check by trying to list models
        let api_key = provider.config
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if api_key.is_empty() {
            return Ok(false);
        }

        // Use a direct HTTP check for cloud providers
        return match provider.adapter {
            ProviderAdapter::OpenRouter => {
                let resp = state.http_client
                    .get("https://openrouter.ai/api/v1/models")
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("HTTP-Referer", "https://mythic.app")
                    .header("X-Title", "Mythic")
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await;
                Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
            }
            _ => Ok(!api_key.is_empty()),
        };
    }

    // Local providers — check if the base URL is reachable
    let health_url = match provider.adapter {
        ProviderAdapter::Ollama => format!("{}/api/tags", base_url),
        ProviderAdapter::ComfyUi => format!("{}/system_stats", base_url),
        _ => format!("{}/v1/models", base_url),
    };

    let resp = state.http_client
        .get(&health_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
}

/// Lists available models from a provider's API.
/// Supports Ollama (/api/tags), OpenRouter (/api/v1/models), and OpenAI-compatible (/v1/models).
#[tauri::command]
pub async fn list_provider_models(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Vec<String>, MythicError> {
    let state = state.read().await;
    let provider = ProviderRepo::get(&state.db, &id).await?;

    let base_url = provider.config
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_end_matches('/');

    let api_key = provider.config
        .get("api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let (url, is_ollama) = match provider.adapter {
        ProviderAdapter::Ollama => {
            let base = if base_url.is_empty() { "http://localhost:11434" } else { base_url };
            (format!("{}/api/tags", base), true)
        }
        ProviderAdapter::OpenRouter => {
            ("https://openrouter.ai/api/v1/models".to_string(), false)
        }
        _ => {
            // OpenAI-compatible (LM Studio, vLLM, etc.)
            if base_url.is_empty() {
                return Err(MythicError::Validation("Base URL is required to list models".to_string()));
            }
            (format!("{}/v1/models", base_url), false)
        }
    };

    let mut req = state.http_client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10));

    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let resp = req.send().await?;
    if !resp.status().is_success() {
        return Err(MythicError::Provider(format!(
            "Failed to list models: HTTP {}", resp.status()
        )));
    }

    let body: serde_json::Value = resp.json().await?;

    let models: Vec<String> = if is_ollama {
        // Ollama format: { "models": [{ "name": "gemma3:latest", ... }] }
        body.get("models")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from)).collect())
            .unwrap_or_default()
    } else {
        // OpenAI/OpenRouter format: { "data": [{ "id": "gpt-4o", ... }] }
        body.get("data")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|m| m.get("id").and_then(|n| n.as_str()).map(String::from)).collect())
            .unwrap_or_default()
    };

    info!("Listed {} models from provider {}", models.len(), provider.name);
    Ok(models)
}

// ── Model enable/disable tracking ──────────────────────────────────────────

/// A single model entry returned by `list_all_models`.
#[derive(serde::Serialize, Debug, Clone)]
pub struct ModelEntry {
    pub model_id:      String,
    pub provider_id:   String,
    pub provider_name: String,
    pub adapter:       String,
    pub model_type:    String,
    pub context_length: Option<u32>,
    pub enabled:       bool,
}

/// Fetches models from ALL configured providers in parallel and merges them
/// with their enabled/disabled state from the `enabled_models` table.
///
/// Per-provider fetches time out after 8 seconds. Partial results are returned
/// on timeout or network error rather than failing the whole call.
#[tauri::command]
pub async fn list_all_models(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<ModelEntry>, MythicError> {
    // 1. Fetch all providers and enabled states while holding the lock
    let (providers, enabled_map, http) = {
        let state_guard = state.read().await;
        let providers = ProviderRepo::list(&state_guard.db, None).await?;
        let enabled_map = ProviderRepo::get_all_enabled_states(&state_guard.db).await?;
        let http = state_guard.http_client.clone();
        (providers, enabled_map, http)
    };

    // 2. Spawn HTTP tasks per provider (only needs http_client + config data, no db)
    let mut tasks = Vec::new();
    for provider in &providers {
        let config = &provider.config;
        let base_url = config.get("base_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let api_key = config.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
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

        tasks.push(tokio::spawn(async move {
            let (url, is_ollama) = match adapter.as_str() {
                "ollama" => {
                    let base = if base_url.is_empty() { "http://localhost:11434".to_string() } else { base_url };
                    (format!("{}/api/tags", base), true)
                }
                "open_router" => ("https://openrouter.ai/api/v1/models".to_string(), false),
                _ => {
                    if base_url.is_empty() { return vec![]; }
                    (format!("{}/v1/models", base_url.trim_end_matches('/')), false)
                }
            };

            let mut req = http_c.get(&url).timeout(std::time::Duration::from_secs(8));
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key));
                if adapter == "open_router" {
                    req = req
                        .header("HTTP-Referer", "https://mythic.app")
                        .header("X-Title", "Mythic");
                }
            }

            let body: serde_json::Value = match req.send().await {
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(v) => v,
                    Err(_) => return vec![],
                },
                Err(_) => return vec![],
            };

            let entries: Vec<(String, Option<u32>)> = if is_ollama {
                body.get("models")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|m| {
                        Some((m.get("name")?.as_str()?.to_string(), None))
                    }).collect())
                    .unwrap_or_default()
            } else {
                body.get("data")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|m| {
                        let id = m.get("id")?.as_str()?.to_string();
                        let ctx = m.get("context_length").and_then(|v| v.as_u64()).map(|v| v as u32);
                        Some((id, ctx))
                    }).collect())
                    .unwrap_or_default()
            };

            entries.into_iter().map(|(model_id, context_length)| {
                (provider_id.clone(), provider_name.clone(), adapter.clone(), provider_type.clone(), model_id, context_length)
            }).collect::<Vec<_>>()
        }));
    }

    // 3. Collect results and merge with enabled states
    let mut all_entries: Vec<ModelEntry> = Vec::new();
    for task in tasks {
        if let Ok(rows) = task.await {
            for (provider_id, provider_name, adapter, model_type, model_id, context_length) in rows {
                let enabled = *enabled_map.get(&(provider_id.clone(), model_id.clone())).unwrap_or(&false);
                all_entries.push(ModelEntry {
                    model_id,
                    provider_id,
                    provider_name,
                    adapter,
                    model_type,
                    context_length,
                    enabled,
                });
            }
        }
    }

    info!("list_all_models: {} total models across {} providers", all_entries.len(), providers.len());
    Ok(all_entries)
}

/// Toggles a model's enabled state in the `enabled_models` table.
/// Uses SurrealDB UPSERT for clean first-time toggling.
#[tauri::command]
pub async fn toggle_model_enabled(
    state: State<'_, Arc<RwLock<AppState>>>,
    provider_id: String,
    model_id: String,
    model_type: String,
    enabled: bool,
) -> Result<(), MythicError> {
    let state = state.read().await;
    ProviderRepo::toggle_model(&state.db, &provider_id, &model_id, &model_type, enabled).await?;
    info!("Model {} on provider {} -> enabled={}", model_id, provider_id, enabled);
    Ok(())
}

/// Returns all enabled models (enabled=true only), enriched with provider name and adapter.
#[tauri::command]
pub async fn list_enabled_models(
    state: State<'_, Arc<RwLock<AppState>>>,
    provider_id: Option<String>,
) -> Result<Vec<ModelEntry>, MythicError> {
    let state = state.read().await;
    let rows = ProviderRepo::list_enabled_models(&state.db, provider_id.as_deref()).await?;

    // Enrich with provider name and adapter by fetching each provider
    let mut entries = Vec::with_capacity(rows.len());
    for r in rows {
        if let Ok(provider) = ProviderRepo::get(&state.db, &r.provider_id).await {
            let adapter_str = serde_json::to_value(&provider.adapter)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            entries.push(ModelEntry {
                model_id: r.model_id,
                provider_id: r.provider_id,
                provider_name: provider.name,
                adapter: adapter_str,
                model_type: r.model_type,
                context_length: None,
                enabled: true,
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
        _ => Err(MythicError::Validation(format!("Invalid provider type: {}", s))),
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

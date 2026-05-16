use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

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

    let state = state.read().await;
    let id = Uuid::new_v4().to_string();

    let _ptype = parse_provider_type(&provider_type)?;
    let _padapter = parse_adapter(&adapter)?;
    let is_default = is_default.unwrap_or(false);

    // If this is set as default, unset any existing default for this type
    if is_default {
        sqlx::query(
            "UPDATE provider_configs SET is_default = 0 WHERE provider_type = ?"
        )
        .bind(&provider_type)
        .execute(&state.db)
        .await?;
    }

    let config_str = serde_json::to_string(&config)?;

    sqlx::query(
        "INSERT INTO provider_configs (id, name, provider_type, adapter, config, is_default)
         VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&name)
    .bind(&provider_type)
    .bind(&adapter)
    .bind(&config_str)
    .bind(is_default)
    .execute(&state.db)
    .await?;

    info!("Created provider: {} ({}) [{}]", name, adapter, provider_type);
    get_provider_by_id(&state.db, &id).await
}

/// Retrieves a single provider by ID.
#[tauri::command]
pub async fn get_provider(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<ProviderConfig, MythicError> {
    let state = state.read().await;
    get_provider_by_id(&state.db, &id).await
}

/// Lists all providers, optionally filtered by type.
#[tauri::command]
pub async fn list_providers(
    state: State<'_, Arc<RwLock<AppState>>>,
    provider_type: Option<String>,
) -> Result<Vec<ProviderConfig>, MythicError> {
    let state = state.read().await;

    let rows = if let Some(ref ptype) = provider_type {
        sqlx::query_as::<_, ProviderRow>(
            "SELECT id, name, provider_type, adapter, config, is_default
             FROM provider_configs WHERE provider_type = ? ORDER BY is_default DESC, name ASC"
        )
        .bind(ptype)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, ProviderRow>(
            "SELECT id, name, provider_type, adapter, config, is_default
             FROM provider_configs ORDER BY provider_type, is_default DESC, name ASC"
        )
        .fetch_all(&state.db)
        .await?
    };

    Ok(rows.into_iter().filter_map(|r| r.try_into().ok()).collect())
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

    // Verify exists
    get_provider_by_id(&state.db, &id).await?;

    if let Some(ref name) = name {
        sqlx::query("UPDATE provider_configs SET name = ? WHERE id = ?")
            .bind(name)
            .bind(&id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref config) = config {
        let config_str = serde_json::to_string(config)?;
        sqlx::query("UPDATE provider_configs SET config = ? WHERE id = ?")
            .bind(&config_str)
            .bind(&id)
            .execute(&state.db)
            .await?;
    }

    info!("Updated provider: {}", id);
    get_provider_by_id(&state.db, &id).await
}

/// Deletes a provider configuration.
#[tauri::command]
pub async fn delete_provider(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;

    let result = sqlx::query("DELETE FROM provider_configs WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MythicError::NotFound(format!("Provider not found: {}", id)));
    }

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

    let provider = get_provider_by_id(&state.db, &id).await?;
    let ptype = match provider.provider_type {
        ProviderType::Llm => "llm",
        ProviderType::Image => "image",
        ProviderType::Video => "video",
    };

    // Unset all defaults for this type
    sqlx::query("UPDATE provider_configs SET is_default = 0 WHERE provider_type = ?")
        .bind(ptype)
        .execute(&state.db)
        .await?;

    // Set this one as default
    sqlx::query("UPDATE provider_configs SET is_default = 1 WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    info!("Set default {} provider: {}", ptype, id);
    Ok(())
}

/// Tests connectivity to a provider by attempting a health check.
#[tauri::command]
pub async fn test_provider_connection(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<bool, MythicError> {
    let state = state.read().await;
    let provider = get_provider_by_id(&state.db, &id).await?;

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

        // For OpenRouter, check the models endpoint
        let url = match provider.adapter {
            ProviderAdapter::OpenRouter => "https://openrouter.ai/api/v1/models".to_string(),
            _ => return Ok(!api_key.is_empty()),
        };

        let resp = state.http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await;

        return Ok(resp.map(|r| r.status().is_success()).unwrap_or(false));
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
    let provider = get_provider_by_id(&state.db, &id).await?;

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

// --- Internal helpers ---

#[derive(sqlx::FromRow)]
struct ProviderRow {
    id: String,
    name: String,
    provider_type: String,
    adapter: String,
    config: String,
    is_default: bool,
}

impl TryFrom<ProviderRow> for ProviderConfig {
    type Error = MythicError;

    fn try_from(row: ProviderRow) -> Result<Self, Self::Error> {
        Ok(ProviderConfig {
            id: row.id,
            name: row.name,
            provider_type: parse_provider_type(&row.provider_type)?,
            adapter: parse_adapter(&row.adapter)?,
            config: serde_json::from_str(&row.config)?,
            is_default: row.is_default,
        })
    }
}

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
        _ => Err(MythicError::Validation(format!("Invalid adapter: {}", s))),
    }
}

async fn get_provider_by_id(
    db: &sqlx::Pool<sqlx::Sqlite>,
    id: &str,
) -> Result<ProviderConfig, MythicError> {
    let row = sqlx::query_as::<_, ProviderRow>(
        "SELECT id, name, provider_type, adapter, config, is_default
         FROM provider_configs WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| MythicError::NotFound(format!("Provider not found: {}", id)))?;

    row.try_into()
}

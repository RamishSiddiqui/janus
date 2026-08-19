//! Resolving which LLM provider/model a call should use, and building a
//! `RigProvider` from stored config — generic logic used across chat,
//! NPC detection, lorebook, and memory-extraction call sites, not specific
//! to any one of them (hence living here rather than in `commands::chat`).

use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::db::providers::ProviderRepo;
use crate::error::MythicError;
use crate::models::provider::{ProviderAdapter, ProviderConfig};
use crate::providers::unified::RigProvider;

/// Finds the default LLM provider configuration from the database.
pub(crate) async fn get_default_llm_provider(
    db: &Surreal<Db>,
) -> Result<ProviderConfig, MythicError> {
    match ProviderRepo::get_default(db, "llm").await? {
        Some(config) => Ok(config),
        None => Err(MythicError::Config(
            "No LLM provider configured. Add one in Settings → Models.".to_string(),
        )),
    }
}

/// Resolves the model ID to use, falling back through stored config then enabled models.
pub(crate) async fn resolve_model_id(
    model: Option<String>,
    provider_config: &ProviderConfig,
    db: &Surreal<Db>,
) -> Result<String, MythicError> {
    match model {
        Some(m) if !m.is_empty() && m != "unknown" => Ok(m),
        _ => {
            let stored = provider_config
                .config
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !stored.is_empty() && stored != "unknown" {
                Ok(stored.to_string())
            } else {
                // Fall back to first enabled LLM model for this provider
                // (explicitly exclude embedding models)
                let provider_id_str =
                    crate::db::value_bridge::record_id_to_string(&provider_config.id);
                let enabled = ProviderRepo::list_enabled_models(db, Some(&provider_id_str)).await?;
                match enabled.into_iter().find(|m| m.model_type != "embedding") {
                    Some(m) => Ok(m.model_id),
                    None => Err(MythicError::Config(
                        "No chat model selected. Go to LLM Models, enable at least one non-embedding model.".to_string()
                    )),
                }
            }
        }
    }
}

/// Creates a unified rig-backed LLM provider from DB config.
pub(crate) fn create_rig_provider(config: &ProviderConfig) -> Result<RigProvider, MythicError> {
    // Convert enum to string for RigProvider::from_config
    let adapter_str = match config.adapter {
        ProviderAdapter::Ollama => "ollama",
        ProviderAdapter::OpenRouter => "openrouter",
        ProviderAdapter::OpenAiCompatible => "openai",
        ProviderAdapter::SiliconFlow => "openai", // OpenAI-compatible
        ProviderAdapter::HuggingFace => "huggingface",
        ProviderAdapter::Anthropic => "anthropic",
        ProviderAdapter::Gemini => "gemini",
        ProviderAdapter::Cohere => "cohere",
        ProviderAdapter::DeepSeek => "deepseek",
        ProviderAdapter::Groq => "groq",
        ProviderAdapter::Perplexity => "perplexity",
        ProviderAdapter::Xai => "xai",
        ProviderAdapter::Hyperbolic => "hyperbolic",
        ProviderAdapter::Moonshot => "moonshot",
        ProviderAdapter::Together => "together",
        ProviderAdapter::ComfyUi => {
            return Err(MythicError::Config(
                "ComfyUI is an image provider, not an LLM provider".to_string(),
            ))
        }
        ProviderAdapter::AiHorde => {
            return Err(MythicError::Config(
                "AI Horde is an image provider, not an LLM provider".to_string(),
            ))
        }
        ProviderAdapter::WanGp => {
            return Err(MythicError::Config(
                "WanGP is an image/video provider, not an LLM provider".to_string(),
            ))
        }
    };

    let api_key = config.config.get("api_key").and_then(|v| v.as_str());
    let base_url = config.config.get("base_url").and_then(|v| v.as_str());

    RigProvider::from_config(adapter_str, api_key, base_url)
}

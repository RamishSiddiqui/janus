//! Ollama provider — local LLM inference.
//!
//! Ollama exposes an OpenAI-compatible `/v1/chat/completions` endpoint,
//! so we reuse the shared client. The only differences are:
//! - Default base URL: `http://localhost:11434/v1`
//! - No auth required
//! - Model listing uses Ollama's native `/api/tags` endpoint for richer metadata

use async_trait::async_trait;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, GenerationParams};
use crate::models::provider::ModelInfo;
use crate::providers::openai_client::{OpenAiClient, OpenAiClientConfig};
use crate::providers::traits::{LlmProvider, StreamChunk};

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

pub struct OllamaProvider {
    client: OpenAiClient,
    native_base_url: String,
    http: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(http: reqwest::Client, base_url: Option<&str>) -> Self {
        let base = base_url.unwrap_or(DEFAULT_OLLAMA_URL);

        let config = OpenAiClientConfig {
            // Ollama's OpenAI-compatible endpoint
            base_url: format!("{}/v1", base),
            headers: HeaderMap::new(),
            default_model: None,
        };

        Self {
            client: OpenAiClient::new(http.clone(), config),
            native_base_url: base.to_string(),
            http,
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "Ollama"
    }

    /// Uses Ollama's native `/api/tags` endpoint for richer model info.
    async fn list_models(&self) -> Result<Vec<ModelInfo>, MythicError> {
        let url = format!("{}/api/tags", self.native_base_url);

        let resp = self.http
            .get(&url)
            .send()
            .await?;

        if !resp.status().is_success() {
            // Fall back to OpenAI-compatible endpoint
            return self.client.list_models().await;
        }

        let body: OllamaTagsResponse = resp.json().await?;

        Ok(body.models.into_iter().map(|m| ModelInfo {
            id: m.name.clone(),
            name: m.name,
            context_length: None,
            metadata: Some(serde_json::json!({
                "size": m.size,
                "family": m.details.as_ref().map(|d| d.family.as_str()).unwrap_or("unknown"),
                "parameter_size": m.details.as_ref().map(|d| d.parameter_size.as_str()).unwrap_or("unknown"),
            })),
        }).collect())
    }

    async fn generate(
        &self,
        model: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
    ) -> Result<String, MythicError> {
        self.client.generate(model, messages, params).await
    }

    async fn generate_stream(
        &self,
        model: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), MythicError> {
        self.client.generate_stream(model, messages, params, tx).await
    }

    async fn health_check(&self) -> Result<bool, MythicError> {
        let url = format!("{}/api/tags", self.native_base_url);

        let resp = self.http
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await;

        Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
    }
}

// --- Ollama-specific API types ---

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    size: u64,
    details: Option<OllamaModelDetails>,
}

#[derive(Deserialize)]
struct OllamaModelDetails {
    #[serde(default)]
    family: String,
    #[serde(default)]
    parameter_size: String,
}

//! OpenRouter provider — cloud LLM aggregator.
//!
//! Wraps the shared OpenAI-compatible client with OpenRouter-specific
//! configuration: API key auth, custom HTTP-Referer header, and the
//! OpenRouter base URL.

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use tokio::sync::mpsc;

use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, GenerationParams};
use crate::models::provider::ModelInfo;
use crate::providers::openai_client::{OpenAiClient, OpenAiClientConfig};
use crate::providers::traits::{LlmProvider, StreamChunk};

const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

pub struct OpenRouterProvider {
    client: OpenAiClient,
}

impl OpenRouterProvider {
    pub fn new(http: reqwest::Client, api_key: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))
                .expect("Invalid API key format"),
        );
        // OpenRouter recommends setting these for ranking/analytics
        headers.insert(
            "HTTP-Referer",
            HeaderValue::from_static("https://mythic.app"),
        );
        headers.insert(
            "X-Title",
            HeaderValue::from_static("Mythic"),
        );

        let config = OpenAiClientConfig {
            base_url: OPENROUTER_BASE_URL.to_string(),
            headers,
            default_model: None,
        };

        Self {
            client: OpenAiClient::new(http, config),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenRouterProvider {
    fn name(&self) -> &str {
        "OpenRouter"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, MythicError> {
        self.client.list_models().await
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
        self.client.health_check().await
    }
}

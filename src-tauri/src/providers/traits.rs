use async_trait::async_trait;

use crate::error::MythicError;
use crate::models::provider::{ImageGenParams, ModelInfo};

/// A streaming chunk emitted during text generation.
///
/// Sent via a channel so the frontend can display tokens in real-time.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// A text delta (partial token)
    Delta(String),

    /// Generation is complete — includes the full assembled response
    Done(String),

    /// An error occurred during generation
    Error(String),
}

/// Result of an image generation request.
#[derive(Debug, Clone)]
pub struct ImageResult {
    /// Raw image bytes (PNG format)
    pub data: Vec<u8>,

    /// The prompt that was used (may differ from input due to enrichment)
    pub prompt_used: String,

    /// The seed used for generation (for reproducibility)
    pub seed: Option<u64>,
}

/// Trait for image generation providers.
///
/// Implementations include: SiliconFlow (cloud) and ComfyUI (local).
#[async_trait]
pub trait ImageProvider: Send + Sync {
    /// Returns a human-readable provider name.
    fn name(&self) -> &str;

    /// Lists available image models.
    async fn list_models(&self) -> Result<Vec<ModelInfo>, MythicError>;

    /// Generates an image from a text prompt.
    async fn generate_image(
        &self,
        model: &str,
        params: &ImageGenParams,
    ) -> Result<ImageResult, MythicError>;

    /// Checks if the provider is reachable and configured correctly.
    async fn health_check(&self) -> Result<bool, MythicError>;
}

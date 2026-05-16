use serde::{Deserialize, Serialize};

/// The type of AI capability a provider offers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    /// Text generation / chat completion
    Llm,
    /// Image generation
    Image,
    /// Video generation
    Video,
}

/// The specific adapter implementation for a provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAdapter {
    /// Local Ollama instance
    Ollama,
    /// OpenRouter API aggregator
    OpenRouter,
    /// Generic OpenAI-compatible API (works with LM Studio, KoboldCPP, vLLM, etc.)
    OpenAiCompatible,
    /// SiliconFlow API (LLM + Image + Video)
    SiliconFlow,
    /// Hugging Face Inference API
    HuggingFace,
    /// Local ComfyUI instance for image/video generation
    ComfyUi,
}

/// Configuration for a specific AI provider connection.
///
/// Stored in the database and used to initialize provider instances
/// at runtime. The `config` field holds adapter-specific JSON settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,

    /// Human-readable name (e.g., "My Local Ollama", "OpenRouter Free")
    pub name: String,

    /// What this provider does
    pub provider_type: ProviderType,

    /// Which adapter implementation to use
    pub adapter: ProviderAdapter,

    /// Adapter-specific configuration as JSON.
    ///
    /// For Ollama: `{ "base_url": "http://localhost:11434" }`
    /// For OpenRouter: `{ "api_key": "sk-...", "model": "meta-llama/llama-4-maverick" }`
    /// For OpenAI-compat: `{ "base_url": "...", "api_key": "...", "model": "..." }`
    /// For ComfyUI: `{ "base_url": "http://localhost:8188", "workflow": "..." }`
    pub config: serde_json::Value,

    /// Whether this is the default provider for its type
    pub is_default: bool,
}

/// Metadata about a model available from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "llama3.2:8b", "meta-llama/llama-4-maverick")
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Context window size in tokens (if known)
    pub context_length: Option<u32>,

    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Parameters for image generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenParams {
    /// The text prompt describing the desired image
    pub prompt: String,

    /// Negative prompt — things to avoid in the image
    #[serde(default)]
    pub negative_prompt: String,

    /// Image width in pixels
    #[serde(default = "default_image_width")]
    pub width: u32,

    /// Image height in pixels
    #[serde(default = "default_image_height")]
    pub height: u32,

    /// Number of inference steps
    #[serde(default = "default_steps")]
    pub steps: u32,

    /// Classifier-free guidance scale
    #[serde(default = "default_guidance")]
    pub guidance_scale: f32,

    /// Random seed (None = random)
    pub seed: Option<u64>,
}

impl Default for ImageGenParams {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            negative_prompt: String::new(),
            width: default_image_width(),
            height: default_image_height(),
            steps: default_steps(),
            guidance_scale: default_guidance(),
            seed: None,
        }
    }
}

fn default_image_width() -> u32 {
    1024
}

fn default_image_height() -> u32 {
    1024
}

fn default_steps() -> u32 {
    20
}

fn default_guidance() -> f32 {
    7.5
}

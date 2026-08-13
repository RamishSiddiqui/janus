use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;

/// The type of AI capability a provider offers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Type)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Type)]
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
    /// AI Horde (formerly Stable Horde) — free, crowdsourced, asynchronous
    /// image generation cluster. Works anonymously (no signup) via the
    /// well-known "0000000000" API key, though registered keys get queue
    /// priority through the kudos system.
    AiHorde,
    /// Anthropic API (Claude models)
    Anthropic,
    /// Google Gemini API
    Gemini,
    /// Cohere API (Command R+)
    Cohere,
    /// DeepSeek API
    DeepSeek,
    /// Groq API (fast inference)
    Groq,
    /// Perplexity API (search-augmented)
    Perplexity,
    /// xAI API (Grok models)
    Xai,
    /// Hyperbolic API
    Hyperbolic,
    /// Moonshot API (Kimi)
    Moonshot,
    /// Together API (open-source models)
    Together,
}

/// Configuration for a specific AI provider connection.
///
/// Stored in the database and used to initialize provider instances
/// at runtime. The `config` field holds adapter-specific JSON settings.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProviderConfig {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,

    /// Human-readable name (e.g., "My Local Ollama", "OpenRouter Free")
    pub name: String,

    /// What this provider does
    pub provider_type: ProviderType,

    /// Which adapter implementation to use
    pub adapter: ProviderAdapter,

    /// Adapter-specific configuration as native JSON.
    ///
    /// For Ollama: `{ "base_url": "http://localhost:11434" }`
    /// For OpenRouter: `{ "api_key": "sk-...", "model": "meta-llama/llama-4-maverick" }`
    /// For OpenAI-compat: `{ "base_url": "...", "api_key": "...", "model": "..." }`
    /// For ComfyUI: `{ "base_url": "http://localhost:8188", "workflow": "..." }` — `workflow`
    /// is the user's own exported (API-format) workflow JSON, stored as a raw string and
    /// parsed at generation time. It may contain `{{POSITIVE_PROMPT}}`, `{{NEGATIVE_PROMPT}}`,
    /// `{{SEED}}`, `{{WIDTH}}`, `{{HEIGHT}}`, and `{{CHARACTER_IMAGE_1..N}}` placeholder tokens —
    /// see `providers::comfyui::substitute_placeholders`.
    #[specta(type = crate::models::JsonValue)]
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

    /// AI Horde's `nsfw` request flag — NOT "make this explicit," but "don't
    /// route to/enforce the safety filter that blindly censors anything a
    /// worker's classifier flags as NSFW-ish." With this false, ordinary
    /// non-explicit character descriptions (e.g. a card's own physical
    /// description) can trip an overzealous classifier and come back as a
    /// black "CENSORED" placeholder instead of the actual image. Sourced
    /// from the user's own comfort-level setting, not hardcoded.
    #[serde(default)]
    pub allow_nsfw: bool,
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
            allow_nsfw: false,
        }
    }
}

/// A single cast member's portrait, handed to a ComfyUI generation so it can
/// be uploaded and substituted into whichever `{{CHARACTER_IMAGE_n}}` token(s)
/// the user's workflow references — see `providers::comfyui`. `relative_path`
/// is relative to `app_data_dir` (e.g. a character's own `avatar_path`), the
/// same convention avatars/portraits already use everywhere else.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CharacterImageRef {
    pub character_id: String,
    pub character_name: String,
    pub relative_path: String,
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

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
    /// Local WanGP (Wan2GP) instance for image/video generation, reached over
    /// its MCP server (streamable-http transport) — see `providers::wangp`.
    /// Built for weaker hardware than ComfyUI typically needs.
    WanGp,
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
    /// For WanGP: `{ "base_url": "http://127.0.0.1:7866", "model": "qwen_image_20B" }` —
    /// `base_url` points at the WanGP MCP server (`/mcp` is appended at connect time). `model`
    /// is the same generic "Default Model" field every non-LLM adapter uses; WanGP treats
    /// whatever's in it as one of its own model identifiers (e.g. `qwen_image_20B` for images,
    /// `ltx2_22B_distilled` for video). A single WanGP instance serving both image and video
    /// needs two separate `ProviderConfig` rows (one per `provider_type`) at the same
    /// `base_url` — see `providers::wangp`.
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

/// Parameters for video generation — same shape/intent as `ImageGenParams`,
/// kept as its own struct (rather than reusing `ImageGenParams` with optional
/// video fields bolted on) since the two media types don't share `steps`/
/// `guidance_scale` vs `duration_seconds`/`fps` in any meaningful way.
/// Provider-agnostic even though WanGP is the only implementor today.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoGenParams {
    pub prompt: String,
    #[serde(default)]
    pub negative_prompt: String,
    #[serde(default = "default_video_width")]
    pub width: u32,
    #[serde(default = "default_video_height")]
    pub height: u32,
    /// Clip length in seconds. Converted to whatever frame-count unit a
    /// given adapter needs (WanGP's `video_length` is frames) internally.
    #[serde(default = "default_duration_seconds")]
    pub duration_seconds: f32,
    #[serde(default = "default_fps")]
    pub fps: u32,
    pub seed: Option<u64>,
    /// See `ImageGenParams::allow_nsfw` — same meaning, same source (the
    /// user's own comfort-level setting).
    #[serde(default)]
    pub allow_nsfw: bool,
}

impl Default for VideoGenParams {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            negative_prompt: String::new(),
            width: default_video_width(),
            height: default_video_height(),
            duration_seconds: default_duration_seconds(),
            fps: default_fps(),
            seed: None,
            allow_nsfw: false,
        }
    }
}

fn default_video_width() -> u32 {
    1280
}

fn default_video_height() -> u32 {
    720
}

fn default_duration_seconds() -> f32 {
    4.0
}

fn default_fps() -> u32 {
    24
}

/// A single cast member's portrait, handed to a ComfyUI or WanGP generation
/// so it can be attached as a reference image. ComfyUI substitutes these into
/// whichever `{{CHARACTER_IMAGE_n}}` token(s) the user's workflow references
/// (`providers::comfyui`); WanGP attaches them under whatever field its own
/// model schema reports for reference images (`providers::wangp`).
/// `relative_path` is relative to `app_data_dir` (e.g. a character's own
/// `avatar_path`), the same convention avatars/portraits already use
/// everywhere else.
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

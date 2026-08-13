use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;

/// A reusable image-generation style bundle (sampler/cfg/steps/karras,
/// optional AI Horde named style, optional negative prompt override).
/// Selectable per-conversation, or applied globally via `is_default`, so
/// different chats can use different visual styles without editing the
/// image provider's own connection config.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ImagePreset {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,
    pub name: String,
    pub model: Option<String>,
    pub sampler_name: String,
    pub cfg_scale: f64,
    pub steps: u32,
    pub karras: bool,
    /// A named/shared AI Horde style (see aihorde.net styles) — when set,
    /// overrides model/sampler/cfg_scale/steps/karras/negative_prompt above
    /// entirely at generation time.
    pub style: Option<String>,
    pub negative_prompt: Option<String>,
    /// CLIP layers to skip (1-12). Most anime/illustration checkpoints
    /// (Pony Diffusion V6 XL, AAM XL AnimeMix) are trained expecting 2;
    /// `None` lets AI Horde apply its own default (1).
    #[serde(default)]
    pub clip_skip: Option<u32>,
    /// AI Horde post-processors to run, in order — face-fixers (GFPGAN,
    /// CodeFormers) and/or upscalers (RealESRGAN variants, 4x_AnimeSharp).
    #[serde(default)]
    pub post_processing: Vec<String>,
    /// Re-processes the image at a higher resolution after the base
    /// generation — the single biggest lever for fixing composition/anatomy
    /// errors, at the cost of roughly doubling generation time and kudos.
    #[serde(default)]
    pub hires_fix: bool,
    #[serde(default)]
    pub hires_fix_denoising_strength: Option<f64>,
    pub is_default: bool,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub created_at: String,
}

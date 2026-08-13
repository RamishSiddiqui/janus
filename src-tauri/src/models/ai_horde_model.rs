use serde::{Deserialize, Serialize};
use specta::Type;

/// Cached capability metadata for an AI Horde image model — merged from the
/// live worker-availability endpoint and the static Haidra-Org model
/// reference. `img2img_supported` is a heuristic derived from `baseline`:
/// reliable on SD1.x/SD2/SDXL checkpoints, unreliable/unsupported on newer
/// architectures (Flux img2img is documented as producing blurred/
/// oversaturated results; Stable Cascade originally launched text2img-only).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AiHordeModelInfo {
    pub name: String,
    pub baseline: Option<String>,
    pub inpainting: bool,
    pub nsfw: bool,
    pub style: Option<String>,
    pub img2img_supported: bool,
    pub worker_count: i64,
}

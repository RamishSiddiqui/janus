//! Native Kokoro-82M text-to-speech (ONNX Runtime, embedded in-process —
//! unlike the ComfyUI/WanGP image/video providers, this does not run as an
//! external server). Kokoro is small enough (int8 ONNX ~90MB, CPU RTF
//! ~0.5) that embedding it directly is worth more than staying consistent
//! with the heavier image/video adapters' external-process shape — see
//! GitHub issue #66 for the full reasoning.
//!
//! Model + voice pack are never bundled in the installer; they download on
//! first use into `app_data_dir/tts/` (see `download`).

pub mod chunker;
pub mod download;
pub mod engine;
pub mod vocab;

pub use engine::{KokoroEngine, VoiceInfo};

//! In-process Kokoro-82M ONNX inference session + voice pack.
//!
//! Loading (`KokoroEngine::load`) does real file I/O and ONNX graph
//! parsing — expected to take real time. Callers must build this once and
//! cache it (see `AppState::tts_engine`), never per-synthesis-call.

use std::io::Cursor;
use std::path::Path;

use ndarray::{Array1, Array2};
use ort::session::Session;
use ort::value::TensorRef;

use crate::error::MythicError;
use crate::tts::vocab::phonemes_to_tokens;

const SAMPLE_RATE: u32 = 24_000;
/// Kokoro's voice style tensors are indexed by phoneme-token count, capped
/// at this many rows — matches the (510, 1, 256) shape observed for every
/// voice in `voices-v1.0.bin`.
const MAX_STYLE_ROWS: usize = 510;

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
}

/// Per-voice style matrices loaded from `voices-v1.0.bin` — a real `.npz`
/// (zip-of-`.npy`) archive, one (510, 1, 256) array per voice, squeezed to
/// (510, 256) on load so a token-length lookup is a plain row index.
struct VoicePack {
    voices: std::collections::HashMap<String, Array2<f32>>,
}

impl VoicePack {
    fn load(path: &Path) -> Result<Self, MythicError> {
        let file = std::fs::File::open(path)?;
        let mut npz = ndarray_npy::NpzReader::new(file)
            .map_err(|e| MythicError::Provider(format!("Failed to open voice pack: {}", e)))?;

        let names = npz
            .names()
            .map_err(|e| MythicError::Provider(format!("Failed to list voice pack entries: {}", e)))?;

        let mut voices = std::collections::HashMap::new();
        for name in names {
            let arr: ndarray::ArrayD<f32> = npz.by_name(&name).map_err(|e| {
                MythicError::Provider(format!("Failed to read voice '{}': {}", name, e))
            })?;
            let rows = *arr.shape().first().unwrap_or(&1);
            let flat: Vec<f32> = arr.iter().copied().collect();
            let arr2 = Array2::from_shape_vec((rows, 256), flat).map_err(|e| {
                MythicError::Provider(format!(
                    "Unexpected voice tensor shape for '{}' (expected (N, 256)): {}",
                    name, e
                ))
            })?;
            let voice_id = name.trim_end_matches(".npy").to_string();
            voices.insert(voice_id, arr2);
        }

        if voices.is_empty() {
            return Err(MythicError::Provider(
                "Voice pack contained no usable voices".to_string(),
            ));
        }

        Ok(Self { voices })
    }

    fn style_for(&self, voice_id: &str, token_len: usize) -> Result<Array1<f32>, MythicError> {
        let mat = self
            .voices
            .get(voice_id)
            .ok_or_else(|| MythicError::NotFound(format!("Unknown TTS voice '{}'", voice_id)))?;
        let max_row = mat.nrows().saturating_sub(1).min(MAX_STYLE_ROWS.saturating_sub(1));
        let row = token_len.min(max_row);
        Ok(mat.row(row).to_owned())
    }

    fn list(&self) -> Vec<VoiceInfo> {
        let mut voices: Vec<VoiceInfo> = self
            .voices
            .keys()
            .map(|id| VoiceInfo {
                id: id.clone(),
                name: display_name(id),
            })
            .collect();
        voices.sort_by(|a, b| a.name.cmp(&b.name));
        voices
    }
}

/// Turns a voice id like `af_heart` into a friendlier display name
/// (`Heart (US, Female)`) — Kokoro's own naming convention prefixes each id
/// with a two-letter locale+gender code (`af_`/`am_`/`bf_`/`bm_`, etc.).
fn display_name(voice_id: &str) -> String {
    let Some(underscore) = voice_id.find('_') else {
        return voice_id.to_string();
    };
    let prefix = &voice_id[..underscore];
    let rest = &voice_id[underscore + 1..];

    let region = match prefix.chars().next() {
        Some('a') => "US",
        Some('b') => "UK",
        _ => "",
    };
    let gender = match prefix.chars().nth(1) {
        Some('f') => "Female",
        Some('m') => "Male",
        _ => "",
    };
    let mut name = rest.replace('_', " ");
    if let Some(first) = name.get_mut(0..1) {
        first.make_ascii_uppercase();
    }

    if region.is_empty() {
        name
    } else {
        format!("{} ({}, {})", name, region, gender)
    }
}

pub struct KokoroEngine {
    session: Session,
    voices: VoicePack,
}

impl KokoroEngine {
    pub fn load(model_path: &Path, voices_path: &Path) -> Result<Self, MythicError> {
        // `commit_from_file` isn't available in this build (only
        // `commit_from_memory` is, per the `ort` 2.0.0-rc.13 API actually
        // compiled against here) — read the ~90MB model into memory
        // ourselves and commit from bytes instead. One-time cost at engine
        // load, not per synthesis call.
        let model_bytes = std::fs::read(model_path)?;
        let session = Session::builder()
            .map_err(|e| {
                MythicError::Provider(format!("Failed to create ONNX session builder: {}", e))
            })?
            .commit_from_memory(&model_bytes)
            .map_err(|e| MythicError::Provider(format!("Failed to load Kokoro ONNX model: {}", e)))?;
        let voices = VoicePack::load(voices_path)?;
        Ok(Self { session, voices })
    }

    pub fn list_voices(&self) -> Vec<VoiceInfo> {
        self.voices.list()
    }

    /// Synthesizes `text` in `voice_id`'s voice at `speed` (1.0 = normal),
    /// returning WAV-encoded bytes at Kokoro's native 24kHz mono output —
    /// no resampling.
    ///
    /// `&mut self`: `ort::session::Session::run` requires mutable access
    /// (it's not a read-only call despite inference conceptually being
    /// one) — the caller holds the engine behind a `tokio::sync::Mutex`,
    /// so exclusive access per call is already the actual concurrency
    /// model, not a new constraint this introduces.
    pub fn synthesize(
        &mut self,
        text: &str,
        voice_id: &str,
        speed: f32,
    ) -> Result<Vec<u8>, MythicError> {
        let g2p = misaki_rs::G2P::new(misaki_rs::language::Language::EnglishUS);
        let (phonemes, _tokens) = g2p
            .g2p(text)
            .map_err(|e| MythicError::Provider(format!("Phonemization failed: {}", e)))?;
        let token_ids = phonemes_to_tokens(&phonemes);
        let n_tokens = token_ids.len();

        let style = self.voices.style_for(voice_id, n_tokens)?;
        let style_data: Vec<f32> = style.into_iter().collect();

        // Built as (shape, flat Vec<T>) rather than passing `ndarray`
        // array types directly to `TensorRef::from_array_view` — `ort`'s
        // own `ndarray` integration feature pulls in a different `ndarray`
        // major version than the one used here for voice-pack loading,
        // and the two don't satisfy the same trait impl. The
        // `(shape, Vec<T>)` construction path sidesteps that version
        // conflict entirely.
        let tokens_ref = TensorRef::from_array_view((vec![1usize, n_tokens], token_ids.as_slice()))
            .map_err(|e| MythicError::Provider(format!("tokens tensor error: {}", e)))?;
        let style_ref = TensorRef::from_array_view((vec![1usize, 256usize], style_data.as_slice()))
            .map_err(|e| MythicError::Provider(format!("style tensor error: {}", e)))?;
        let speed_data = [speed];
        let speed_ref = TensorRef::from_array_view((vec![1usize], speed_data.as_slice()))
            .map_err(|e| MythicError::Provider(format!("speed tensor error: {}", e)))?;

        let outputs = self
            .session
            .run(ort::inputs![
                "tokens" => tokens_ref,
                "style" => style_ref,
                "speed" => speed_ref,
            ])
            .map_err(|e| MythicError::Provider(format!("Kokoro inference failed: {}", e)))?;

        let (_shape, samples) = outputs["waveform"]
            .try_extract_tensor::<f32>()
            .map_err(|e| MythicError::Provider(format!("Failed to read waveform output: {}", e)))?;

        encode_wav(samples)
    }
}

fn encode_wav(samples: &[f32]) -> Result<Vec<u8>, MythicError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|e| MythicError::Provider(format!("WAV encode failed: {}", e)))?;
        for &s in samples {
            writer
                .write_sample(s)
                .map_err(|e| MythicError::Provider(format!("WAV encode failed: {}", e)))?;
        }
        writer
            .finalize()
            .map_err(|e| MythicError::Provider(format!("WAV finalize failed: {}", e)))?;
    }
    Ok(cursor.into_inner())
}

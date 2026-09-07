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

        let names = npz.names().map_err(|e| {
            MythicError::Provider(format!("Failed to list voice pack entries: {}", e))
        })?;

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
        let max_row = mat
            .nrows()
            .saturating_sub(1)
            .min(MAX_STYLE_ROWS.saturating_sub(1));
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
    /// Built once here, not per `synthesize()` call — `G2P::new()`'s
    /// lexicon/POS-tagger data is compiled into the binary (no disk I/O),
    /// but constructing the engine from it still isn't free. Recreating it
    /// on every single preview/sentence was the actual cause of a
    /// consistent multi-second-per-call delay that a smaller WAV payload
    /// alone didn't fix — this is real inference-adjacent setup cost, not
    /// I/O, so caching it here is the correct fix, not a workaround.
    g2p: misaki_rs::G2P,
}

impl KokoroEngine {
    pub fn load(
        model_path: &Path,
        voices_path: &Path,
        runtime_path: &Path,
    ) -> Result<Self, MythicError> {
        let t0 = std::time::Instant::now();

        // Must happen before any other `ort` API call — loads
        // `onnxruntime.dll` via `libloading` rather than linking ONNX
        // Runtime into this binary at compile time (see the `ort`
        // dependency comment in Cargo.toml for why: a static-link MSVC STL
        // mismatch). Idempotent — `ort`'s internal `G_ORT_LIB` is a
        // `OnceLock`, so a second call here (e.g. engine reload after a
        // failed first attempt) is a cheap no-op rather than a re-load.
        // `commit()` returns `bool` (not `Result`) — `false` only means an
        // environment was already committed elsewhere (e.g. a prior load
        // attempt this session), which is fine to ignore; the dylib-load
        // itself is what `init_from`'s `?` above actually guards.
        ort::init_from(runtime_path)
            .map_err(|e| {
                MythicError::Provider(format!(
                    "Failed to load ONNX Runtime from {}: {}",
                    runtime_path.display(),
                    e
                ))
            })?
            .commit();
        tracing::info!("[tts] ONNX Runtime dylib loaded in {:?}", t0.elapsed());

        // `commit_from_file` isn't available in this build (only
        // `commit_from_memory` is, per the `ort` 2.0.0-rc.13 API actually
        // compiled against here) — read the ~90MB model into memory
        // ourselves and commit from bytes instead. One-time cost at engine
        // load, not per synthesis call.
        let t1 = std::time::Instant::now();
        let model_bytes = std::fs::read(model_path)?;
        tracing::info!(
            "[tts] Model file read ({} bytes) in {:?}",
            model_bytes.len(),
            t1.elapsed()
        );

        let t2 = std::time::Instant::now();
        // Deliberately NOT setting `.with_intra_threads(...)` — tried
        // pinning it to `available_parallelism()` (12 on this machine,
        // likely a logical/hyperthread count, not physical cores) and it
        // measurably made inference *slower* (4.48s vs. 3.65s baseline for
        // the same clip) — Kokoro is a small model (82M params) with many
        // small ops, and ONNX Runtime's own docs confirm intra-op thread
        // *coordination* overhead can dominate over the actual compute at
        // that scale. The untouched default (0 = ORT's own physical-core
        // heuristic) already outperformed every explicit override tried
        // here, so leave it alone rather than re-guess a "better" number.
        // Optimization level 3 is independent of that regression and kept.
        let session = Session::builder()
            .map_err(|e| {
                MythicError::Provider(format!("Failed to create ONNX session builder: {}", e))
            })?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| MythicError::Provider(format!("Failed to set optimization level: {}", e)))?
            .commit_from_memory(&model_bytes)
            .map_err(|e| {
                MythicError::Provider(format!("Failed to load Kokoro ONNX model: {}", e))
            })?;
        tracing::info!(
            "[tts] ONNX session committed (default threading, opt level 3) in {:?}",
            t2.elapsed()
        );

        let t3 = std::time::Instant::now();
        let voices = VoicePack::load(voices_path)?;
        tracing::info!("[tts] Voice pack loaded in {:?}", t3.elapsed());

        // Built once here (see the `g2p` field doc comment) — this is the
        // step that was silently costing several seconds on *every*
        // synthesize() call before this fix.
        let t4 = std::time::Instant::now();
        let g2p = misaki_rs::G2P::new(misaki_rs::language::Language::EnglishUS);
        tracing::info!("[tts] G2P engine built in {:?}", t4.elapsed());

        tracing::info!("[tts] KokoroEngine::load total: {:?}", t0.elapsed());
        Ok(Self {
            session,
            voices,
            g2p,
        })
    }

    pub fn list_voices(&self) -> Vec<VoiceInfo> {
        self.voices.list()
    }

    /// Synthesizes `text` in `voice_id`'s voice at `speed` (1.0 = normal),
    /// returning WAV-encoded bytes at Kokoro's native 24kHz mono output —
    /// no resampling. `text` must be short enough to phonemize under the
    /// model's own `input_ids` limit (its card documents shape `(1, <=512)`)
    /// — for anything that might be a full multi-sentence message, use
    /// `synthesize_long` instead, which chunks by sentence first. A real
    /// "Non-zero status code ... invalid expand shape" ONNX error surfaced
    /// exactly this: a full chat message synthesized in one call here
    /// overran that limit and the encoder's Expand node choked on it.
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
        let samples = self.synthesize_samples(text, voice_id, speed)?;
        encode_wav(&samples)
    }

    /// Synthesizes arbitrarily long `text` by splitting it into sentences
    /// (reusing the same chunker the live-streaming path already relies on
    /// for exactly this reason) and running each one through the model
    /// separately, joined by a brief silence — then encodes the whole
    /// concatenated result as one WAV. Used for replaying an already-saved
    /// chat message (see `commands::chat` — replay), which can be
    /// arbitrarily long, unlike a short "preview this voice" phrase.
    pub fn synthesize_long(
        &mut self,
        text: &str,
        voice_id: &str,
        speed: f32,
    ) -> Result<Vec<u8>, MythicError> {
        let (mut sentences, remainder) = crate::tts::chunker::split_complete_sentences(text);
        let trimmed_remainder = remainder.trim();
        if !trimmed_remainder.is_empty() {
            sentences.push(trimmed_remainder.to_string());
        }
        if sentences.is_empty() {
            // Nothing looked like a sentence (e.g. a single short fragment
            // with no terminator) — still worth trying as one chunk rather
            // than silently returning empty audio.
            sentences.push(text.trim().to_string());
        }

        // ~200ms of silence between sentences reads as a natural pause,
        // not a stitching artifact, without the whole thing dragging.
        const GAP_SAMPLES: usize = (SAMPLE_RATE / 5) as usize;
        let mut all_samples: Vec<f32> = Vec::new();
        for (i, sentence) in sentences.iter().enumerate() {
            if sentence.is_empty() {
                continue;
            }
            if i > 0 && !all_samples.is_empty() {
                all_samples.extend(std::iter::repeat_n(0.0f32, GAP_SAMPLES));
            }
            let chunk_samples = self.synthesize_samples(sentence, voice_id, speed)?;
            all_samples.extend(chunk_samples);
        }
        encode_wav(&all_samples)
    }

    /// The actual inference call, returning raw f32 PCM samples rather
    /// than WAV-encoded bytes — shared by `synthesize` (single WAV) and
    /// `synthesize_long` (concatenates several of these before encoding
    /// once at the end, so there's exactly one WAV header for the whole
    /// reply instead of one per sentence).
    fn synthesize_samples(
        &mut self,
        text: &str,
        voice_id: &str,
        speed: f32,
    ) -> Result<Vec<f32>, MythicError> {
        let t0 = std::time::Instant::now();
        let (phonemes, _tokens) = self
            .g2p
            .g2p(text)
            .map_err(|e| MythicError::Provider(format!("Phonemization failed: {}", e)))?;
        let token_ids = phonemes_to_tokens(&phonemes);
        let n_tokens = token_ids.len();
        tracing::debug!(
            "[tts] Phonemized {} chars -> {} tokens in {:?}",
            text.len(),
            n_tokens,
            t0.elapsed()
        );

        let t1 = std::time::Instant::now();
        let style = self.voices.style_for(voice_id, n_tokens)?;
        let style_data: Vec<f32> = style.into_iter().collect();
        tracing::debug!("[tts] Voice style resolved in {:?}", t1.elapsed());

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

        let t2 = std::time::Instant::now();
        let outputs = self
            .session
            .run(ort::inputs![
                "input_ids" => tokens_ref,
                "style" => style_ref,
                "speed" => speed_ref,
            ])
            .map_err(|e| MythicError::Provider(format!("Kokoro inference failed: {}", e)))?;
        tracing::debug!("[tts] ONNX inference ran in {:?}", t2.elapsed());

        // Indexed positionally, not by name ("waveform"/"audio" — the
        // model's own reference Python code (`sess.run(None, ...)[0]`)
        // never names it either, and this graph only has one output, so
        // position 0 is unambiguous and doesn't depend on guessing a name.
        let (_shape, samples) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| MythicError::Provider(format!("Failed to read waveform output: {}", e)))?;
        tracing::debug!(
            "[tts] {} samples produced; total synthesize_samples() {:?}",
            samples.len(),
            t0.elapsed()
        );
        Ok(samples.to_vec())
    }
}

fn encode_wav(samples: &[f32]) -> Result<Vec<u8>, MythicError> {
    // 16-bit PCM, not 32-bit float — halves the payload that then has to go
    // through base64 + JSON + Tauri IPC on every synthesis call, with no
    // audible quality loss for speech. decodeAudioData() on the frontend
    // reads standard PCM16 WAV natively, no changes needed there.
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|e| MythicError::Provider(format!("WAV encode failed: {}", e)))?;
        for &s in samples {
            let clamped = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer
                .write_sample(clamped)
                .map_err(|e| MythicError::Provider(format!("WAV encode failed: {}", e)))?;
        }
        writer
            .finalize()
            .map_err(|e| MythicError::Provider(format!("WAV finalize failed: {}", e)))?;
    }
    Ok(cursor.into_inner())
}

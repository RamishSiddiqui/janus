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

/// Payload emitted per synthesized sentence — one per completed sentence
/// during a live streamed response (`commands::chat::streaming`) or a
/// full-message replay (`commands::tts::tts_replay_message`), same shape
/// either way so the frontend's single `ttsPlayback.ts` queue handles both
/// without caring which produced a given chunk. `audio` is base64-encoded
/// WAV — a raw `Vec<u8>` field would still serialize as a JSON array of
/// numbers over Tauri's IPC (3-5x the byte size in JSON text for a payload
/// that's routinely a few hundred KB per sentence), while base64 is ~1.33x.
#[derive(Clone, serde::Serialize)]
pub struct TtsChunkEvent {
    pub conversation_id: String,
    pub message_id: String,
    pub sequence: u32,
    pub audio: String,
    /// The exact sentence this chunk's audio was synthesized from — lets
    /// the frontend highlight the matching text while it plays (sentence-
    /// level "glow while speaking," not word-level: Kokoro's ONNX output
    /// carries no per-word timing to sync against, only raw audio).
    pub text: String,
}

/// Emitted once, after every chunk for a message has been dispatched (all
/// sentences in `tts_replay_message`'s loop; the response's `Done` handling
/// in the live-streaming path) — carries no audio, just enough to identify
/// which message is finished. On its own this doesn't mean playback has
/// actually *finished* (audio already scheduled can still be playing), so
/// the frontend combines it with "nothing left scheduled" before clearing
/// its "currently playing" state. Without this, nothing ever told the
/// frontend "no more chunks are coming" for a message with a known, finite
/// chunk count (replay) or one whose stream has ended (live) — the "Play
/// voice" button got stuck showing "Playing…" forever once its last chunk
/// finished, since only a *new* message starting or the manual stop button
/// ever cleared that state.
#[derive(Clone, serde::Serialize)]
pub struct TtsStreamEndEvent {
    pub conversation_id: String,
    pub message_id: String,
}

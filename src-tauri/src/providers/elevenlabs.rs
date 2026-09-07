//! ElevenLabs cloud TTS adapter (BYOK) — issue #78, follow-up to the
//! built-in Kokoro engine (issue #66). Mirrors `ai_horde.rs`'s shape: plain
//! `reqwest` calls, no DB access, caller resolves the `ProviderConfig` and
//! passes it in.
//!
//! API shapes verified directly against ElevenLabs' current docs (not
//! assumed): voice listing is `GET /v2/voices` (not `/v1/voices` — an
//! earlier draft of this adapter's spec had that wrong), synthesis is
//! `POST /v1/text-to-speech/{voice_id}` returning raw audio bytes directly
//! (not JSON-wrapped/base64), unlike Google Cloud TTS's `audioContent`
//! field — see `google_cloud_tts.rs`. Both adapters still return the same
//! `Vec<u8>` shape from `synthesize()` so the command layer can treat them
//! identically regardless of that difference.

use crate::error::MythicError;
use crate::models::provider::ProviderConfig;
use crate::tts::VoiceInfo;

const BASE_URL: &str = "https://api.elevenlabs.io";
/// ElevenLabs' documented default `model_id` for the synthesis endpoint —
/// passed explicitly rather than omitted, since relying on the API's own
/// default silently changes behavior if that default ever moves.
const DEFAULT_MODEL_ID: &str = "eleven_multilingual_v2";

fn api_key(provider: &ProviderConfig) -> Result<&str, MythicError> {
    provider
        .config
        .get("api_key")
        .and_then(|v| v.as_str())
        .filter(|k| !k.is_empty())
        .ok_or_else(|| MythicError::Validation("No ElevenLabs API key configured".to_string()))
}

#[derive(serde::Deserialize)]
struct VoicesResponse {
    voices: Vec<VoiceEntry>,
}

#[derive(serde::Deserialize)]
struct VoiceEntry {
    voice_id: String,
    name: Option<String>,
}

/// Lists this account's available voices. ElevenLabs accounts can have
/// hundreds to thousands of voices (their own shared library plus any
/// cloned/custom ones) — unlike Kokoro's fixed 54-voice pack, this list is
/// per-account and fetched live, not bundled.
pub async fn list_voices(
    provider: &ProviderConfig,
    http_client: &reqwest::Client,
) -> Result<Vec<VoiceInfo>, MythicError> {
    let key = api_key(provider)?;
    let resp = http_client
        .get(format!("{BASE_URL}/v2/voices"))
        .header("xi-api-key", key)
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("ElevenLabs voice list request failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "ElevenLabs voice list failed: HTTP {status} — {body}"
        )));
    }
    let parsed: VoicesResponse = resp
        .json()
        .await
        .map_err(|e| MythicError::Provider(format!("ElevenLabs voice list response parse failed: {e}")))?;
    Ok(parsed
        .voices
        .into_iter()
        .map(|v| {
            let name = v.name.unwrap_or_else(|| v.voice_id.clone());
            VoiceInfo {
                id: v.voice_id,
                name,
            }
        })
        .collect())
}

/// Synthesizes `text` in `voice_id`'s voice, returning raw MP3 bytes
/// (ElevenLabs' default `output_format`). No sentence-chunking here —
/// unlike Kokoro's ~512-token ONNX input limit, ElevenLabs has no such
/// constraint; the caller (see `commands::tts`) sends the whole message in
/// one call for a cloud provider rather than splitting by sentence.
pub async fn synthesize(
    provider: &ProviderConfig,
    http_client: &reqwest::Client,
    text: &str,
    voice_id: &str,
) -> Result<Vec<u8>, MythicError> {
    let key = api_key(provider)?;
    let resp = http_client
        .post(format!("{BASE_URL}/v1/text-to-speech/{voice_id}"))
        .header("xi-api-key", key)
        .json(&serde_json::json!({
            "text": text,
            "model_id": DEFAULT_MODEL_ID,
        }))
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("ElevenLabs synthesis request failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "ElevenLabs synthesis failed: HTTP {status} — {body}"
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| MythicError::Provider(format!("ElevenLabs audio download failed: {e}")))?;
    Ok(bytes.to_vec())
}

/// Bare liveness/auth check for "Test Connection" — the voice list call
/// doubles as this, since there's no cheaper dedicated health endpoint.
pub async fn test_connection(
    provider: &ProviderConfig,
    http_client: &reqwest::Client,
) -> Result<(), MythicError> {
    list_voices(provider, http_client).await?;
    Ok(())
}

//! Fish Audio cloud TTS adapter (BYOK) — issue #80, third cloud TTS option
//! alongside `elevenlabs.rs` and `google_cloud_tts.rs`. Same shape as both:
//! plain `reqwest` calls, no DB access, caller resolves the `ProviderConfig`
//! and passes it in.
//!
//! API shapes verified directly against Fish Audio's own docs
//! (docs.fish.audio) and the official Python SDK's source
//! (github.com/fishaudio/fish-audio-python), not assumed:
//! - Auth is a plain `Authorization: Bearer <api_key>` header (unlike
//!   ElevenLabs' `xi-api-key` header or Google's `?key=` query param).
//! - Voice listing is `GET /model` (Fish Audio calls a voice a "model") —
//!   paginated, `{ total, items: [{ _id, title, languages, ... }] }`. No
//!   `self`/`self_only` filter is sent, so this returns the public voice
//!   library sorted by popularity (`sort_by=task_count`, the SDK's own
//!   default), not just voices this account cloned — matching what a "pick
//!   a voice" picker actually wants.
//! - Synthesis is `POST /v1/tts` with a JSON body (`text`, `reference_id`,
//!   `format`) and an explicit `model` header (passed explicitly rather
//!   than left to the API's own default, same reasoning as
//!   `elevenlabs.rs`'s `DEFAULT_MODEL_ID` — relying on a silent default
//!   risks behavior changing underneath this adapter if it ever moves).
//!   The response is the raw audio body directly (MP3 by default), not
//!   JSON-wrapped/base64 like Google's `audioContent` field — same as
//!   ElevenLabs, unlike Google.

use crate::error::MythicError;
use crate::models::provider::ProviderConfig;
use crate::tts::VoiceInfo;

const BASE_URL: &str = "https://api.fish.audio";
/// Fish Audio's current flagship TTS model, passed explicitly — see the
/// module doc comment above for why.
const DEFAULT_MODEL: &str = "s2.1-pro";
/// One page is enough for a voice picker (matches the popularity-sorted
/// default) without paginating through Fish Audio's full public library,
/// which runs into the thousands.
const VOICE_LIST_PAGE_SIZE: u32 = 100;

fn api_key(provider: &ProviderConfig) -> Result<&str, MythicError> {
    provider
        .config
        .get("api_key")
        .and_then(|v| v.as_str())
        .filter(|k| !k.is_empty())
        .ok_or_else(|| MythicError::Validation("No Fish Audio API key configured".to_string()))
}

#[derive(serde::Deserialize)]
struct VoicesResponse {
    items: Vec<VoiceEntry>,
}

#[derive(serde::Deserialize)]
struct VoiceEntry {
    #[serde(rename = "_id")]
    id: String,
    title: String,
    #[serde(default)]
    languages: Vec<String>,
}

/// Lists Fish Audio's public voice library (popularity-sorted), plus any
/// voices this account has cloned. Fish Audio's own library runs into the
/// thousands, so this fetches one page rather than exhausting every page —
/// the same tradeoff `google_cloud_tts.rs` makes the other way (fetches
/// everything, since Google's ~400-voice catalog is small enough to).
pub async fn list_voices(
    provider: &ProviderConfig,
    http_client: &reqwest::Client,
) -> Result<Vec<VoiceInfo>, MythicError> {
    let key = api_key(provider)?;
    let resp = http_client
        .get(format!("{BASE_URL}/model"))
        .bearer_auth(key)
        .query(&[
            ("page_size", VOICE_LIST_PAGE_SIZE.to_string()),
            ("page_number", "1".to_string()),
            ("sort_by", "task_count".to_string()),
        ])
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("Fish Audio voice list request failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "Fish Audio voice list failed: HTTP {status} — {body}"
        )));
    }
    let parsed: VoicesResponse = resp.json().await.map_err(|e| {
        MythicError::Provider(format!("Fish Audio voice list response parse failed: {e}"))
    })?;
    Ok(parsed
        .items
        .into_iter()
        .map(|v| {
            let name = if v.languages.is_empty() {
                v.title
            } else {
                format!("{} ({})", v.title, v.languages.join(", "))
            };
            VoiceInfo { id: v.id, name }
        })
        .collect())
}

/// Synthesizes `text` in `voice_id`'s voice, returning raw MP3 bytes (Fish
/// Audio's default `format`). No sentence-chunking — same reasoning as
/// `elevenlabs.rs::synthesize`, this API has no ~512-token-style input
/// limit worth working around.
pub async fn synthesize(
    provider: &ProviderConfig,
    http_client: &reqwest::Client,
    text: &str,
    voice_id: &str,
) -> Result<Vec<u8>, MythicError> {
    let key = api_key(provider)?;
    let resp = http_client
        .post(format!("{BASE_URL}/v1/tts"))
        .bearer_auth(key)
        .header("model", DEFAULT_MODEL)
        .json(&serde_json::json!({
            "text": text,
            "reference_id": voice_id,
            "format": "mp3",
        }))
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("Fish Audio synthesis request failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "Fish Audio synthesis failed: HTTP {status} — {body}"
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| MythicError::Provider(format!("Fish Audio audio download failed: {e}")))?;
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

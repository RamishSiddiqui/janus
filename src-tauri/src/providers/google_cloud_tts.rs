//! Google Cloud Text-to-Speech adapter (BYOK) — issue #78. Mirrors
//! `elevenlabs.rs`'s shape; the one real difference is auth and response
//! encoding, noted inline below.
//!
//! Google's official docs lead with OAuth2/service-account auth, but this
//! API also accepts a plain API key via `?key=` — the same simple-key path
//! Cloud Console's "Create API Key" button enables for it, and the only
//! reasonable BYOK option for a desktop app (full OAuth2 setup is not
//! something to ask a user to do for a TTS voice). Verified directly
//! against Google's current REST reference: `voices.list` returns
//! `{ voices: [{ name, languageCodes, ssmlGender, naturalSampleRateHertz }] }`
//! (no separate friendly display name — one is built here from language +
//! gender), and `text:synthesize` returns `{ "audioContent": "<base64>" }`,
//! which must be base64-*decoded* here so this adapter's `synthesize()`
//! returns raw bytes just like `elevenlabs.rs`'s (which never had base64 to
//! begin with — ElevenLabs returns the audio body directly).

use base64::Engine as _;

use crate::error::MythicError;
use crate::models::provider::ProviderConfig;
use crate::tts::VoiceInfo;

const BASE_URL: &str = "https://texttospeech.googleapis.com/v1";

fn api_key(provider: &ProviderConfig) -> Result<&str, MythicError> {
    provider
        .config
        .get("api_key")
        .and_then(|v| v.as_str())
        .filter(|k| !k.is_empty())
        .ok_or_else(|| MythicError::Validation("No Google Cloud API key configured".to_string()))
}

/// Google's voice names embed their language as a leading `xx-YY-` segment
/// (e.g. "en-US-Neural2-F") — extracted here rather than stored separately,
/// since `synthesize`'s request body needs `languageCode` split out from
/// the `voice.name` the frontend only ever hands back as one opaque id
/// (matching `VoiceInfo`'s plain `{id, name}` shape, unchanged for this
/// provider too).
fn language_code_from_voice_name(name: &str) -> Option<&str> {
    let mut parts = name.splitn(3, '-');
    let region = parts.next()?;
    let country = parts.next()?;
    Some(&name[..region.len() + 1 + country.len()])
}

#[derive(serde::Deserialize)]
struct VoicesResponse {
    voices: Vec<VoiceEntry>,
}

#[derive(serde::Deserialize)]
struct VoiceEntry {
    name: String,
    #[serde(default)]
    language_codes: Vec<String>,
    #[serde(default)]
    ssml_gender: Option<String>,
}

/// Lists every voice Google Cloud TTS offers (400+ across all supported
/// languages — no `languageCode` filter applied, so the full catalog comes
/// back; the frontend's own picker is responsible for making that
/// navigable, not this call).
pub async fn list_voices(
    provider: &ProviderConfig,
    http_client: &reqwest::Client,
) -> Result<Vec<VoiceInfo>, MythicError> {
    let key = api_key(provider)?;
    let resp = http_client
        .get(format!("{BASE_URL}/voices"))
        .query(&[("key", key)])
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("Google Cloud TTS voice list request failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "Google Cloud TTS voice list failed: HTTP {status} — {body}"
        )));
    }
    let parsed: VoicesResponse = resp
        .json()
        .await
        .map_err(|e| MythicError::Provider(format!("Google Cloud TTS voice list response parse failed: {e}")))?;
    Ok(parsed
        .voices
        .into_iter()
        .map(|v| {
            let lang = v.language_codes.first().map(String::as_str).unwrap_or("?");
            let gender = v.ssml_gender.as_deref().unwrap_or("NEUTRAL");
            let name = format!("{} ({}, {})", v.name, lang, gender.to_lowercase());
            VoiceInfo { id: v.name, name }
        })
        .collect())
}

/// Synthesizes `text` in `voice_name`'s voice, returning raw MP3 bytes. No
/// sentence-chunking — same reasoning as `elevenlabs.rs::synthesize`, this
/// API has no ~512-token-style input limit worth working around.
pub async fn synthesize(
    provider: &ProviderConfig,
    http_client: &reqwest::Client,
    text: &str,
    voice_name: &str,
) -> Result<Vec<u8>, MythicError> {
    let key = api_key(provider)?;
    let language_code = language_code_from_voice_name(voice_name).ok_or_else(|| {
        MythicError::Validation(format!(
            "Couldn't determine a language code from Google voice id '{voice_name}'"
        ))
    })?;

    #[derive(serde::Deserialize)]
    struct SynthesizeResponse {
        #[serde(rename = "audioContent")]
        audio_content: String,
    }

    let resp = http_client
        .post(format!("{BASE_URL}/text:synthesize"))
        .query(&[("key", key)])
        .json(&serde_json::json!({
            "input": { "text": text },
            "voice": { "languageCode": language_code, "name": voice_name },
            "audioConfig": { "audioEncoding": "MP3" },
        }))
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("Google Cloud TTS synthesis request failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(MythicError::Provider(format!(
            "Google Cloud TTS synthesis failed: HTTP {status} — {body}"
        )));
    }
    let parsed: SynthesizeResponse = resp
        .json()
        .await
        .map_err(|e| MythicError::Provider(format!("Google Cloud TTS synthesis response parse failed: {e}")))?;
    base64::engine::general_purpose::STANDARD
        .decode(parsed.audio_content)
        .map_err(|e| MythicError::Provider(format!("Google Cloud TTS audioContent wasn't valid base64: {e}")))
}

/// Bare liveness/auth check for "Test Connection".
pub async fn test_connection(
    provider: &ProviderConfig,
    http_client: &reqwest::Client,
) -> Result<(), MythicError> {
    list_voices(provider, http_client).await?;
    Ok(())
}

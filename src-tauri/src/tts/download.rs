//! Downloads the Kokoro-82M ONNX model + voice pack into the app data dir
//! on first use — never bundled into the installer (see issue #66). Mirrors
//! the app-data-dir resolution pattern in `db::backup` and the
//! progress-event pattern in `providers::ai_horde`.

use std::path::{Path, PathBuf};

use futures::StreamExt;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;
use tracing::info;

use crate::error::MythicError;

const MODEL_URL: &str = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model_quantized.onnx";
const VOICES_URL: &str = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/voices/voices-v1.0.bin";

const MODEL_FILENAME: &str = "kokoro-v1.0.int8.onnx";
const VOICES_FILENAME: &str = "voices-v1.0.bin";
const TTS_DIR_NAME: &str = "tts";

pub fn tts_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(TTS_DIR_NAME)
}

pub fn model_path(app_data_dir: &Path) -> PathBuf {
    tts_dir(app_data_dir).join(MODEL_FILENAME)
}

pub fn voices_path(app_data_dir: &Path) -> PathBuf {
    tts_dir(app_data_dir).join(VOICES_FILENAME)
}

/// True once both the model and voice pack exist on disk. A cheap
/// existence check only — no checksum/size verification, matching the
/// lightweight cache-check style `db::backup` already uses rather than a
/// stronger integrity check.
pub fn is_downloaded(app_data_dir: &Path) -> bool {
    model_path(app_data_dir).exists() && voices_path(app_data_dir).exists()
}

/// Downloads the model + voice pack, emitting `tts-download-progress`
/// events (`{ phase: "model" | "voices", percent: 0-100 }`) as bytes
/// arrive. Each file downloads to a `.part` sibling and is only renamed to
/// its real name once fully written, so `is_downloaded` can never see a
/// half-downloaded file under the real filename.
pub async fn download_all(
    app: &AppHandle,
    http_client: &reqwest::Client,
) -> Result<(), MythicError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    let dir = tts_dir(&app_data_dir);
    tokio::fs::create_dir_all(&dir).await?;

    download_one(app, http_client, MODEL_URL, &model_path(&app_data_dir), "model").await?;
    download_one(
        app,
        http_client,
        VOICES_URL,
        &voices_path(&app_data_dir),
        "voices",
    )
    .await?;

    info!("[tts] Model + voice pack downloaded to {:?}", dir);
    Ok(())
}

async fn download_one(
    app: &AppHandle,
    http_client: &reqwest::Client,
    url: &str,
    dest: &Path,
    phase: &str,
) -> Result<(), MythicError> {
    let resp = http_client
        .get(url)
        .send()
        .await
        .map_err(|e| MythicError::Provider(format!("TTS asset download failed: {}", e)))?;
    if !resp.status().is_success() {
        return Err(MythicError::Provider(format!(
            "TTS asset download failed: HTTP {} for {}",
            resp.status(),
            url
        )));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let tmp_dest = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp_dest).await?;

    let mut stream = resp.bytes_stream();
    let mut last_emitted_percent: u32 = u32::MAX;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| MythicError::Provider(format!("TTS download stream error: {}", e)))?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 {
            ((downloaded as f64 / total as f64) * 100.0) as u32
        } else {
            0
        };
        // Only emit on actual percent change — a multi-hundred-KB stream
        // fires far more chunks than the UI needs progress ticks for.
        if percent != last_emitted_percent {
            last_emitted_percent = percent;
            let _ = app.emit(
                "tts-download-progress",
                serde_json::json!({ "phase": phase, "percent": percent }),
            );
        }
    }
    file.flush().await?;
    drop(file);
    tokio::fs::rename(&tmp_dest, dest).await?;
    Ok(())
}

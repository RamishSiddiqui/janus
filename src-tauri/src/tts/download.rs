//! Downloads the Kokoro-82M ONNX model + voice pack + ONNX Runtime dylib
//! into the app data dir on first use — never bundled into the installer
//! (see issue #66). Mirrors the app-data-dir resolution pattern in
//! `db::backup` and the progress-event pattern in `providers::ai_horde`.

use std::path::{Path, PathBuf};

use futures::StreamExt;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;
use tracing::info;

use crate::error::MythicError;

// `model_quantized.onnx` (92.4MB, generic dynamic quantization) measured a
// real 3.6s of pure inference time for a 2.9s clip on this machine — RTF
// ~1.25x, slower than real-time, not the ~2x-faster figure the original
// research cited for "the int8 variant." `model_q8f16.onnx` (86MB, actual
// int8-weights/fp16-activations quantization, the smallest file in this
// repo) is much closer to that benchmark's likely source and worth trying
// instead — the local cache filename below already said "int8", which
// `model_quantized.onnx` never actually was.
const MODEL_URL: &str =
    "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model_q8f16.onnx";
// The onnx-community HF repo only ships 59 *individual* per-voice files
// (voices/af_heart.bin, voices/am_adam.bin, ...), not a combined pack — the
// combined voices-v1.0.bin (28MB, all 54 v1.0 voices, numpy-format style
// vectors) that the whole Rust Kokoro ecosystem (kokoroxide, Kokoros, this
// engine's own VoicePack parser) is built against instead lives in
// thewh1teagle/kokoro-onnx's GitHub releases. Verified this URL resolves
// (200, Content-Length 28214398) before fixing the original 404.
const VOICES_URL: &str = "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.1/voices-v1.0.bin";
// Pinned to the exact ONNX Runtime release `ort-sys` 2.0.0-rc.13's own
// `download-binaries` feature fetches (see build/download/dist.tsv in the
// ort-sys source: `pyke:ort-rs/ms@1.28.0`) — same ABI, fetched from
// Microsoft's official releases instead of pyke's CDN since we're loading
// it ourselves via `load-dynamic` rather than linking it at build time.
const RUNTIME_URL: &str =
    "https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/onnxruntime-win-x64-1.28.0.zip";
/// Path *inside* the release zip to the dll we actually want.
const RUNTIME_ZIP_INNER_PATH: &str = "onnxruntime-win-x64-1.28.0/lib/onnxruntime.dll";

const MODEL_FILENAME: &str = "kokoro-v1.0.int8.onnx";
const VOICES_FILENAME: &str = "voices-v1.0.bin";
const RUNTIME_FILENAME: &str = "onnxruntime.dll";
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

/// Where the dynamically-loaded ONNX Runtime library lives once downloaded
/// — passed to `ort::init_from` before any session is created. Windows-only
/// for now (the primary dev/target platform this session); `RUNTIME_URL`
/// and `RUNTIME_ZIP_INNER_PATH` are the only spots that would need a
/// per-platform match arm to extend this to Linux/macOS.
pub fn runtime_path(app_data_dir: &Path) -> PathBuf {
    tts_dir(app_data_dir).join(RUNTIME_FILENAME)
}

/// True once the model, voice pack, and ONNX Runtime dylib all exist on
/// disk. A cheap existence check only — no checksum/size verification,
/// matching the lightweight cache-check style `db::backup` already uses
/// rather than a stronger integrity check.
pub fn is_downloaded(app_data_dir: &Path) -> bool {
    model_path(app_data_dir).exists()
        && voices_path(app_data_dir).exists()
        && runtime_path(app_data_dir).exists()
}

/// Downloads the model, voice pack, and ONNX Runtime dylib, emitting
/// `tts-download-progress` events (`{ phase: "model" | "voices" |
/// "runtime", percent: 0-100 }`) as bytes arrive. Each file downloads to a
/// `.part` sibling and is only renamed to its real name once fully
/// written, so `is_downloaded` can never see a half-downloaded file under
/// the real filename.
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

    // Skip any file that already landed successfully — without this, a
    // retry after one phase fails (e.g. voices 404s after model already
    // downloaded fine) would needlessly re-fetch the ~90MB model too.
    let model_dest = model_path(&app_data_dir);
    if !model_dest.exists() {
        download_one(app, http_client, MODEL_URL, &model_dest, "model").await?;
    }
    let voices_dest = voices_path(&app_data_dir);
    if !voices_dest.exists() {
        download_one(app, http_client, VOICES_URL, &voices_dest, "voices").await?;
    }
    let runtime_dest = runtime_path(&app_data_dir);
    if !runtime_dest.exists() {
        download_runtime(app, http_client, &runtime_dest).await?;
    }

    info!(
        "[tts] Model, voice pack, and ONNX Runtime downloaded to {:?}",
        dir
    );
    Ok(())
}

/// Downloads the ONNX Runtime release zip to a temp file, extracts just
/// `onnxruntime.dll` from it, and discards the rest — the release archive
/// also contains headers/static libs/PDBs we have no use for.
async fn download_runtime(
    app: &AppHandle,
    http_client: &reqwest::Client,
    dest: &Path,
) -> Result<(), MythicError> {
    // Reuses `download_one` to fetch the raw zip to a stable path first
    // (it handles its own internal `.part` staging + progress events) —
    // `tmp_zip` here is just where the *whole zip* lands, distinct from
    // `dest`, which is the extracted dll's final path.
    let tmp_zip = dest.with_extension("zip.tmp");
    download_one(app, http_client, RUNTIME_URL, &tmp_zip, "runtime").await?;

    let tmp_zip_read = tmp_zip.clone();
    let dest_owned = dest.to_path_buf();
    // Zip extraction is blocking/CPU work — off the async runtime thread.
    tokio::task::spawn_blocking(move || -> Result<(), MythicError> {
        let file = std::fs::File::open(&tmp_zip_read)?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| {
            MythicError::Provider(format!("Failed to open ONNX Runtime archive: {}", e))
        })?;
        let mut entry = archive.by_name(RUNTIME_ZIP_INNER_PATH).map_err(|e| {
            MythicError::Provider(format!(
                "ONNX Runtime archive is missing expected entry '{}': {}",
                RUNTIME_ZIP_INNER_PATH, e
            ))
        })?;
        let tmp_dll = dest_owned.with_extension("dll.part");
        let mut out = std::fs::File::create(&tmp_dll)?;
        std::io::copy(&mut entry, &mut out).map_err(|e| {
            MythicError::Provider(format!("Failed to extract onnxruntime.dll: {}", e))
        })?;
        drop(out);
        std::fs::rename(&tmp_dll, &dest_owned)?;
        Ok(())
    })
    .await
    .map_err(|e| MythicError::Provider(format!("Extraction task panicked: {}", e)))??;

    let _ = tokio::fs::remove_file(&tmp_zip).await;
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
        let chunk = chunk
            .map_err(|e| MythicError::Provider(format!("TTS download stream error: {}", e)))?;
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

//! Live resource usage for Janus's own process (not system-wide) — surfaced
//! in Settings so the user can visually sanity-check that resource-heavy
//! features (starting with native TTS, see `commands::tts`) aren't going
//! haywire. See GitHub issue #77.

use std::sync::Arc;

use sysinfo::get_current_pid;
use tauri::State;
use tokio::sync::RwLock;

use crate::error::MythicError;
use crate::AppState;

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct ResourceUsage {
    /// Resident memory used by this process, in MB.
    pub memory_mb: u32,
    /// CPU usage percentage since the *previous* call to this command —
    /// `sysinfo` computes this as a delta, not an instantaneous snapshot,
    /// so the first call after app start reads as 0% (nothing to diff
    /// against yet) and settles into a real number once the frontend's
    /// poll has fired a couple of times.
    pub cpu_percent: f32,
}

/// Reads this process's own current memory/CPU usage. Cheap — a targeted
/// refresh of one PID, not a full system scan.
#[tauri::command]
#[specta::specta]
pub async fn get_resource_usage(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<ResourceUsage, MythicError> {
    let monitor = state.read().await.resource_monitor.clone();
    let mut sys = monitor.lock().await;

    let pid = get_current_pid()
        .map_err(|e| MythicError::Provider(format!("Failed to get current PID: {}", e)))?;
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);

    let Some(process) = sys.process(pid) else {
        return Err(MythicError::Provider(
            "Current process not found in system table".to_string(),
        ));
    };

    Ok(ResourceUsage {
        memory_mb: (process.memory() / 1_048_576) as u32,
        cpu_percent: process.cpu_usage(),
    })
}

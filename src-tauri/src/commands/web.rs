//! Desktop controls for browser access (issue #94). Admin only.

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;

use crate::commands::actor::acting;
use crate::db::users::AuthSettingsRepo;
use crate::error::MythicError;
use crate::models::user::{NetworkSettings, WebStatus};
use crate::web::{bind_addr, WebServer, WebState};
use crate::AppState;

/// Builds the asset lookup the server uses to serve the built frontend.
pub fn asset_fn(app: &AppHandle) -> crate::web::AssetFn {
    let app = app.clone();
    Arc::new(move |path: &str| {
        app.asset_resolver()
            .get(path.to_string())
            .map(|a| (a.bytes, a.mime_type))
    })
}

/// Starts the server for the stored settings, replacing any running one.
/// A no-op (and stops a running one) when browser access is off.
pub async fn apply_settings(
    app: &AppHandle,
    state: &Arc<RwLock<AppState>>,
) -> Result<Option<String>, MythicError> {
    let (db, slot, bus) = {
        let s = state.read().await;
        (s.db.clone(), s.web_server.clone(), s.event_bus.clone())
    };
    let settings = AuthSettingsRepo::network(&db).await?;
    let mut guard = slot.lock().await;
    if let Some(running) = guard.take() {
        running.stop().await;
    }
    if !settings.enabled {
        return Ok(None);
    }
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {e}")))?;
    let tls = if settings.https {
        Some(crate::web::tls::load_or_create(&data_dir, &lan_ips())?)
    } else {
        None
    };
    let server = WebServer::start_with(
        WebState::new(db, Some(asset_fn(app)))
            .with_rpc(Arc::new(crate::web::rpc::AppRpc(app.clone())))
            .with_bus(bus)
            .with_data_dir(data_dir)
            .with_https(settings.https),
        bind_addr(settings.lan, settings.port),
        tls,
    )
    .await?;
    let bound = server.addr.to_string();
    *guard = Some(server);
    Ok(Some(bound))
}

/// The IPv4 addresses other devices on the local network could use to reach
/// this computer, read from the network adapters so it works with no internet
/// and no default route. Private ranges come first; link-local (169.254.x.x,
/// what an adapter falls back to with no router) is kept only when nothing else exists.
fn lan_ips() -> Vec<String> {
    let mut private = Vec::new();
    let mut other = Vec::new();
    let mut link_local = Vec::new();
    for iface in if_addrs::get_if_addrs().unwrap_or_default() {
        let std::net::IpAddr::V4(ip) = iface.ip() else {
            continue;
        };
        if ip.is_loopback() || ip.is_unspecified() {
            continue;
        }
        let list = if ip.is_link_local() {
            &mut link_local
        } else if ip.is_private() {
            &mut private
        } else {
            &mut other
        };
        let text = ip.to_string();
        if !list.contains(&text) {
            list.push(text);
        }
    }
    private.extend(other);
    if private.is_empty() {
        link_local
    } else {
        private
    }
}

async fn status_of(state: &Arc<RwLock<AppState>>, settings: NetworkSettings) -> WebStatus {
    let slot = state.read().await.web_server.clone();
    let guard = slot.lock().await;
    WebStatus {
        settings,
        running: guard.is_some(),
        bound: guard.as_ref().map(|s| s.addr.to_string()),
        lan_ips: lan_ips(),
        cert_fingerprint: guard.as_ref().and_then(|s| s.fingerprint.clone()),
    }
}

/// Admin only: whether browser access is on, its saved settings, and the address it is listening on.
#[tauri::command]
#[specta::specta]
pub async fn web_status(state: State<'_, Arc<RwLock<AppState>>>) -> Result<WebStatus, MythicError> {
    let (db, actor) = acting(&state).await?;
    actor.require_admin()?;
    let settings = AuthSettingsRepo::network(&db).await?;
    Ok(status_of(state.inner(), settings).await)
}

/// Admin only: turns browser access on or off, chooses loopback-only or LAN, turns HTTPS (a self-signed certificate Janus makes) on or off, and sets the port (1024 or higher). Restarts the server; if the port cannot be bound it stays off and the error is returned.
#[tauri::command]
#[specta::specta]
pub async fn web_set_network(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    enabled: bool,
    lan: bool,
    port: u16,
    https: bool,
) -> Result<WebStatus, MythicError> {
    let (db, actor) = acting(&state).await?;
    actor.require_admin()?;
    if port < 1024 {
        return Err(MythicError::Validation(
            "Choose a port of 1024 or higher.".to_string(),
        ));
    }
    let settings = NetworkSettings {
        enabled,
        lan,
        port,
        https,
    };
    AuthSettingsRepo::set_network(&db, &settings).await?;
    if let Err(e) = apply_settings(&app, state.inner()).await {
        // Couldn't bind (port in use, ...): leave it off rather than half-on.
        AuthSettingsRepo::set_network(
            &db,
            &NetworkSettings {
                enabled: false,
                ..settings
            },
        )
        .await?;
        return Err(e);
    }
    Ok(status_of(state.inner(), settings).await)
}

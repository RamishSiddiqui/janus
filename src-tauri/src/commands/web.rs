//! Desktop controls for browser access (issue #94). Admin only.

use std::sync::Arc;

use tauri::{AppHandle, State};
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
    let (db, slot) = {
        let s = state.read().await;
        (s.db.clone(), s.web_server.clone())
    };
    let settings = AuthSettingsRepo::network(&db).await?;
    let mut guard = slot.lock().await;
    if let Some(running) = guard.take() {
        running.stop().await;
    }
    if !settings.enabled {
        return Ok(None);
    }
    let server = WebServer::start(
        WebState::new(db, Some(asset_fn(app))),
        bind_addr(settings.lan, settings.port),
    )
    .await?;
    let bound = server.addr.to_string();
    *guard = Some(server);
    Ok(Some(bound))
}

async fn status_of(state: &Arc<RwLock<AppState>>, settings: NetworkSettings) -> WebStatus {
    let slot = state.read().await.web_server.clone();
    let guard = slot.lock().await;
    WebStatus {
        settings,
        running: guard.is_some(),
        bound: guard.as_ref().map(|s| s.addr.to_string()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn web_status(state: State<'_, Arc<RwLock<AppState>>>) -> Result<WebStatus, MythicError> {
    let (db, actor) = acting(&state).await?;
    actor.require_admin()?;
    let settings = AuthSettingsRepo::network(&db).await?;
    Ok(status_of(state.inner(), settings).await)
}

#[tauri::command]
#[specta::specta]
pub async fn web_set_network(
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    enabled: bool,
    lan: bool,
    port: u16,
) -> Result<WebStatus, MythicError> {
    let (db, actor) = acting(&state).await?;
    actor.require_admin()?;
    if port < 1024 {
        return Err(MythicError::Validation(
            "Choose a port of 1024 or higher.".to_string(),
        ));
    }
    let settings = NetworkSettings { enabled, lan, port };
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

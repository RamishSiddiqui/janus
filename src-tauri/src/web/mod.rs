//! Browser access to Janus over HTTP (issue #94 / #14, phase 3).
//!
//! Off by default; the admin turns it on from the desktop. It listens on
//! loopback unless LAN access is also enabled. This slice serves the built
//! frontend and the `/api/auth/*` endpoints only; the app's other commands
//! are exposed in later slices, each behind a signed-in session.

pub mod auth_routes;
pub mod events_routes;
pub mod rpc;
pub mod rpc_characters;
pub mod rpc_chat;
pub mod rpc_conversations;
pub mod rpc_providers;
pub mod rpc_tables;
pub mod util;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::Router;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::error::MythicError;
use util::RateLimiter;

/// Looks up a built frontend file by path, returning its bytes and MIME type.
pub type AssetFn = Arc<dyn Fn(&str) -> Option<(Vec<u8>, String)> + Send + Sync>;

#[derive(Clone)]
pub struct WebState {
    pub db: Surreal<Db>,
    pub assets: Option<AssetFn>,
    pub limiter: Arc<RateLimiter>,
    /// Reaches the app's command functions; `None` in tests that don't need them.
    pub rpc: Option<Arc<dyn rpc::Rpc>>,
    /// Where browser event streams read from.
    pub bus: crate::events::Bus,
}

impl WebState {
    pub fn new(db: Surreal<Db>, assets: Option<AssetFn>) -> Self {
        WebState {
            db,
            assets,
            // 10 sign-in / sign-up / recovery attempts a minute per address.
            limiter: Arc::new(RateLimiter::new(10, Duration::from_secs(60))),
            rpc: None,
            bus: crate::events::new_bus(),
        }
    }

    pub fn with_bus(mut self, bus: crate::events::Bus) -> Self {
        self.bus = bus;
        self
    }

    pub fn with_rpc(mut self, rpc: Arc<dyn rpc::Rpc>) -> Self {
        self.rpc = Some(rpc);
        self
    }
}

pub fn router(state: WebState) -> Router {
    let api = Router::new()
        .nest("/auth", auth_routes::routes())
        .route("/rpc/{name}", axum::routing::post(rpc::handle))
        .route("/events", axum::routing::get(events_routes::stream))
        .fallback(api_not_found);
    Router::new()
        .nest("/api", api)
        .fallback(static_files)
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn(util::csrf_guard))
        .layer(middleware::from_fn(util::security_headers))
        .with_state(state)
}

async fn api_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({ "error": "not_found", "message": "No such endpoint." })),
    )
        .into_response()
}

/// Serves the built frontend; unknown paths fall back to `index.html` so the
/// single-page app can handle its own routes.
async fn static_files(State(st): State<WebState>, uri: Uri) -> Response {
    let Some(assets) = &st.assets else {
        return (StatusCode::NOT_FOUND, "Frontend not available.").into_response();
    };
    let path = uri.path().trim_start_matches('/');
    let wanted = if path.is_empty() { "index.html" } else { path };
    let found = assets(wanted).or_else(|| {
        // A real file that is missing stays a 404; only route-like paths fall back.
        if wanted.rsplit('/').next().is_some_and(|f| f.contains('.')) {
            None
        } else {
            assets("index.html")
        }
    });
    match found {
        Some((bytes, mime)) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
        None => (StatusCode::NOT_FOUND, "Not found.").into_response(),
    }
}

/// A running server. Dropping it without `stop` leaves the task running.
pub struct WebServer {
    pub addr: SocketAddr,
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

impl WebServer {
    pub async fn start(state: WebState, addr: SocketAddr) -> Result<WebServer, MythicError> {
        let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
            MythicError::Config(format!("Couldn't start browser access on {addr}: {e}"))
        })?;
        let addr = listener.local_addr()?;
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let app = router(state).into_make_service_with_connect_info::<SocketAddr>();
        let task = tokio::spawn(async move {
            let served = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = rx.await;
            });
            if let Err(e) = served.await {
                tracing::error!("[web] server stopped: {e}");
            }
        });
        tracing::info!("[web] browser access listening on http://{addr}");
        Ok(WebServer {
            addr,
            shutdown: tx,
            task,
        })
    }

    pub async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = tokio::time::timeout(Duration::from_secs(3), self.task).await;
    }
}

/// Where to bind: loopback unless LAN access is on.
pub fn bind_addr(lan: bool, port: u16) -> SocketAddr {
    let ip = if lan {
        std::net::Ipv4Addr::UNSPECIFIED
    } else {
        std::net::Ipv4Addr::LOCALHOST
    };
    SocketAddr::from((ip, port))
}

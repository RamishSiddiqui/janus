//! Building blocks shared by the web routes: error mapping, the session
//! cookie, request guards, and a small per-IP rate limiter.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::WebState;
use crate::auth::service;
use crate::error::MythicError;
use crate::models::user::{DbSession, DbUser};

pub const SESSION_COOKIE: &str = "janus_session";
/// Required on every state-changing request. A cross-site page can't set a
/// custom header without a CORS preflight, which this server never grants.
pub const CSRF_HEADER: &str = "x-janus-request";

// ── errors ────────────────────────────────────────────────────────────────

pub struct ApiError(pub MythicError);

impl From<MythicError> for ApiError {
    fn from(e: MythicError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match &self.0 {
            MythicError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, "unauthorized", m.clone()),
            MythicError::Validation(m) => (StatusCode::BAD_REQUEST, "validation", m.clone()),
            MythicError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m.clone()),
            other => {
                // Internal details (database errors, paths, ...) stay in the log.
                tracing::error!("[web] {other}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "Something went wrong on the server.".to_string(),
                )
            }
        };
        (
            status,
            Json(serde_json::json!({ "error": kind, "message": message })),
        )
            .into_response()
    }
}

pub fn forbidden(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "error": "forbidden", "message": message })),
    )
        .into_response()
}

// ── cookie ────────────────────────────────────────────────────────────────

/// `Secure` can't be set: this is plain HTTP on a local network. HttpOnly keeps
/// page scripts from reading it and SameSite=Strict keeps other sites from
/// sending it.
pub fn session_cookie(token: &str, max_age_secs: i64) -> String {
    format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age_secs}")
}

pub fn clear_cookie() -> String {
    format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

pub fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    for value in headers.get_all(header::COOKIE) {
        let Ok(text) = value.to_str() else { continue };
        for part in text.split(';') {
            if let Some((name, val)) = part.trim().split_once('=') {
                if name == SESSION_COOKIE && !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// "Chrome on Windows"-style label for the device list.
pub fn device_label(headers: &HeaderMap) -> String {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let browser = if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        "Opera"
    } else if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("Chrome/") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "Browser"
    };
    let os = if ua.contains("Android") {
        "Android"
    } else if ua.contains("iPhone") || ua.contains("iPad") {
        "iOS"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        "macOS"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "unknown device"
    };
    format!("{browser} on {os}")
}

// ── guards ────────────────────────────────────────────────────────────────

/// Rejects a state-changing request unless it carries the custom header, and
/// rejects one whose `Origin` names a different host than the one addressed.
pub async fn csrf_guard(req: Request, next: Next) -> Response {
    let method = req.method();
    let safe = matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe {
        if !req.headers().contains_key(CSRF_HEADER) {
            return forbidden("Missing request header.");
        }
        if let Some(origin) = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
        {
            let host = req
                .headers()
                .get(header::HOST)
                .and_then(|v| v.to_str().ok());
            let origin_host = origin.split("://").nth(1).unwrap_or(origin);
            if host != Some(origin_host) {
                return forbidden("Cross-site request blocked.");
            }
        }
    }
    next.run(req).await
}

pub async fn security_headers(req: Request, next: Next) -> Response {
    let is_api = req.uri().path().starts_with("/api/");
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert("x-content-type-options", "nosniff".parse().expect("static"));
    h.insert("x-frame-options", "DENY".parse().expect("static"));
    h.insert("referrer-policy", "no-referrer".parse().expect("static"));
    if is_api {
        h.insert(header::CACHE_CONTROL, "no-store".parse().expect("static"));
    }
    resp
}

pub struct ClientIp(pub IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        Ok(ClientIp(ip))
    }
}

/// A signed-in account. Allows an account that still has to replace a
/// temporary passphrase, so it can do exactly that (see `ActiveUser`).
pub struct AuthedUser {
    pub user: DbUser,
    pub session: DbSession,
}

impl FromRequestParts<WebState> for AuthedUser {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &WebState) -> Result<Self, ApiError> {
        let unauth = || ApiError(MythicError::Unauthorized("Sign in first.".to_string()));
        let token = token_from_headers(&parts.headers).ok_or_else(unauth)?;
        match service::authenticate(&state.db, &token).await? {
            Some((user, session)) => Ok(AuthedUser { user, session }),
            None => Err(unauth()),
        }
    }
}

/// A signed-in account that has no pending passphrase change.
pub struct ActiveUser(pub AuthedUser);

impl FromRequestParts<WebState> for ActiveUser {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &WebState) -> Result<Self, ApiError> {
        let authed = AuthedUser::from_request_parts(parts, state).await?;
        if authed.user.must_change {
            return Err(ApiError(MythicError::Unauthorized(
                "Choose your own passphrase first.".to_string(),
            )));
        }
        Ok(ActiveUser(authed))
    }
}

// ── rate limit ────────────────────────────────────────────────────────────

/// At most `max` attempts per `window` per IP. Per-account lockout already
/// stops guessing one account; this stops one address spraying many.
pub struct RateLimiter {
    max: usize,
    window: Duration,
    hits: Mutex<HashMap<IpAddr, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new(max: usize, window: Duration) -> Self {
        RateLimiter {
            max,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn allow(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut map = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, v| {
            v.last()
                .is_some_and(|t| now.duration_since(*t) < self.window)
        });
        let entry = map.entry(ip).or_default();
        entry.retain(|t| now.duration_since(*t) < self.window);
        if entry.len() >= self.max {
            return false;
        }
        entry.push(now);
        true
    }
}

pub fn too_many() -> ApiError {
    ApiError(MythicError::Unauthorized(
        "Too many attempts from this address. Wait a minute and try again.".to_string(),
    ))
}

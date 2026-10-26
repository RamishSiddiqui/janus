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

/// HttpOnly keeps page scripts from reading it and SameSite=Strict keeps other
/// sites from sending it. `Secure` is added only when the server itself speaks
/// HTTPS: on plain HTTP a browser would refuse to store it.
pub fn session_cookie(token: &str, max_age_secs: i64, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age_secs}{secure}")
}

pub fn clear_cookie(secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0{secure}")
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

// ── host check ────────────────────────────────────────────────────────────

/// Extra host names allowed to reach the server, comma-separated, for setups
/// that put Janus behind their own domain name (a reverse proxy).
pub const ALLOWED_HOSTS_ENV: &str = "JANUS_ALLOWED_HOSTS";

/// True when the `Host` a request names is one this server is meant to be
/// reached by: an IP address, `localhost`, a bare machine name, a local-network
/// name, or a Tailscale name. A public domain name is refused. That is what
/// stops a malicious website from pointing its own domain at this machine
/// (DNS rebinding) and then talking to the server as if it were that site.
pub fn host_allowed(host_header: &str, extra: &[String]) -> bool {
    let host = host_header.trim().to_ascii_lowercase();
    // Drop the port, taking care with bracketed IPv6 such as `[::1]:8080`.
    let name = if let Some(rest) = host.strip_prefix('[') {
        match rest.split_once(']') {
            Some((addr, _)) => return addr.parse::<std::net::Ipv6Addr>().is_ok(),
            None => return false,
        }
    } else {
        host.rsplit_once(':')
            .filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
            .map_or(host.as_str(), |(name, _)| name)
    };
    let name = name.trim_end_matches('.');
    if name.is_empty() {
        return false;
    }
    if name.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    if !name.contains('.') {
        return true; // `localhost`, a machine name: never a public domain
    }
    const LOCAL_SUFFIXES: [&str; 6] = [
        ".localhost",
        ".local",
        ".lan",
        ".home.arpa",
        ".localdomain",
        ".ts.net",
    ];
    if LOCAL_SUFFIXES.iter().any(|s| name.ends_with(s)) {
        return true;
    }
    extra.iter().any(|h| h.eq_ignore_ascii_case(name))
}

fn extra_allowed_hosts() -> &'static Vec<String> {
    static EXTRA: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    EXTRA.get_or_init(|| {
        std::env::var(ALLOWED_HOSTS_ENV)
            .unwrap_or_default()
            .split(',')
            .map(|h| h.trim().to_ascii_lowercase())
            .filter(|h| !h.is_empty())
            .collect()
    })
}

/// Refuses a request whose `Host` is not one of ours (see [`host_allowed`]).
/// A request with no `Host` header is let through: browsers always send one.
pub async fn host_guard(req: Request, next: Next) -> Response {
    if let Some(host) = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
    {
        if !host_allowed(host, extra_allowed_hosts()) {
            return (
                StatusCode::MISDIRECTED_REQUEST,
                Json(serde_json::json!({
                    "error": "forbidden",
                    "message": "This address isn't allowed to reach Janus. Use the machine's IP address or local name."
                })),
            )
                .into_response();
        }
    }
    next.run(req).await
}

// ── page security policy ─────────────────────────────────────────────────

/// The `Content-Security-Policy` for the app page served to browsers.
///
/// Everything loads from this server only, no plug-ins, no framing, no calls
/// to other sites. The built page has one inline bootstrap script, which is
/// allowed by its SHA-256 hash instead of by `'unsafe-inline'`, so script
/// injected into the page would not run.
pub fn page_csp(html: &str) -> String {
    use base64::Engine as _;
    use sha2::{Digest, Sha256};

    let mut sources = String::from("'self'");
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        let after = &rest[start..];
        let Some(tag_end) = after.find('>') else {
            break;
        };
        let tag = &after[..tag_end];
        let body_start = tag_end + 1;
        let Some(close) = after[body_start..].find("</script>") else {
            break;
        };
        if !tag.contains("src=") {
            let body = &after[body_start..body_start + close];
            let digest = Sha256::digest(body.as_bytes());
            sources.push_str(&format!(
                " 'sha256-{}'",
                base64::engine::general_purpose::STANDARD.encode(digest)
            ));
        }
        rest = &after[body_start + close + "</script>".len()..];
    }
    format!(
        "default-src 'self'; script-src {sources}; style-src 'self' 'unsafe-inline'; \
         img-src 'self' blob: data:; media-src 'self' blob: data:; font-src 'self'; \
         connect-src 'self'; worker-src 'self' blob:; object-src 'none'; base-uri 'self'; \
         form-action 'self'; frame-ancestors 'none'"
    )
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

#[cfg(test)]
mod host_tests {
    use super::*;

    #[test]
    fn addresses_and_local_names_are_allowed() {
        for ok in [
            "localhost",
            "localhost:8080",
            "127.0.0.1:8080",
            "192.168.1.20:8787",
            "[::1]:8080",
            "[fe80::1]",
            "my-pc",
            "my-pc:8787",
            "living-room.local",
            "nas.lan:8787",
            "box.home.arpa",
            "desktop.tail1234.ts.net",
            "app.localhost",
        ] {
            assert!(host_allowed(ok, &[]), "{ok} should be allowed");
        }
    }

    #[test]
    fn public_domains_and_junk_are_refused() {
        for bad in [
            "evil.example.com",
            "evil.example.com:8080",
            "192.168.1.20.evil.com",
            "localhost.evil.com",
            "",
            ":8080",
            "[not-an-address]",
            "[::1",
        ] {
            assert!(!host_allowed(bad, &[]), "{bad:?} should be refused");
        }
    }

    #[test]
    fn extra_hosts_can_be_allowed_by_name() {
        let extra = vec!["janus.example.com".to_string()];
        assert!(host_allowed("Janus.Example.com:443", &extra));
        assert!(!host_allowed("other.example.com", &extra));
    }

    #[test]
    fn page_policy_allows_only_the_inline_script_by_hash() {
        let html = "<html><script>alert(1)</script><script src=\"/a.js\"></script></html>";
        let csp = page_csp(html);
        assert!(csp.contains("script-src 'self' 'sha256-"), "{csp}");
        assert_eq!(csp.matches("'sha256-").count(), 1, "{csp}");
        let script_src = csp
            .split("; ")
            .find(|d| d.starts_with("script-src"))
            .expect("script-src");
        assert!(!script_src.contains("unsafe-inline"), "{script_src}");
        for must in [
            "object-src 'none'",
            "base-uri 'self'",
            "frame-ancestors 'none'",
            "connect-src 'self'",
        ] {
            assert!(csp.contains(must), "{must} missing from {csp}");
        }
    }
}

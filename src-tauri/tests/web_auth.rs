//! The browser-access auth endpoints, driven in-process through the real
//! router (no sockets), against a real embedded database (#94, phase 3).

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use janus_lib::auth::service as auth;
use janus_lib::db::init_database;
use janus_lib::models::user::{SignupMode, UserInfo};
use janus_lib::web::util::RateLimiter;
use janus_lib::web::{router, AssetFn, WebState};
use tower::ServiceExt;

const PASS: &str = "correct horse battery";

type TestDb = surrealdb::Surreal<surrealdb::engine::local::Db>;

struct Resp {
    status: StatusCode,
    cookie: Option<String>,
    set_cookie_raw: Option<String>,
    body: serde_json::Value,
    text: String,
}

async fn app() -> (Router, TestDb, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("mythic_test_{}", uuid::Uuid::new_v4()));
    let db = init_database(&dir).await.unwrap();
    let assets: AssetFn = Arc::new(|p: &str| match p {
        "index.html" => Some((b"<html>janus</html>".to_vec(), "text/html".to_string())),
        "app.js" => Some((b"console.log(1)".to_vec(), "text/javascript".to_string())),
        _ => None,
    });
    (router(WebState::new(db.clone(), Some(assets))), db, dir)
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    json: Option<serde_json::Value>,
    csrf: bool,
) -> Resp {
    let mut req = Request::builder().method(method).uri(uri);
    if csrf {
        req = req.header("x-janus-request", "1");
    }
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let body = match json {
        Some(v) => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let set_cookie_raw = resp
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let cookie = set_cookie_raw
        .as_ref()
        .and_then(|c| c.split(';').next())
        .filter(|c| !c.ends_with('='))
        .map(str::to_string);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    Resp {
        status,
        cookie,
        set_cookie_raw,
        body,
        text,
    }
}

fn creds(user: &str, pass: &str) -> serde_json::Value {
    serde_json::json!({ "username": user, "passphrase": pass, "trusted": true })
}

#[tokio::test]
async fn first_visitor_becomes_admin_and_gets_a_locked_down_cookie() {
    let (app, _db, dir) = app().await;

    let s = call(&app, Method::GET, "/api/auth/status", None, None, false).await;
    assert_eq!(s.status, StatusCode::OK);
    assert_eq!(s.body["has_users"], false);
    assert!(s.body["user"].is_null());

    let r = call(
        &app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(creds("Ada", PASS)),
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["user"]["role"], "admin");
    assert!(r.body["recovery_key"]
        .as_str()
        .unwrap()
        .starts_with("JANUS-"));
    assert!(
        r.body.get("token").is_none(),
        "the token must only travel in the cookie"
    );
    let raw = r.set_cookie_raw.clone().unwrap();
    assert!(raw.contains("HttpOnly") && raw.contains("SameSite=Strict") && raw.contains("Path=/"));

    let me = call(
        &app,
        Method::GET,
        "/api/auth/me",
        r.cookie.as_deref(),
        None,
        false,
    )
    .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["username"], "ada");

    let s = call(
        &app,
        Method::GET,
        "/api/auth/status",
        r.cookie.as_deref(),
        None,
        false,
    )
    .await;
    assert_eq!(s.body["has_users"], true);
    assert_eq!(s.body["user"]["username"], "ada");

    let _ = std::fs::remove_dir_all(dir);
}

fn register_request(origin: Option<&str>) -> Request<Body> {
    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/register")
        .header("x-janus-request", "1")
        .header(header::HOST, "192.168.1.5:1421")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(o) = origin {
        req = req.header(header::ORIGIN, o);
    }
    req.body(Body::from(creds("ada", PASS).to_string()))
        .unwrap()
}

#[tokio::test]
async fn csrf_and_cross_origin_requests_are_refused() {
    let (app, _db, dir) = app().await;

    // No custom header.
    let r = call(
        &app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(creds("ada", PASS)),
        false,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // Right header, but an Origin that isn't this host.
    let resp = app
        .clone()
        .oneshot(register_request(Some("http://evil.example")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Same origin is fine.
    let resp = app
        .clone()
        .oneshot(register_request(Some("http://192.168.1.5:1421")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn login_logout_and_signup_mode() {
    let (app, db, dir) = app().await;
    let (admin, _) = auth::register(&db, "ada", PASS).await.unwrap();

    let anon = call(&app, Method::GET, "/api/auth/me", None, None, false).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);

    let bad = call(
        &app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(creds("ada", "not the passphrase")),
        true,
    )
    .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
    assert!(bad.cookie.is_none());

    let ok = call(
        &app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(creds("ada", PASS)),
        true,
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK);
    let cookie = ok.cookie.unwrap();
    let me = call(
        &app,
        Method::GET,
        "/api/auth/me",
        Some(&cookie),
        None,
        false,
    )
    .await;
    assert_eq!(me.status, StatusCode::OK);

    let out = call(
        &app,
        Method::POST,
        "/api/auth/logout",
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert!(out.set_cookie_raw.unwrap().contains("Max-Age=0"));
    let me = call(
        &app,
        Method::GET,
        "/api/auth/me",
        Some(&cookie),
        None,
        false,
    )
    .await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);

    // Signups are admin-only by default; opening them lets a stranger in as a member.
    let closed = call(
        &app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(creds("guest", PASS)),
        true,
    )
    .await;
    assert_eq!(closed.status, StatusCode::UNAUTHORIZED);
    auth::set_signup_mode(&db, &UserInfo::from(&admin), SignupMode::Open)
        .await
        .unwrap();
    let open = call(
        &app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(creds("guest", PASS)),
        true,
    )
    .await;
    assert_eq!(open.status, StatusCode::OK);
    assert_eq!(open.body["user"]["role"], "member");

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn admin_added_member_must_change_passphrase_before_anything_else() {
    let (app, db, dir) = app().await;
    let (admin, _) = auth::register(&db, "ada", PASS).await.unwrap();
    auth::admin_create_user(&db, &UserInfo::from(&admin), "sara", "temporary passphrase")
        .await
        .unwrap();

    let login = call(
        &app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(creds("sara", "temporary passphrase")),
        true,
    )
    .await;
    assert_eq!(login.status, StatusCode::OK);
    assert_eq!(login.body["user"]["must_change"], true);
    let cookie = login.cookie.unwrap();

    // Allowed while pending: who am I, and change the passphrase. Blocked: everything else.
    let me = call(
        &app,
        Method::GET,
        "/api/auth/me",
        Some(&cookie),
        None,
        false,
    )
    .await;
    assert_eq!(me.status, StatusCode::OK);
    let sessions = call(
        &app,
        Method::GET,
        "/api/auth/sessions",
        Some(&cookie),
        None,
        false,
    )
    .await;
    assert_eq!(sessions.status, StatusCode::UNAUTHORIZED);

    let change = call(
        &app,
        Method::POST,
        "/api/auth/change-passphrase",
        Some(&cookie),
        Some(
            serde_json::json!({ "current": "temporary passphrase", "new": "sara chose this one" }),
        ),
        true,
    )
    .await;
    assert_eq!(change.status, StatusCode::NO_CONTENT);

    // The session that made the change survives, and now works everywhere.
    let sessions = call(
        &app,
        Method::GET,
        "/api/auth/sessions",
        Some(&cookie),
        None,
        false,
    )
    .await;
    assert_eq!(sessions.status, StatusCode::OK);
    assert_eq!(sessions.body.as_array().unwrap().len(), 1);
    assert_eq!(sessions.body[0]["current"], true);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn recovery_over_http_replaces_the_passphrase() {
    let (app, _db, dir) = app().await;
    let r = call(
        &app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(creds("ada", PASS)),
        true,
    )
    .await;
    let key = r.body["recovery_key"].as_str().unwrap().to_string();
    let old_cookie = r.cookie.clone().unwrap();

    let reset = call(
        &app,
        Method::POST,
        "/api/auth/reset",
        None,
        Some(serde_json::json!({ "username": "ada", "recovery_key": key, "new_passphrase": "a brand new passphrase" })),
        true,
    )
    .await;
    assert_eq!(reset.status, StatusCode::OK);
    assert_ne!(reset.body["recovery_key"].as_str().unwrap(), key);

    // Old sessions are gone; the new cookie works.
    let old = call(
        &app,
        Method::GET,
        "/api/auth/me",
        Some(&old_cookie),
        None,
        false,
    )
    .await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
    let new = call(
        &app,
        Method::GET,
        "/api/auth/me",
        reset.cookie.as_deref(),
        None,
        false,
    )
    .await;
    assert_eq!(new.status, StatusCode::OK);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn frontend_is_served_with_spa_fallback_and_api_404s_stay_json() {
    let (app, _db, dir) = app().await;

    let root = call(&app, Method::GET, "/", None, None, false).await;
    assert_eq!(root.status, StatusCode::OK);
    assert!(root.text.contains("janus"));
    let js = call(&app, Method::GET, "/app.js", None, None, false).await;
    assert_eq!(js.status, StatusCode::OK);

    // A client-side route falls back to index.html; a missing real file does not.
    let route = call(&app, Method::GET, "/gallery/abc", None, None, false).await;
    assert!(route.text.contains("janus"));
    let missing = call(&app, Method::GET, "/missing.js", None, None, false).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let api = call(&app, Method::GET, "/api/nope", None, None, false).await;
    assert_eq!(api.status, StatusCode::NOT_FOUND);
    assert_eq!(api.body["error"], "not_found");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rate_limiter_allows_a_burst_then_refuses_per_address() {
    let limiter = RateLimiter::new(3, Duration::from_secs(60));
    let a: std::net::IpAddr = "10.0.0.1".parse().unwrap();
    let b: std::net::IpAddr = "10.0.0.2".parse().unwrap();
    assert!(limiter.allow(a) && limiter.allow(a) && limiter.allow(a));
    assert!(!limiter.allow(a));
    assert!(limiter.allow(b), "another address is unaffected");

    let short = RateLimiter::new(1, Duration::from_millis(30));
    assert!(short.allow(a));
    assert!(!short.allow(a));
    std::thread::sleep(Duration::from_millis(60));
    assert!(short.allow(a), "the window expires");
}

// ── commands over HTTP ────────────────────────────────────────────────────

mod rpc {
    use super::*;
    use janus_lib::web::rpc::{BoxFut, Rpc};
    use janus_lib::AppState;
    use tauri::test::{mock_app, MockRuntime};
    use tauri::Manager;
    use tokio::sync::{Mutex as AsyncMutex, RwLock};

    /// Reaches the real command functions through a mock Tauri app.
    struct MockRpc(tauri::AppHandle<MockRuntime>);

    impl Rpc for MockRpc {
        fn call(
            &self,
            name: String,
            args: serde_json::Value,
        ) -> BoxFut<Result<serde_json::Value, janus_lib::error::MythicError>> {
            let app = self.0.clone();
            Box::pin(async move {
                match janus_lib::web::rpc_tables::dispatch_generic(&app, &name, args).await {
                    Some(r) => r,
                    None => Err(janus_lib::error::MythicError::NotFound(format!(
                        "No such command: {name}"
                    ))),
                }
            })
        }
    }

    pub async fn app_with_rpc() -> (Router, TestDb, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("mythic_test_{}", uuid::Uuid::new_v4()));
        let db = init_database(&dir).await.unwrap();
        let tauri_app = mock_app();
        let state = AppState {
            db: db.clone(),
            http_client: reqwest::Client::new(),
            active_generations: Arc::new(AsyncMutex::new(Default::default())),
            active_scene_generations: Arc::new(AsyncMutex::new(Default::default())),
            tts_engine: Arc::new(AsyncMutex::new(None)),
            resource_monitor: Arc::new(AsyncMutex::new(sysinfo::System::new())),
            desktop_user: Arc::new(AsyncMutex::new(None)),
            web_server: Arc::new(AsyncMutex::new(None)),
        };
        tauri_app.manage(Arc::new(RwLock::new(state)));
        let handle = tauri_app.handle().clone();
        // The mock app must outlive the server for the test's duration.
        std::mem::forget(tauri_app);
        let st = WebState::new(db.clone(), None).with_rpc(Arc::new(MockRpc(handle)));
        (router(st), db, dir)
    }
}

#[tokio::test]
async fn commands_run_as_the_session_account_and_stay_separate() {
    let (app, db, dir) = rpc::app_with_rpc().await;
    let (admin, _) = auth::register(&db, "ada", PASS).await.unwrap();
    auth::set_signup_mode(&db, &UserInfo::from(&admin), SignupMode::Open)
        .await
        .unwrap();

    let ada = call(
        &app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(creds("ada", PASS)),
        true,
    )
    .await
    .cookie
    .unwrap();
    let bob = call(
        &app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(creds("bob", PASS)),
        true,
    )
    .await
    .cookie
    .unwrap();

    // No session, no commands.
    let anon = call(
        &app,
        Method::POST,
        "/api/rpc/list_characters",
        None,
        Some(serde_json::json!({})),
        true,
    )
    .await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);

    // CSRF header is required here too.
    let nocsrf = call(
        &app,
        Method::POST,
        "/api/rpc/list_characters",
        Some(&ada),
        Some(serde_json::json!({})),
        false,
    )
    .await;
    assert_eq!(nocsrf.status, StatusCode::FORBIDDEN);

    // Ada creates a character with the same camelCase arguments the desktop sends.
    let made = call(
        &app,
        Method::POST,
        "/api/rpc/create_character",
        Some(&ada),
        Some(serde_json::json!({ "name": "Elara", "data": { "name": "Elara" } })),
        true,
    )
    .await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text);
    let id = made.body["id"].as_str().unwrap().to_string();

    let ada_list = call(
        &app,
        Method::POST,
        "/api/rpc/list_characters",
        Some(&ada),
        None,
        true,
    )
    .await;
    assert!(ada_list
        .body
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["name"] == "Elara"));
    // Bob sees none of Ada's, and cannot read, change or delete her character by id.
    let bob_list = call(
        &app,
        Method::POST,
        "/api/rpc/list_characters",
        Some(&bob),
        None,
        true,
    )
    .await;
    assert!(!bob_list
        .body
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["name"] == "Elara"));
    for (cmd, body) in [
        ("get_character", serde_json::json!({ "id": id })),
        (
            "update_character",
            serde_json::json!({ "id": id, "name": "Hijacked" }),
        ),
        ("delete_character", serde_json::json!({ "id": id })),
    ] {
        let r = call(
            &app,
            Method::POST,
            &format!("/api/rpc/{cmd}"),
            Some(&bob),
            Some(body),
            true,
        )
        .await;
        assert_eq!(
            r.status,
            StatusCode::NOT_FOUND,
            "{cmd} must look like not-found to Bob"
        );
    }
    // Ada still has it, unchanged.
    let still = call(
        &app,
        Method::POST,
        "/api/rpc/get_character",
        Some(&ada),
        Some(serde_json::json!({ "id": id })),
        true,
    )
    .await;
    assert_eq!(still.body["name"], "Elara");

    // Bad arguments are a 400, unknown commands a 404.
    let bad = call(
        &app,
        Method::POST,
        "/api/rpc/get_character",
        Some(&ada),
        Some(serde_json::json!({ "wrong": 1 })),
        true,
    )
    .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let unknown = call(
        &app,
        Method::POST,
        "/api/rpc/format_the_disk",
        Some(&ada),
        None,
        true,
    )
    .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_member_with_a_pending_passphrase_change_cannot_call_commands() {
    let (app, db, dir) = rpc::app_with_rpc().await;
    let (admin, _) = auth::register(&db, "ada", PASS).await.unwrap();
    auth::admin_create_user(&db, &UserInfo::from(&admin), "sara", "temporary passphrase")
        .await
        .unwrap();
    let sara = call(
        &app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(creds("sara", "temporary passphrase")),
        true,
    )
    .await
    .cookie
    .unwrap();
    let r = call(
        &app,
        Method::POST,
        "/api/rpc/list_characters",
        Some(&sara),
        None,
        true,
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let _ = std::fs::remove_dir_all(dir);
}

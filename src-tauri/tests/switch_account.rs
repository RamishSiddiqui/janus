//! The admin's "use this window as them" switch on the desktop (#94): who may
//! switch, how impersonation is tracked, and how the admin returns.

use std::sync::Arc;

use janus_lib::auth::service as auth;
use janus_lib::commands::auth::auth_switch_to;
use janus_lib::db::init_database;
use janus_lib::error::MythicError;
use janus_lib::models::user::{SignupMode, UserInfo};
use janus_lib::AppState;
use tauri::test::mock_app;
use tauri::Manager;
use tokio::sync::{Mutex as AsyncMutex, RwLock};

const PASS: &str = "correct horse battery";

struct Fixture {
    app: tauri::App<tauri::test::MockRuntime>,
    desktop_user: Arc<AsyncMutex<Option<String>>>,
    desktop_admin: Arc<AsyncMutex<Option<String>>>,
    admin: String,
    bob: String,
    carol: String,
}

async fn fixture() -> Fixture {
    let dir = std::env::temp_dir().join(format!("mythic_test_{}", uuid::Uuid::new_v4()));
    let db = init_database(&dir).await.unwrap();
    let (admin, _) = auth::register(&db, "ada", PASS).await.unwrap();
    auth::set_signup_mode(&db, &UserInfo::from(&admin), SignupMode::Open)
        .await
        .unwrap();
    let (bob, _) = auth::register(&db, "bob", PASS).await.unwrap();
    let (carol, _) = auth::register(&db, "carol", PASS).await.unwrap();

    let desktop_user = Arc::new(AsyncMutex::new(Some(admin.id.clone())));
    let desktop_admin = Arc::new(AsyncMutex::new(None));
    let app = mock_app();
    app.manage(Arc::new(RwLock::new(AppState {
        db,
        http_client: reqwest::Client::new(),
        active_generations: Arc::new(AsyncMutex::new(Default::default())),
        active_scene_generations: Arc::new(AsyncMutex::new(Default::default())),
        tts_engine: Arc::new(AsyncMutex::new(None)),
        resource_monitor: Arc::new(AsyncMutex::new(sysinfo::System::new())),
        desktop_user: desktop_user.clone(),
        desktop_admin: desktop_admin.clone(),
        web_server: Arc::new(AsyncMutex::new(None)),
        event_bus: janus_lib::events::new_bus(),
    })));
    Fixture {
        app,
        desktop_user,
        desktop_admin,
        admin: admin.id.clone(),
        bob: bob.id.clone(),
        carol: carol.id.clone(),
    }
}

async fn switch(f: &Fixture, to: &str) -> Result<UserInfo, MythicError> {
    auth_switch_to(f.app.state::<Arc<RwLock<AppState>>>(), to.to_string()).await
}

#[tokio::test]
async fn admin_switches_into_an_account_and_returns_without_a_passphrase() {
    let f = fixture().await;

    let as_bob = switch(&f, &f.bob).await.unwrap();
    assert_eq!(as_bob.username, "bob");
    assert_eq!(f.desktop_user.lock().await.as_deref(), Some(f.bob.as_str()));
    assert_eq!(
        f.desktop_admin.lock().await.as_deref(),
        Some(f.admin.as_str())
    );

    // Moving on to another account keeps the original admin as the one to return to.
    let as_carol = switch(&f, &f.carol).await;
    assert!(as_carol.is_err(), "an impersonated member is not an admin");
    assert_eq!(f.desktop_user.lock().await.as_deref(), Some(f.bob.as_str()));

    let back = switch(&f, &f.admin).await.unwrap();
    assert_eq!(back.username, "ada");
    assert_eq!(
        f.desktop_user.lock().await.as_deref(),
        Some(f.admin.as_str())
    );
    assert!(f.desktop_admin.lock().await.is_none());
}

#[tokio::test]
async fn a_member_cannot_switch_accounts() {
    let f = fixture().await;
    *f.desktop_user.lock().await = Some(f.bob.clone());

    let err = switch(&f, &f.admin).await.unwrap_err();
    assert!(matches!(err, MythicError::Unauthorized(_)), "got {err:?}");
    let err = switch(&f, &f.carol).await.unwrap_err();
    assert!(matches!(err, MythicError::Unauthorized(_)), "got {err:?}");
    assert_eq!(f.desktop_user.lock().await.as_deref(), Some(f.bob.as_str()));
}

#[tokio::test]
async fn switching_to_a_missing_account_or_with_nobody_signed_in_fails() {
    let f = fixture().await;
    let err = switch(&f, "user:nobody").await.unwrap_err();
    assert!(matches!(err, MythicError::NotFound(_)), "got {err:?}");
    assert_eq!(
        f.desktop_user.lock().await.as_deref(),
        Some(f.admin.as_str())
    );

    *f.desktop_user.lock().await = None;
    assert!(switch(&f, &f.bob).await.is_err());
}

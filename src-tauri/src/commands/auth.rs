//! Account commands for the desktop window (issue #94). The desktop needs no
//! token: it acts as whichever account `AppState.desktop_user` names, which is
//! the admin by default and changes when the user signs out and in as someone
//! else. The network path (a later phase) will call the same
//! `auth::service` functions with a session token instead.

use std::sync::Arc;

use tauri::State;
use tokio::sync::RwLock;

use crate::auth::service;
use crate::db::users::UserRepo;
use crate::error::MythicError;
use crate::models::user::{AccountCreated, AuthStatus, SignupMode, UserInfo};
use crate::AppState;

async fn db_and_desktop(
    state: &State<'_, Arc<RwLock<AppState>>>,
) -> (
    surrealdb::Surreal<surrealdb::engine::local::Db>,
    Arc<tokio::sync::Mutex<Option<String>>>,
) {
    let s = state.read().await;
    (s.db.clone(), s.desktop_user.clone())
}

/// The account the desktop is currently acting as, if it still exists.
async fn current(
    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    desktop: &tokio::sync::Mutex<Option<String>>,
) -> Result<Option<UserInfo>, MythicError> {
    let id = desktop.lock().await.clone();
    match id {
        Some(id) => Ok(UserRepo::get(db, &id).await?.as_ref().map(UserInfo::from)),
        None => Ok(None),
    }
}

async fn require_current(
    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    desktop: &tokio::sync::Mutex<Option<String>>,
) -> Result<UserInfo, MythicError> {
    current(db, desktop)
        .await?
        .ok_or_else(|| MythicError::Unauthorized("Sign in first.".to_string()))
}

/// Whether any account exists, who may sign up, and which account this window is acting as (plus the admin it is acting for, if any).
#[tauri::command]
#[specta::specta]
pub async fn auth_status(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<AuthStatus, MythicError> {
    let (db, desktop) = db_and_desktop(&state).await;
    let admin_slot = state.read().await.desktop_admin.clone();
    let admin_id = admin_slot.lock().await.clone();
    let impersonator = match admin_id {
        Some(id) => UserRepo::get(&db, &id).await?.as_ref().map(UserInfo::from),
        None => None,
    };
    Ok(AuthStatus {
        has_users: service::has_users(&db).await?,
        signup_mode: service::signup_mode(&db).await?,
        user: current(&db, &desktop).await?,
        impersonator,
    })
}

/// Creates an account. The first one becomes the admin. The new account
/// becomes the one the desktop acts as.
#[tauri::command]
#[specta::specta]
pub async fn auth_register(
    state: State<'_, Arc<RwLock<AppState>>>,
    username: String,
    passphrase: String,
) -> Result<AccountCreated, MythicError> {
    let (db, desktop) = db_and_desktop(&state).await;
    let (user, recovery_key) = service::register(&db, &username, &passphrase).await?;
    *desktop.lock().await = Some(user.id.clone());
    *state.read().await.desktop_admin.lock().await = None;
    Ok(AccountCreated {
        user: UserInfo::from(&user),
        recovery_key,
    })
}

/// Signs the desktop window in as an account, checking its passphrase.
#[tauri::command]
#[specta::specta]
pub async fn auth_login(
    state: State<'_, Arc<RwLock<AppState>>>,
    username: String,
    passphrase: String,
) -> Result<UserInfo, MythicError> {
    let (db, desktop) = db_and_desktop(&state).await;
    let user = service::verify_credentials(&db, &username, &passphrase).await?;
    *desktop.lock().await = Some(user.id.clone());
    *state.read().await.desktop_admin.lock().await = None;
    Ok(UserInfo::from(&user))
}

/// The admin, sitting at the host machine, acts as any account without its
/// passphrase (to help someone or check what they see), and can return to the
/// admin account the same way. Desktop only: it is not reachable from the
/// browser.
#[tauri::command]
#[specta::specta]
pub async fn auth_switch_to(
    state: State<'_, Arc<RwLock<AppState>>>,
    user_id: String,
) -> Result<UserInfo, MythicError> {
    let (db, desktop) = db_and_desktop(&state).await;
    let admin_slot = state.read().await.desktop_admin.clone();
    let me = require_current(&db, &desktop).await?;
    let returning = admin_slot.lock().await.clone();
    let allowed =
        me.role == crate::models::user::Role::Admin || returning.as_deref() == Some(&user_id);
    if !allowed {
        return Err(MythicError::Unauthorized(
            "Only the admin can switch accounts.".to_string(),
        ));
    }
    let target = UserRepo::get(&db, &user_id)
        .await?
        .ok_or_else(|| MythicError::NotFound("No such account.".to_string()))?;
    let mut admin = admin_slot.lock().await;
    if me.role == crate::models::user::Role::Admin && target.id != me.id {
        *admin = Some(me.id.clone());
    } else if returning.as_deref() == Some(&target.id) {
        *admin = None;
    }
    *desktop.lock().await = Some(target.id.clone());
    Ok(UserInfo::from(&target))
}

/// Signs the desktop window out. Until someone signs in again, account-owned data is not reachable.
#[tauri::command]
#[specta::specta]
pub async fn auth_logout(state: State<'_, Arc<RwLock<AppState>>>) -> Result<(), MythicError> {
    let (_, desktop) = db_and_desktop(&state).await;
    *desktop.lock().await = None;
    *state.read().await.desktop_admin.lock().await = None;
    Ok(())
}

/// Sets a new passphrase using the account's recovery key, signs every other session out, and issues a fresh recovery key.
#[tauri::command]
#[specta::specta]
pub async fn auth_reset_with_recovery(
    state: State<'_, Arc<RwLock<AppState>>>,
    username: String,
    recovery_key: String,
    new_passphrase: String,
) -> Result<AccountCreated, MythicError> {
    let (db, desktop) = db_and_desktop(&state).await;
    let (user, new_key) =
        service::reset_with_recovery(&db, &username, &recovery_key, &new_passphrase).await?;
    *desktop.lock().await = Some(user.id.clone());
    *state.read().await.desktop_admin.lock().await = None;
    Ok(AccountCreated {
        user: UserInfo::from(&user),
        recovery_key: new_key,
    })
}

/// Changes the signed-in account's passphrase after checking the current one.
#[tauri::command]
#[specta::specta]
pub async fn auth_change_passphrase(
    state: State<'_, Arc<RwLock<AppState>>>,
    current_passphrase: String,
    new_passphrase: String,
) -> Result<(), MythicError> {
    let (db, desktop) = db_and_desktop(&state).await;
    let me = require_current(&db, &desktop).await?;
    service::change_passphrase(&db, &me.id, &current_passphrase, &new_passphrase, None).await
}

/// Admin only: chooses whether anyone can sign up (`open`) or only the admin creates accounts (`admin_only`).
#[tauri::command]
#[specta::specta]
pub async fn auth_set_signup_mode(
    state: State<'_, Arc<RwLock<AppState>>>,
    mode: SignupMode,
) -> Result<(), MythicError> {
    let (db, desktop) = db_and_desktop(&state).await;
    let me = require_current(&db, &desktop).await?;
    service::set_signup_mode(&db, &me, mode).await
}

/// Admin only: lists every account.
#[tauri::command]
#[specta::specta]
pub async fn auth_list_users(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<UserInfo>, MythicError> {
    let (db, desktop) = db_and_desktop(&state).await;
    let me = require_current(&db, &desktop).await?;
    service::list_users(&db, &me).await
}

/// Admin adds a member with a temporary passphrase they must replace on
/// first sign-in. The desktop keeps acting as the admin.
#[tauri::command]
#[specta::specta]
pub async fn auth_create_user(
    state: State<'_, Arc<RwLock<AppState>>>,
    username: String,
    temporary_passphrase: String,
) -> Result<AccountCreated, MythicError> {
    let (db, desktop) = db_and_desktop(&state).await;
    let me = require_current(&db, &desktop).await?;
    let (user, recovery_key) =
        service::admin_create_user(&db, &me, &username, &temporary_passphrase).await?;
    Ok(AccountCreated {
        user: UserInfo::from(&user),
        recovery_key,
    })
}

/// Admin only: deletes an account together with its characters, chats, memories, providers and stored files.
#[tauri::command]
#[specta::specta]
pub async fn auth_delete_user(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    user_id: String,
) -> Result<(), MythicError> {
    use tauri::Manager;
    let (db, desktop) = db_and_desktop(&state).await;
    let me = require_current(&db, &desktop).await?;
    // Collect the account's files before its rows disappear, remove them after.
    let files = crate::db::users::OwnershipRepo::files_of_owner(&db, &user_id).await?;
    service::delete_user(&db, &me, &user_id).await?;
    if let Ok(dir) = app.path().app_data_dir() {
        for relative in files {
            if let Ok(full) = crate::error::resolve_within(&dir, &relative) {
                if let Err(e) = tokio::fs::remove_file(&full).await {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!("Failed to remove {}: {}", full.display(), e);
                    }
                }
            }
        }
    }
    Ok(())
}

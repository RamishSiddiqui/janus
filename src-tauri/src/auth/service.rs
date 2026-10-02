//! Sign-up, sign-in, reset and session logic. Callers (the desktop's Tauri
//! commands now, the HTTP server later) pass in who is acting; nothing here
//! reads global state.

use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use super::*;
use crate::db::users::{AuthSettingsRepo, SessionRepo, UserRepo};
use crate::models::user::{DbSession, DbUser, Role, SessionInfo, SignupMode, UserInfo};

const BAD_CREDENTIALS: &str = "That username and passphrase don't match.";
const BAD_RECOVERY: &str = "That username and recovery key don't match.";

/// Serializes account creation so two simultaneous first visitors can't both
/// become admin.
static CREATE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

async fn hash_blocking(secret: String) -> Result<String, MythicError> {
    tokio::task::spawn_blocking(move || hash_secret(&secret))
        .await
        .map_err(|e| MythicError::Config(format!("Hashing task failed: {e}")))?
}

async fn verify_blocking(secret: String, hash: String) -> Result<bool, MythicError> {
    tokio::task::spawn_blocking(move || verify_secret(&secret, &hash))
        .await
        .map_err(|e| MythicError::Config(format!("Hashing task failed: {e}")))
}

async fn dummy_blocking(secret: String) {
    let _ = tokio::task::spawn_blocking(move || dummy_verify(&secret)).await;
}

pub fn require_admin(actor: &UserInfo) -> Result<(), MythicError> {
    if actor.role == Role::Admin {
        Ok(())
    } else {
        Err(MythicError::Unauthorized(
            "Only the admin can do that.".to_string(),
        ))
    }
}

pub async fn has_users(db: &Surreal<Db>) -> Result<bool, MythicError> {
    Ok(UserRepo::count(db).await? > 0)
}

pub async fn signup_mode(db: &Surreal<Db>) -> Result<SignupMode, MythicError> {
    AuthSettingsRepo::signup_mode(db).await
}

pub async fn set_signup_mode(
    db: &Surreal<Db>,
    actor: &UserInfo,
    mode: SignupMode,
) -> Result<(), MythicError> {
    require_admin(actor)?;
    AuthSettingsRepo::set_signup_mode(db, mode).await
}

async fn create_account(
    db: &Surreal<Db>,
    username: &str,
    passphrase: &str,
    role: Role,
    must_change: bool,
) -> Result<(DbUser, String), MythicError> {
    if UserRepo::get_by_username(db, username).await?.is_some() {
        return Err(MythicError::Validation(
            "That username is taken.".to_string(),
        ));
    }
    let passphrase_hash = hash_blocking(passphrase.to_string()).await?;
    let recovery_key = generate_recovery_key();
    let recovery_hash = hash_blocking(recovery_key.clone()).await?;
    let user = UserRepo::create(
        db,
        username,
        role,
        &passphrase_hash,
        &recovery_hash,
        must_change,
    )
    .await?;
    Ok((user, recovery_key))
}

/// Self-service sign-up. The very first account becomes the admin whatever
/// the signup setting says; after that it follows the setting. Returns the
/// new account and its recovery key (shown once).
pub async fn register(
    db: &Surreal<Db>,
    username: &str,
    passphrase: &str,
) -> Result<(DbUser, String), MythicError> {
    let username = normalize_username(username)?;
    validate_passphrase(passphrase)?;
    let _guard = CREATE_LOCK.lock().await;
    let first = UserRepo::count(db).await? == 0;
    if !first && AuthSettingsRepo::signup_mode(db).await? != SignupMode::Open {
        return Err(MythicError::Unauthorized(
            "Signups are turned off. Ask the admin to add you.".to_string(),
        ));
    }
    let role = if first { Role::Admin } else { Role::Member };
    create_account(db, &username, passphrase, role, false).await
}

/// Admin adds a member with a temporary passphrase they must replace on
/// first sign-in.
pub async fn admin_create_user(
    db: &Surreal<Db>,
    actor: &UserInfo,
    username: &str,
    temporary_passphrase: &str,
) -> Result<(DbUser, String), MythicError> {
    require_admin(actor)?;
    let username = normalize_username(username)?;
    validate_passphrase(temporary_passphrase)?;
    let _guard = CREATE_LOCK.lock().await;
    create_account(db, &username, temporary_passphrase, Role::Member, true).await
}

/// Counts a wrong secret against the account, locking it after
/// `MAX_FAILED_ATTEMPTS` in a row. Returns the error to show.
async fn register_failure(
    db: &Surreal<Db>,
    user: &DbUser,
    generic: &str,
) -> Result<MythicError, MythicError> {
    let attempts = user.failed_attempts + 1;
    if attempts >= MAX_FAILED_ATTEMPTS {
        UserRepo::set_failures(db, &user.id, 0, now() + LOCKOUT_SECS).await?;
        return Ok(MythicError::Unauthorized(format!(
            "Too many tries. This account is locked for {} minutes.",
            LOCKOUT_SECS / 60
        )));
    }
    UserRepo::set_failures(db, &user.id, attempts, 0).await?;
    let left = MAX_FAILED_ATTEMPTS - attempts;
    Ok(MythicError::Unauthorized(format!(
        "{generic} {left} {} left before a {}-minute lock.",
        if left == 1 { "attempt" } else { "attempts" },
        LOCKOUT_SECS / 60
    )))
}

fn check_not_locked(user: &DbUser) -> Result<(), MythicError> {
    let remaining = user.locked_until - now();
    if remaining > 0 {
        return Err(MythicError::Unauthorized(format!(
            "Too many tries. Try again in {} seconds.",
            remaining
        )));
    }
    Ok(())
}

/// Checks a username + passphrase and returns the account. Unknown usernames
/// and wrong passphrases give the same answer in the same time.
pub async fn verify_credentials(
    db: &Surreal<Db>,
    username: &str,
    passphrase: &str,
) -> Result<DbUser, MythicError> {
    let bad = || MythicError::Unauthorized(BAD_CREDENTIALS.to_string());
    let username = normalize_username(username).map_err(|_| bad())?;
    let Some(user) = UserRepo::get_by_username(db, &username).await? else {
        dummy_blocking(passphrase.to_string()).await;
        return Err(bad());
    };
    check_not_locked(&user)?;
    if verify_blocking(passphrase.to_string(), user.passphrase_hash.clone()).await? {
        if user.failed_attempts > 0 || user.locked_until != 0 {
            UserRepo::set_failures(db, &user.id, 0, 0).await?;
        }
        Ok(user)
    } else {
        Err(register_failure(db, &user, BAD_CREDENTIALS).await?)
    }
}

/// Starts a session and returns its token (shown to the client once).
pub async fn issue_session(
    db: &Surreal<Db>,
    user_id: &str,
    label: &str,
    trusted: bool,
) -> Result<String, MythicError> {
    let token = new_session_token();
    let t = now();
    let lifetime = if trusted {
        TRUSTED_SESSION_DAYS * 86_400
    } else {
        SHORT_SESSION_HOURS * 3_600
    };
    SessionRepo::delete_expired(db, t).await?;
    SessionRepo::create(
        db,
        user_id,
        &hash_token(&token),
        label,
        trusted,
        t,
        t + lifetime,
    )
    .await?;
    Ok(token)
}

pub async fn login(
    db: &Surreal<Db>,
    username: &str,
    passphrase: &str,
    trusted: bool,
    label: &str,
) -> Result<(DbUser, String), MythicError> {
    let user = verify_credentials(db, username, passphrase).await?;
    let token = issue_session(db, &user.id, label, trusted).await?;
    Ok((user, token))
}

/// Resolves a session token to its account, or `None` if the token is
/// unknown, expired, or its account was deleted.
pub async fn authenticate(
    db: &Surreal<Db>,
    token: &str,
) -> Result<Option<(DbUser, DbSession)>, MythicError> {
    let Some(session) = SessionRepo::find_by_token_hash(db, &hash_token(token)).await? else {
        return Ok(None);
    };
    let t = now();
    if session.expires_at <= t {
        SessionRepo::delete(db, &session.id).await?;
        return Ok(None);
    }
    let Some(user) = UserRepo::get(db, &session.user_id).await? else {
        SessionRepo::delete(db, &session.id).await?;
        return Ok(None);
    };
    if t - session.last_seen > 60 {
        SessionRepo::touch(db, &session.id, t).await?;
    }
    Ok(Some((user, session)))
}

pub async fn logout(db: &Surreal<Db>, token: &str) -> Result<(), MythicError> {
    if let Some(session) = SessionRepo::find_by_token_hash(db, &hash_token(token)).await? {
        SessionRepo::delete(db, &session.id).await?;
    }
    Ok(())
}

/// Sets a new passphrase using the recovery key, issues a fresh recovery key
/// (the old one is spent) and signs the account out everywhere.
pub async fn reset_with_recovery(
    db: &Surreal<Db>,
    username: &str,
    recovery_key: &str,
    new_passphrase: &str,
) -> Result<(DbUser, String), MythicError> {
    let bad = || MythicError::Unauthorized(BAD_RECOVERY.to_string());
    validate_passphrase(new_passphrase)?;
    let username = normalize_username(username).map_err(|_| bad())?;
    let Some(user) = UserRepo::get_by_username(db, &username).await? else {
        dummy_blocking(recovery_key.to_string()).await;
        return Err(bad());
    };
    check_not_locked(&user)?;
    let Some(key) = normalize_recovery_key(recovery_key) else {
        return Err(register_failure(db, &user, BAD_RECOVERY).await?);
    };
    if !verify_blocking(key, user.recovery_hash.clone()).await? {
        return Err(register_failure(db, &user, BAD_RECOVERY).await?);
    }
    let passphrase_hash = hash_blocking(new_passphrase.to_string()).await?;
    let new_key = generate_recovery_key();
    let recovery_hash = hash_blocking(new_key.clone()).await?;
    UserRepo::set_passphrase(db, &user.id, &passphrase_hash, false).await?;
    UserRepo::set_recovery_hash(db, &user.id, &recovery_hash).await?;
    SessionRepo::delete_for_user(db, &user.id, None).await?;
    Ok((user, new_key))
}

/// Changes the passphrase of a signed-in account. Other sessions are signed
/// out; `keep_session` (the caller's own) survives.
pub async fn change_passphrase(
    db: &Surreal<Db>,
    user_id: &str,
    current: &str,
    new: &str,
    keep_session: Option<&str>,
) -> Result<(), MythicError> {
    validate_passphrase(new)?;
    let user = UserRepo::get(db, user_id)
        .await?
        .ok_or_else(|| MythicError::NotFound("Account not found".to_string()))?;
    check_not_locked(&user)?;
    if !verify_blocking(current.to_string(), user.passphrase_hash.clone()).await? {
        return Err(register_failure(db, &user, "That isn't your current passphrase.").await?);
    }
    let hash = hash_blocking(new.to_string()).await?;
    UserRepo::set_passphrase(db, &user.id, &hash, false).await?;
    SessionRepo::delete_for_user(db, &user.id, keep_session).await?;
    Ok(())
}

pub async fn list_users(db: &Surreal<Db>, actor: &UserInfo) -> Result<Vec<UserInfo>, MythicError> {
    require_admin(actor)?;
    Ok(UserRepo::list(db)
        .await?
        .iter()
        .map(UserInfo::from)
        .collect())
}

pub async fn delete_user(
    db: &Surreal<Db>,
    actor: &UserInfo,
    target_id: &str,
) -> Result<(), MythicError> {
    require_admin(actor)?;
    if actor.id == target_id {
        return Err(MythicError::Validation(
            "You can't delete your own account.".to_string(),
        ));
    }
    let target = UserRepo::get(db, target_id)
        .await?
        .ok_or_else(|| MythicError::NotFound("Account not found".to_string()))?;
    if target.role == Role::Admin && UserRepo::count_admins(db).await? <= 1 {
        return Err(MythicError::Validation(
            "There has to be at least one admin.".to_string(),
        ));
    }
    UserRepo::delete(db, target_id).await
}

pub async fn list_sessions(
    db: &Surreal<Db>,
    user_id: &str,
    current_session_id: Option<&str>,
) -> Result<Vec<SessionInfo>, MythicError> {
    Ok(SessionRepo::list_for_user(db, user_id)
        .await?
        .into_iter()
        .filter(|s| s.expires_at > now())
        .map(|s| SessionInfo {
            current: Some(s.id.as_str()) == current_session_id,
            id: s.id,
            label: s.label,
            trusted: s.trusted,
            created_at: s.created_at,
            last_seen: s.last_seen,
            expires_at: s.expires_at,
        })
        .collect())
}

pub async fn revoke_session(
    db: &Surreal<Db>,
    user_id: &str,
    session_id: &str,
) -> Result<(), MythicError> {
    let owned = SessionRepo::list_for_user(db, user_id)
        .await?
        .iter()
        .any(|s| s.id == session_id);
    if !owned {
        return Err(MythicError::NotFound("Session not found".to_string()));
    }
    SessionRepo::delete(db, session_id).await
}

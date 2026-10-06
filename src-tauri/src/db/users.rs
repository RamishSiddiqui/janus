//! Storage for accounts, sessions and the auth settings (issue #94).
//! Pure persistence: all policy (lockout, expiry, who may do what) lives in
//! `auth::service`.

use surrealdb::engine::local::Db;
use surrealdb::types::Value;
use surrealdb::Surreal;

use crate::db::value_bridge::{from_value_vec, to_surreal_value};
use crate::error::MythicError;
use crate::models::user::{DbSession, DbUser, NetworkSettings, Role, SignupMode, DEFAULT_WEB_PORT};

async fn rows(
    db: &Surreal<Db>,
    sql: &str,
    bind: serde_json::Value,
) -> Result<Vec<Value>, MythicError> {
    let mut result = db.query(sql).bind(to_surreal_value(bind)).await?;
    Ok(result.take(0)?)
}

fn role_str(role: Role) -> &'static str {
    match role {
        Role::Admin => "admin",
        Role::Member => "member",
    }
}

pub struct UserRepo;

impl UserRepo {
    pub async fn count(db: &Surreal<Db>) -> Result<usize, MythicError> {
        Ok(
            rows(db, "SELECT VALUE uid FROM users", serde_json::json!({}))
                .await?
                .len(),
        )
    }

    pub async fn create(
        db: &Surreal<Db>,
        username: &str,
        role: Role,
        passphrase_hash: &str,
        recovery_hash: &str,
        must_change: bool,
    ) -> Result<DbUser, MythicError> {
        let uid = uuid::Uuid::new_v4().to_string();
        let created: Vec<DbUser> = from_value_vec(
            rows(
                db,
                "CREATE type::record('users', $uid) CONTENT {
                    uid: $uid, username: $username, role: $role,
                    passphrase_hash: $ph, recovery_hash: $rh, must_change: $mc
                }",
                serde_json::json!({
                    "uid": uid, "username": username, "role": role_str(role),
                    "ph": passphrase_hash, "rh": recovery_hash, "mc": must_change,
                }),
            )
            .await?,
        )?;
        created
            .into_iter()
            .next()
            .ok_or_else(|| MythicError::DatabaseOp("Failed to create user".into()))
    }

    pub async fn get(db: &Surreal<Db>, uid: &str) -> Result<Option<DbUser>, MythicError> {
        let found: Vec<DbUser> = from_value_vec(
            rows(
                db,
                "SELECT * FROM users WHERE uid = $uid LIMIT 1",
                serde_json::json!({ "uid": uid }),
            )
            .await?,
        )?;
        Ok(found.into_iter().next())
    }

    pub async fn get_by_username(
        db: &Surreal<Db>,
        username: &str,
    ) -> Result<Option<DbUser>, MythicError> {
        let found: Vec<DbUser> = from_value_vec(
            rows(
                db,
                "SELECT * FROM users WHERE username = $u LIMIT 1",
                serde_json::json!({ "u": username }),
            )
            .await?,
        )?;
        Ok(found.into_iter().next())
    }

    pub async fn list(db: &Surreal<Db>) -> Result<Vec<DbUser>, MythicError> {
        from_value_vec(
            rows(
                db,
                "SELECT * FROM users ORDER BY created_at ASC",
                serde_json::json!({}),
            )
            .await?,
        )
    }

    /// The oldest admin account. The desktop window acts as this account
    /// until the user signs in as someone else.
    pub async fn first_admin(db: &Surreal<Db>) -> Result<Option<DbUser>, MythicError> {
        let found: Vec<DbUser> = from_value_vec(
            rows(
                db,
                "SELECT * FROM users WHERE role = 'admin' ORDER BY created_at ASC LIMIT 1",
                serde_json::json!({}),
            )
            .await?,
        )?;
        Ok(found.into_iter().next())
    }

    pub async fn count_admins(db: &Surreal<Db>) -> Result<usize, MythicError> {
        Ok(rows(
            db,
            "SELECT VALUE uid FROM users WHERE role = 'admin'",
            serde_json::json!({}),
        )
        .await?
        .len())
    }

    pub async fn delete(db: &Surreal<Db>, uid: &str) -> Result<(), MythicError> {
        rows(
            db,
            "DELETE type::record('users', $uid)",
            serde_json::json!({ "uid": uid }),
        )
        .await?;
        SessionRepo::delete_for_user(db, uid, None).await
    }

    /// Sets a new passphrase hash and clears any lockout.
    pub async fn set_passphrase(
        db: &Surreal<Db>,
        uid: &str,
        hash: &str,
        must_change: bool,
    ) -> Result<(), MythicError> {
        rows(
            db,
            "UPDATE type::record('users', $uid)
             SET passphrase_hash = $h, must_change = $mc, failed_attempts = 0, locked_until = 0",
            serde_json::json!({ "uid": uid, "h": hash, "mc": must_change }),
        )
        .await?;
        Ok(())
    }

    pub async fn set_recovery_hash(
        db: &Surreal<Db>,
        uid: &str,
        hash: &str,
    ) -> Result<(), MythicError> {
        rows(
            db,
            "UPDATE type::record('users', $uid) SET recovery_hash = $h",
            serde_json::json!({ "uid": uid, "h": hash }),
        )
        .await?;
        Ok(())
    }

    pub async fn set_failures(
        db: &Surreal<Db>,
        uid: &str,
        failed_attempts: u32,
        locked_until: i64,
    ) -> Result<(), MythicError> {
        rows(
            db,
            "UPDATE type::record('users', $uid) SET failed_attempts = $a, locked_until = $l",
            serde_json::json!({ "uid": uid, "a": failed_attempts, "l": locked_until }),
        )
        .await?;
        Ok(())
    }
}

pub struct SessionRepo;

impl SessionRepo {
    pub async fn create(
        db: &Surreal<Db>,
        user_id: &str,
        token_hash: &str,
        label: &str,
        trusted: bool,
        now: i64,
        expires_at: i64,
    ) -> Result<DbSession, MythicError> {
        let uid = uuid::Uuid::new_v4().to_string();
        let created: Vec<DbSession> = from_value_vec(
            rows(
                db,
                "CREATE type::record('sessions', $uid) CONTENT {
                    uid: $uid, user_id: $user_id, token_hash: $th, label: $label,
                    trusted: $trusted, last_seen: $now, expires_at: $exp
                }",
                serde_json::json!({
                    "uid": uid, "user_id": user_id, "th": token_hash, "label": label,
                    "trusted": trusted, "now": now, "exp": expires_at,
                }),
            )
            .await?,
        )?;
        created
            .into_iter()
            .next()
            .ok_or_else(|| MythicError::DatabaseOp("Failed to create session".into()))
    }

    pub async fn find_by_token_hash(
        db: &Surreal<Db>,
        token_hash: &str,
    ) -> Result<Option<DbSession>, MythicError> {
        let found: Vec<DbSession> = from_value_vec(
            rows(
                db,
                "SELECT * FROM sessions WHERE token_hash = $th LIMIT 1",
                serde_json::json!({ "th": token_hash }),
            )
            .await?,
        )?;
        Ok(found.into_iter().next())
    }

    pub async fn list_for_user(
        db: &Surreal<Db>,
        user_id: &str,
    ) -> Result<Vec<DbSession>, MythicError> {
        from_value_vec(
            rows(
                db,
                "SELECT * FROM sessions WHERE user_id = $u ORDER BY created_at DESC",
                serde_json::json!({ "u": user_id }),
            )
            .await?,
        )
    }

    pub async fn touch(db: &Surreal<Db>, uid: &str, now: i64) -> Result<(), MythicError> {
        rows(
            db,
            "UPDATE type::record('sessions', $uid) SET last_seen = $now",
            serde_json::json!({ "uid": uid, "now": now }),
        )
        .await?;
        Ok(())
    }

    pub async fn delete(db: &Surreal<Db>, uid: &str) -> Result<(), MythicError> {
        rows(
            db,
            "DELETE type::record('sessions', $uid)",
            serde_json::json!({ "uid": uid }),
        )
        .await?;
        Ok(())
    }

    /// Removes every session of an account, optionally keeping one.
    pub async fn delete_for_user(
        db: &Surreal<Db>,
        user_id: &str,
        except: Option<&str>,
    ) -> Result<(), MythicError> {
        rows(
            db,
            "DELETE sessions WHERE user_id = $u AND uid != $keep",
            serde_json::json!({ "u": user_id, "keep": except.unwrap_or("") }),
        )
        .await?;
        Ok(())
    }

    pub async fn delete_expired(db: &Surreal<Db>, now: i64) -> Result<(), MythicError> {
        rows(
            db,
            "DELETE sessions WHERE expires_at < $now",
            serde_json::json!({ "now": now }),
        )
        .await?;
        Ok(())
    }
}

pub struct AuthSettingsRepo;

impl AuthSettingsRepo {
    pub async fn signup_mode(db: &Surreal<Db>) -> Result<SignupMode, MythicError> {
        let found: Vec<serde_json::Value> = from_value_vec(
            rows(
                db,
                "SELECT signup_mode FROM type::record('auth_settings', 'main')",
                serde_json::json!({}),
            )
            .await?,
        )?;
        let mode = found
            .first()
            .and_then(|v| v.get("signup_mode"))
            .and_then(|v| v.as_str());
        Ok(match mode {
            Some("open") => SignupMode::Open,
            _ => SignupMode::AdminOnly,
        })
    }

    /// Browser-access settings. Off by default, loopback-only unless `lan`.
    pub async fn network(db: &Surreal<Db>) -> Result<NetworkSettings, MythicError> {
        let found: Vec<serde_json::Value> = from_value_vec(
            rows(
                db,
                "SELECT network_enabled, network_lan, network_port FROM type::record('auth_settings', 'main')",
                serde_json::json!({}),
            )
            .await?,
        )?;
        let row = found.first();
        let get_bool = |k: &str| row.and_then(|v| v.get(k)).and_then(|v| v.as_bool());
        let port = row
            .and_then(|v| v.get("network_port"))
            .and_then(|v| v.as_u64())
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(DEFAULT_WEB_PORT);
        Ok(NetworkSettings {
            enabled: get_bool("network_enabled").unwrap_or(false),
            lan: get_bool("network_lan").unwrap_or(false),
            port,
        })
    }

    pub async fn set_network(db: &Surreal<Db>, s: &NetworkSettings) -> Result<(), MythicError> {
        rows(
            db,
            "UPSERT type::record('auth_settings', 'main')
             SET network_enabled = $e, network_lan = $l, network_port = $p",
            serde_json::json!({ "e": s.enabled, "l": s.lan, "p": s.port }),
        )
        .await?;
        Ok(())
    }

    pub async fn set_signup_mode(db: &Surreal<Db>, mode: SignupMode) -> Result<(), MythicError> {
        rows(
            db,
            "UPSERT type::record('auth_settings', 'main') SET signup_mode = $m",
            serde_json::json!({ "m": mode.as_str() }),
        )
        .await?;
        Ok(())
    }
}

/// Tables whose rows belong to one account (their `owner_id` column).
/// Everything else is reached through one of these.
pub const OWNED_TABLES: [&str; 7] = [
    "characters",
    "personas",
    "conversations",
    "provider_configs",
    "image_presets",
    "lorebook_entries",
    "memories",
];

pub struct OwnershipRepo;

impl OwnershipRepo {
    /// Gives every unowned row (`owner_id = ''`) to `user_id`. Run when the
    /// first account is created so the data that existed before accounts
    /// becomes that account's.
    pub async fn claim_unowned(db: &Surreal<Db>, user_id: &str) -> Result<(), MythicError> {
        for table in OWNED_TABLES {
            rows(
                db,
                &format!("UPDATE {table} SET owner_id = $u WHERE owner_id = ''"),
                serde_json::json!({ "u": user_id }),
            )
            .await?;
        }
        Ok(())
    }

    /// Assigns one row to an account.
    pub async fn set_owner(
        db: &Surreal<Db>,
        table: &str,
        id: &str,
        owner: &str,
    ) -> Result<(), MythicError> {
        if !OWNED_TABLES.contains(&table) {
            return Err(MythicError::Validation(format!(
                "{table} has no owner column"
            )));
        }
        rows(
            db,
            "UPDATE type::record($t, $id) SET owner_id = $o",
            serde_json::json!({ "t": table, "id": id, "o": owner }),
        )
        .await?;
        Ok(())
    }

    /// The owner of a row that belongs to a conversation (a message or a
    /// scene), read through its `conversation_id` link. `None` if the row
    /// doesn't exist.
    pub async fn owner_via_conversation(
        db: &Surreal<Db>,
        table: &str,
        id: &str,
    ) -> Result<Option<String>, MythicError> {
        if !matches!(table, "messages" | "scenes") {
            return Err(MythicError::Validation(format!(
                "{table} is not a conversation child"
            )));
        }
        let found: Vec<serde_json::Value> = from_value_vec(
            rows(
                db,
                "SELECT conversation_id.owner_id AS owner_id FROM type::record($t, $id)",
                serde_json::json!({ "t": table, "id": id }),
            )
            .await?,
        )?;
        Ok(found.first().map(|v| {
            v.get("owner_id")
                .and_then(|o| o.as_str())
                .unwrap_or("")
                .to_string()
        }))
    }

    /// The `owner_id` of one row, or `None` if the row doesn't exist.
    pub async fn owner_of(
        db: &Surreal<Db>,
        table: &str,
        id: &str,
    ) -> Result<Option<String>, MythicError> {
        if !OWNED_TABLES.contains(&table) {
            return Err(MythicError::Validation(format!(
                "{table} has no owner column"
            )));
        }
        let found: Vec<serde_json::Value> = from_value_vec(
            rows(
                db,
                "SELECT owner_id FROM type::record($t, $id)",
                serde_json::json!({ "t": table, "id": id }),
            )
            .await?,
        )?;
        Ok(found.first().map(|v| {
            v.get("owner_id")
                .and_then(|o| o.as_str())
                .unwrap_or("")
                .to_string()
        }))
    }
}

impl OwnershipRepo {
    /// Deletes everything an account owns: its chats (and, through the
    /// cascade events, their messages, scenes, summaries and embeddings),
    /// characters, personas, memories, lorebook entries, image presets, and
    /// providers with their stored API keys. Used when an account is deleted.
    pub async fn delete_all_owned(db: &Surreal<Db>, user_id: &str) -> Result<(), MythicError> {
        // enabled_models point at providers; remove them before the providers.
        let statements = [
            "DELETE enabled_models WHERE provider_id.owner_id = $u".to_string(),
            "DELETE conversations WHERE owner_id = $u".to_string(),
            "DELETE characters WHERE owner_id = $u".to_string(),
            "DELETE personas WHERE owner_id = $u".to_string(),
            "DELETE memories WHERE owner_id = $u".to_string(),
            "DELETE lorebook_entries WHERE owner_id = $u".to_string(),
            "DELETE image_presets WHERE owner_id = $u".to_string(),
            "DELETE provider_configs WHERE owner_id = $u".to_string(),
        ];
        for sql in statements {
            let mut attempt = 0;
            loop {
                match rows(db, &sql, serde_json::json!({ "u": user_id })).await {
                    Ok(_) => break,
                    // Cascading deletes can hit a retryable write conflict.
                    Err(e) if e.to_string().contains("can be retried") && attempt < 3 => {
                        attempt += 1;
                        tokio::time::sleep(std::time::Duration::from_millis(150 * attempt)).await;
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(())
    }
}

impl OwnershipRepo {
    /// Owners of every row that references a stored file by its relative
    /// path: avatars/portraits (characters, personas), scene files, and chat
    /// attachments (via the message's conversation).
    pub async fn owners_of_file(
        db: &Surreal<Db>,
        relative: &str,
    ) -> Result<Vec<String>, MythicError> {
        let mut owners = Vec::new();
        for sql in [
            "SELECT VALUE owner_id FROM characters WHERE avatar_path = $p",
            "SELECT VALUE owner_id FROM personas WHERE avatar_path = $p",
            "SELECT VALUE conversation_id.owner_id FROM scenes WHERE file_path = $p",
            "SELECT VALUE conversation_id.owner_id FROM messages WHERE metadata.attachments.relativePath CONTAINS $p",
        ] {
            let found: Vec<serde_json::Value> =
                from_value_vec(rows(db, sql, serde_json::json!({ "p": relative })).await?)?;
            owners.extend(found.into_iter().filter_map(|v| v.as_str().map(str::to_string)));
        }
        Ok(owners)
    }

    /// Every stored file an account's rows reference, as relative paths, so
    /// they can be removed along with the account.
    pub async fn files_of_owner(
        db: &Surreal<Db>,
        user_id: &str,
    ) -> Result<Vec<String>, MythicError> {
        let mut files = Vec::new();
        for sql in [
            "SELECT VALUE avatar_path FROM characters WHERE owner_id = $u AND avatar_path != NONE",
            "SELECT VALUE avatar_path FROM personas WHERE owner_id = $u AND avatar_path != NONE",
            "SELECT VALUE file_path FROM scenes WHERE conversation_id.owner_id = $u",
        ] {
            let found: Vec<serde_json::Value> =
                from_value_vec(rows(db, sql, serde_json::json!({ "u": user_id })).await?)?;
            files.extend(
                found
                    .into_iter()
                    .filter_map(|v| v.as_str().map(str::to_string)),
            );
        }
        let metas: Vec<serde_json::Value> = from_value_vec(
            rows(
                db,
                "SELECT VALUE metadata.attachments.relativePath FROM messages WHERE conversation_id.owner_id = $u AND metadata.attachments != NONE",
                serde_json::json!({ "u": user_id }),
            )
            .await?,
        )?;
        for m in metas {
            if let Some(arr) = m.as_array() {
                files.extend(arr.iter().filter_map(|v| v.as_str().map(str::to_string)));
            }
        }
        files.sort();
        files.dedup();
        Ok(files)
    }
}

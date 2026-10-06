use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Member,
}

/// Who may create accounts once the first (admin) account exists.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SignupMode {
    /// Anyone who can reach Janus may create a member account.
    Open,
    /// Only the admin creates accounts.
    AdminOnly,
}

impl SignupMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SignupMode::Open => "open",
            SignupMode::AdminOnly => "admin_only",
        }
    }
}

/// A `users` row as stored, including the hashes. Never sent to the frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct DbUser {
    /// Plain-string copy of the record key (`uid` column), so rows never
    /// have to be parsed out of SurrealDB's `table:key` record-id form.
    #[serde(rename = "uid")]
    pub id: String,
    pub username: String,
    pub role: Role,
    pub passphrase_hash: String,
    pub recovery_hash: String,
    #[serde(default)]
    pub failed_attempts: u32,
    #[serde(default)]
    pub locked_until: i64,
    #[serde(default)]
    pub must_change: bool,
    #[serde(default)]
    pub created_at: String,
}

/// A `sessions` row as stored.
#[derive(Debug, Clone, Deserialize)]
pub struct DbSession {
    #[serde(rename = "uid")]
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub trusted: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub last_seen: i64,
    pub expires_at: i64,
}

/// The account as the frontend sees it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub role: Role,
    /// True after an admin created the account with a temporary passphrase;
    /// the user must choose their own before doing anything else.
    pub must_change: bool,
    pub created_at: String,
}

impl From<&DbUser> for UserInfo {
    fn from(u: &DbUser) -> Self {
        UserInfo {
            id: u.id.clone(),
            username: u.username.clone(),
            role: u.role,
            must_change: u.must_change,
            created_at: u.created_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct SessionInfo {
    pub id: String,
    pub label: String,
    pub trusted: bool,
    pub created_at: String,
    pub last_seen: i64,
    pub expires_at: i64,
    pub current: bool,
}

/// What the sign-in page needs to know before anyone has typed anything.
#[derive(Debug, Clone, Serialize, Type)]
pub struct AuthStatus {
    /// False until the first account exists; the first account becomes admin.
    pub has_users: bool,
    pub signup_mode: SignupMode,
    /// The signed-in account, if any.
    pub user: Option<UserInfo>,
}

/// Returned by sign-in / sign-up on the network path. The token is shown to
/// the client once; only its hash is stored.
#[derive(Debug, Clone, Serialize, Type)]
pub struct AuthResult {
    pub user: UserInfo,
    pub token: String,
    /// Present when a new recovery key was just issued (sign-up, reset).
    pub recovery_key: Option<String>,
}

/// Returned by the desktop's account commands, which need no token.
#[derive(Debug, Clone, Serialize, Type)]
pub struct AccountCreated {
    pub user: UserInfo,
    pub recovery_key: String,
}

pub const DEFAULT_WEB_PORT: u16 = 1421;

/// Browser access to this Janus. Off until the admin turns it on; when on it
/// listens on loopback only unless `lan` is set.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct NetworkSettings {
    pub enabled: bool,
    pub lan: bool,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct WebStatus {
    pub settings: NetworkSettings,
    pub running: bool,
    /// The address the server is actually bound to, when running.
    pub bound: Option<String>,
}

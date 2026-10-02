//! Accounts and sessions (issue #94, first piece of #14's LAN/web access).
//!
//! This module holds the pure building blocks: credential validation,
//! argon2id hashing, session-token and recovery-key generation. Nothing here
//! touches the database; `db::users` stores the results and `service` ties
//! them together into the sign-up / sign-in / reset flows.

pub mod access;
pub mod service;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::Engine as _;
use sha2::{Digest, Sha256};

use crate::error::MythicError;

pub const MIN_PASSPHRASE_LEN: usize = 12;
pub const MAX_PASSPHRASE_LEN: usize = 256;
/// Wrong passphrases in a row before the account locks.
pub const MAX_FAILED_ATTEMPTS: u32 = 3;
pub const LOCKOUT_SECS: i64 = 300;
/// "Keep this device signed in" lifetime.
pub const TRUSTED_SESSION_DAYS: i64 = 30;
/// Lifetime of a session the user did not ask to keep.
pub const SHORT_SESSION_HOURS: i64 = 12;

const KEY_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const KEY_LEN: usize = 16;

/// Trims and lowercases a username and checks it is 3 to 24 characters of
/// letters, digits, `.`, `_` or `-`, starting with a letter or digit.
pub fn normalize_username(raw: &str) -> Result<String, MythicError> {
    let u = raw.trim().to_lowercase();
    let ok_len = (3..=24).contains(&u.len());
    let mut chars = u.chars();
    let ok_first = chars
        .next()
        .map(|c| c.is_ascii_alphanumeric())
        .unwrap_or(false);
    let ok_rest = u
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok_len && ok_first && ok_rest {
        Ok(u)
    } else {
        Err(MythicError::Validation(
            "Username must be 3 to 24 letters, numbers, . _ or -".to_string(),
        ))
    }
}

pub fn validate_passphrase(p: &str) -> Result<(), MythicError> {
    let n = p.chars().count();
    if n < MIN_PASSPHRASE_LEN {
        return Err(MythicError::Validation(format!(
            "Passphrase must be at least {MIN_PASSPHRASE_LEN} characters"
        )));
    }
    if n > MAX_PASSPHRASE_LEN {
        return Err(MythicError::Validation(format!(
            "Passphrase must be at most {MAX_PASSPHRASE_LEN} characters"
        )));
    }
    Ok(())
}

/// Hashes a passphrase or recovery key with argon2id and a fresh random salt.
pub fn hash_secret(secret: &str) -> Result<String, MythicError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| MythicError::Config(format!("Failed to hash secret: {e}")))
}

pub fn verify_secret(secret: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(secret.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// Burns the same time as a real verification, so a sign-in attempt for a
/// username that does not exist is not distinguishable by response time.
pub fn dummy_verify(secret: &str) {
    use std::sync::OnceLock;
    static DUMMY: OnceLock<String> = OnceLock::new();
    let hash = DUMMY.get_or_init(|| hash_secret("janus-dummy-secret").unwrap_or_default());
    let _ = verify_secret(secret, hash);
}

/// A fresh opaque session token (256 random bits, URL-safe). Only its hash
/// is stored; the token itself is handed to the client once.
pub fn new_session_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// `JANUS-XXXX-XXXX-XXXX-XXXX`, 16 characters from an alphabet without
/// look-alikes (no 0/O, 1/I/L).
pub fn generate_recovery_key() -> String {
    let limit = 256 - (256 % KEY_ALPHABET.len());
    let mut out = String::with_capacity(KEY_LEN);
    let mut buf = [0u8; 32];
    while out.len() < KEY_LEN {
        OsRng.fill_bytes(&mut buf);
        for &b in &buf {
            if out.len() == KEY_LEN {
                break;
            }
            if (b as usize) < limit {
                out.push(KEY_ALPHABET[b as usize % KEY_ALPHABET.len()] as char);
            }
        }
    }
    format_recovery_key(&out)
}

fn format_recovery_key(raw: &str) -> String {
    let groups: Vec<&str> = raw
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    format!("JANUS-{}", groups.join("-"))
}

/// Accepts a recovery key however it was typed (case, spaces, dashes, with or
/// without the `JANUS` prefix) and returns the canonical form, or `None` if
/// it does not have 16 key characters.
pub fn normalize_recovery_key(input: &str) -> Option<String> {
    let mut raw: String = input
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if let Some(rest) = raw.strip_prefix("JANUS") {
        raw = rest.to_string();
    }
    if raw.len() == KEY_LEN {
        Some(format_recovery_key(&raw))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert_eq!(normalize_username("  Ramish ").unwrap(), "ramish");
        assert_eq!(normalize_username("a.b_c-d9").unwrap(), "a.b_c-d9");
        assert!(normalize_username("ab").is_err());
        assert!(normalize_username(&"x".repeat(25)).is_err());
        assert!(normalize_username(".dot").is_err());
        assert!(normalize_username("has space").is_err());
        assert!(normalize_username("emoji\u{1F600}x").is_err());
    }

    #[test]
    fn passphrase_length() {
        assert!(validate_passphrase("short").is_err());
        assert!(validate_passphrase("twelve chars").is_ok());
        assert!(validate_passphrase(&"p".repeat(MAX_PASSPHRASE_LEN + 1)).is_err());
    }

    #[test]
    fn hashing_roundtrip() {
        let h = hash_secret("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_secret("correct horse battery", &h));
        assert!(!verify_secret("wrong", &h));
        assert!(!verify_secret("anything", "not-a-hash"));
        assert_ne!(h, hash_secret("correct horse battery").unwrap());
    }

    #[test]
    fn tokens() {
        let a = new_session_token();
        let b = new_session_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert_eq!(hash_token(&a).len(), 64);
        assert_eq!(hash_token(&a), hash_token(&a));
        assert_ne!(hash_token(&a), hash_token(&b));
    }

    #[test]
    fn recovery_keys() {
        let k = generate_recovery_key();
        assert_eq!(k.len(), 6 + 16 + 3);
        assert_eq!(normalize_recovery_key(&k).as_deref(), Some(k.as_str()));
        let sloppy = k.to_lowercase().replace('-', " ");
        assert_eq!(normalize_recovery_key(&sloppy).as_deref(), Some(k.as_str()));
        assert!(normalize_recovery_key("JANUS-ABCD").is_none());
        assert!(!k.contains('0') && !k.contains('O') && !k.contains('1') && !k.contains('I'));
    }
}

//! Keeps secrets (provider API keys) unreadable at rest (issue #227).
//!
//! A single random 256-bit master key encrypts each secret with
//! XChaCha20-Poly1305 before it is written to the database. The master key
//! lives in the operating system's keychain (Windows Credential Manager, macOS
//! Keychain, Linux Secret Service). Where there is no keychain, it lives in
//! `secret.key` in the app data folder, readable by the current user only, and
//! once that file exists it is always used, so the key never moves between
//! stores and strands what it encrypted.
//!
//! A stored secret looks like `enc:v1:<base64 of nonce + ciphertext>`. It is
//! tied to the row it belongs to (the row id is authenticated along with it),
//! so a ciphertext copied onto another row does not decrypt. Values without
//! the prefix are read as plain text, which is how older databases load until
//! `protect_existing` has run.
//!
//! Backups carry the encrypted form, so a backup restored on another computer
//! has keys that cannot be read there; they show up as needing to be entered
//! again.

use std::path::Path;
use std::sync::OnceLock;

use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

use crate::error::MythicError;

const PREFIX: &str = "enc:v1:";
const KEYCHAIN_SERVICE: &str = "com.janus.app";
const KEYCHAIN_USER: &str = "master-key";
const KEY_FILE: &str = "secret.key";
const NONCE_LEN: usize = 24;

/// What the masked form of a key starts with. No real key does.
pub const MASK_PREFIX: &str = "\u{2022}\u{2022}\u{2022}\u{2022}";

/// Where the master key came from, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Keychain,
    File,
    /// Made fresh in memory and never stored: used only when the app has not
    /// set the store up, which is the case in tests.
    Ephemeral,
}

struct Vault {
    key: [u8; 32],
    source: KeySource,
}

static VAULT: OnceLock<Result<Vault, String>> = OnceLock::new();

fn random_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).expect("the operating system's random source is unavailable");
    key
}

fn decode_key(text: &str) -> Option<[u8; 32]> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .ok()?;
    bytes.try_into().ok()
}

fn encode_key(key: &[u8; 32]) -> String {
    base64::engine::general_purpose::STANDARD.encode(key)
}

fn keychain_key() -> Result<[u8; 32], String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(text) => decode_key(&text).ok_or_else(|| "the stored key is damaged".to_string()),
        Err(keyring::Error::NoEntry) => {
            let key = random_key();
            entry
                .set_password(&encode_key(&key))
                .map_err(|e| e.to_string())?;
            // Some systems accept the write and then cannot read it back.
            match entry.get_password() {
                Ok(text) if decode_key(&text) == Some(key) => Ok(key),
                _ => Err("the keychain did not keep the key".to_string()),
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(text.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, text)
    }
}

fn file_key(dir: &Path) -> Result<[u8; 32], String> {
    let path = dir.join(KEY_FILE);
    if let Ok(text) = std::fs::read_to_string(&path) {
        return decode_key(&text).ok_or_else(|| format!("{} is damaged", path.display()));
    }
    let key = random_key();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    write_private(&path, &encode_key(&key)).map_err(|e| e.to_string())?;
    Ok(key)
}

fn open_vault(data_dir: &Path) -> Result<Vault, String> {
    // A key file that already exists wins, so the key never switches stores.
    if data_dir.join(KEY_FILE).exists() {
        return file_key(data_dir).map(|key| Vault {
            key,
            source: KeySource::File,
        });
    }
    match keychain_key() {
        Ok(key) => Ok(Vault {
            key,
            source: KeySource::Keychain,
        }),
        Err(why) => {
            tracing::warn!("[secrets] no usable keychain ({why}); using a key file instead");
            file_key(data_dir).map(|key| Vault {
                key,
                source: KeySource::File,
            })
        }
    }
}

/// Sets up the master key. Call once at startup, before the database is used.
/// If it fails, saving a secret returns an error and reading an encrypted one
/// fails, rather than quietly storing anything in the clear.
pub fn init(data_dir: &Path) -> Result<KeySource, String> {
    match VAULT.get_or_init(|| open_vault(data_dir)) {
        Ok(v) => Ok(v.source),
        Err(why) => Err(why.clone()),
    }
}

fn vault() -> Result<&'static Vault, MythicError> {
    VAULT
        .get_or_init(|| {
            Ok(Vault {
                key: random_key(),
                source: KeySource::Ephemeral,
            })
        })
        .as_ref()
        .map_err(|why| {
            MythicError::Config(format!(
                "Janus could not open its key store, so API keys cannot be saved or read ({why})."
            ))
        })
}

pub fn is_encrypted(stored: &str) -> bool {
    stored.starts_with(PREFIX)
}

/// Encrypts `plain` for the row named by `context`.
pub fn encrypt(plain: &str, context: &str) -> Result<String, MythicError> {
    let v = vault()?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&v.key));
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).expect("the operating system's random source is unavailable");
    let sealed = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plain.as_bytes(),
                aad: context.as_bytes(),
            },
        )
        .map_err(|_| MythicError::Config("Could not encrypt the key.".to_string()))?;
    let mut out = nonce.to_vec();
    out.extend(sealed);
    Ok(format!(
        "{PREFIX}{}",
        base64::engine::general_purpose::STANDARD.encode(out)
    ))
}

/// Reads a stored secret. A value without the `enc:v1:` prefix is plain text
/// from before encryption and is returned as it is. An encrypted value that
/// this computer's key cannot open (a backup from another computer, or a lost
/// key) is an error.
pub fn decrypt(stored: &str, context: &str) -> Result<String, MythicError> {
    let Some(body) = stored.strip_prefix(PREFIX) else {
        return Ok(stored.to_string());
    };
    let unreadable = || MythicError::Config("The stored key cannot be read here.".to_string());
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|_| unreadable())?;
    if bytes.len() < NONCE_LEN {
        return Err(unreadable());
    }
    let (nonce, sealed) = bytes.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&vault()?.key));
    let plain = cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: sealed,
                aad: context.as_bytes(),
            },
        )
        .map_err(|_| unreadable())?;
    String::from_utf8(plain).map_err(|_| unreadable())
}

/// What screens and the browser see instead of a key: four bullets and the
/// last four characters, or only bullets for a short one.
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() < 12 {
        return format!("{MASK_PREFIX}{MASK_PREFIX}");
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{MASK_PREFIX}{tail}")
}

/// True for a value that is the masked form of a key, which a screen sends
/// back unchanged when the user did not retype it.
pub fn is_mask(value: &str) -> bool {
    value.starts_with(MASK_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_is_not_plain_text() {
        let sealed = encrypt("sk-secret-value", "provider:a").unwrap();
        assert!(is_encrypted(&sealed));
        assert!(!sealed.contains("sk-secret-value"));
        assert_eq!(decrypt(&sealed, "provider:a").unwrap(), "sk-secret-value");
    }

    #[test]
    fn the_same_key_encrypts_differently_each_time() {
        let a = encrypt("same", "provider:a").unwrap();
        let b = encrypt("same", "provider:a").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn a_ciphertext_does_not_open_on_another_row_or_when_altered() {
        let sealed = encrypt("sk-secret-value", "provider:a").unwrap();
        assert!(decrypt(&sealed, "provider:b").is_err());
        let mut bytes = sealed.into_bytes();
        let last = bytes.len() - 3;
        bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
        assert!(decrypt(&String::from_utf8(bytes).unwrap(), "provider:a").is_err());
        assert!(decrypt("enc:v1:not-base64!!", "provider:a").is_err());
        assert!(decrypt("enc:v1:AAAA", "provider:a").is_err());
    }

    #[test]
    fn plain_text_from_before_encryption_reads_as_it_is() {
        assert_eq!(decrypt("sk-old", "provider:a").unwrap(), "sk-old");
        assert_eq!(decrypt("", "provider:a").unwrap(), "");
    }

    #[test]
    fn masks_show_only_the_end_of_a_long_key() {
        assert_eq!(
            mask("sk-or-v1-abcdefghijWXYZ"),
            format!("{MASK_PREFIX}WXYZ")
        );
        assert_eq!(mask("short"), format!("{MASK_PREFIX}{MASK_PREFIX}"));
        assert!(is_mask(&mask("sk-or-v1-abcdefghijWXYZ")));
        assert!(!is_mask("sk-or-v1-abcdefghijWXYZ"));
    }

    #[test]
    fn a_key_file_is_made_once_and_reused() {
        let dir = std::env::temp_dir().join(format!("janus-keyfile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = file_key(&dir).unwrap();
        let b = file_key(&dir).unwrap();
        assert_eq!(a, b);
        std::fs::write(dir.join(KEY_FILE), "not a key").unwrap();
        assert!(file_key(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! A self-signed certificate for HTTPS browser access (issue #223).
//!
//! Janus makes its own certificate the first time HTTPS is turned on and keeps
//! it in `<app data>/tls/`. It names `localhost`, the loopback addresses, this
//! computer's name and its current network addresses, and is made again when
//! those change or the certificate is close to expiring. A browser warns about
//! it once per device, because no authority vouches for it; the fingerprint
//! shown in Settings lets you check you are looking at the right one.

use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use rcgen::{CertificateParams, DnType, KeyPair, SanType};
use sha2::{Digest, Sha256};
use tokio_rustls::TlsAcceptor;

use crate::error::MythicError;

const CERT_FILE: &str = "cert.pem";
const KEY_FILE: &str = "key.pem";
/// The names the stored certificate was made for, and when it stops being valid.
const META_FILE: &str = "meta.txt";
/// Under 825 days, the longest Apple's browsers accept for a certificate.
const VALID_DAYS: i64 = 800;
/// Make a new one when fewer than this many days remain.
const RENEW_BEFORE_DAYS: i64 = 30;

pub struct Tls {
    pub acceptor: TlsAcceptor,
    /// SHA-256 of the certificate, as `AB:CD:…`, for checking it by eye.
    pub fingerprint: String,
}

/// This computer's name, when the system gives one.
fn computer_name() -> Option<String> {
    ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok())
        .map(|n| n.trim().to_ascii_lowercase())
        .filter(|n| {
            !n.is_empty()
                && n.len() < 64
                && n.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        })
}

/// Everything the certificate should answer to, sorted so equal sets compare equal.
fn wanted_names(lan_ips: &[String]) -> Vec<String> {
    let mut names = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    names.extend(computer_name());
    names.extend(
        lan_ips
            .iter()
            .filter(|ip| ip.parse::<IpAddr>().is_ok())
            .cloned(),
    );
    names.sort();
    names.dedup();
    names
}

fn fingerprint_of(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// The stored certificate, if it still names `names` and has time left.
fn stored(dir: &Path, names: &[String]) -> Option<(String, String)> {
    let meta = std::fs::read_to_string(dir.join(META_FILE)).ok()?;
    let mut lines = meta.lines();
    let expires: i64 = lines.next()?.parse().ok()?;
    let saved: Vec<&str> = lines.collect();
    let same = saved.len() == names.len() && saved.iter().zip(names).all(|(a, b)| a == b);
    if !same || expires - now() < RENEW_BEFORE_DAYS * 86_400 {
        return None;
    }
    Some((
        std::fs::read_to_string(dir.join(CERT_FILE)).ok()?,
        std::fs::read_to_string(dir.join(KEY_FILE)).ok()?,
    ))
}

fn make(dir: &Path, names: &[String]) -> Result<(String, String), MythicError> {
    let fail =
        |e: rcgen::Error| MythicError::Config(format!("Could not make the HTTPS certificate: {e}"));
    let mut params = CertificateParams::default();
    params.subject_alt_names = names
        .iter()
        .map(|n| match n.parse::<IpAddr>() {
            Ok(ip) => Ok(SanType::IpAddress(ip)),
            Err(_) => n.clone().try_into().map(SanType::DnsName),
        })
        .collect::<Result<_, _>>()
        .map_err(fail)?;
    params.distinguished_name.push(DnType::CommonName, "Janus");
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Janus (self-signed)");
    let from = time::OffsetDateTime::now_utc() - time::Duration::days(1);
    params.not_before = from;
    params.not_after = from + time::Duration::days(VALID_DAYS);
    let key = KeyPair::generate().map_err(fail)?;
    let cert = params.self_signed(&key).map_err(fail)?;
    let (cert_pem, key_pem) = (cert.pem(), key.serialize_pem());

    std::fs::create_dir_all(dir)?;
    write_private(&dir.join(KEY_FILE), &key_pem)?;
    std::fs::write(dir.join(CERT_FILE), &cert_pem)?;
    let expires = params.not_after.unix_timestamp();
    std::fs::write(
        dir.join(META_FILE),
        format!("{expires}\n{}", names.join("\n")),
    )?;
    Ok((cert_pem, key_pem))
}

/// Writes the private key readable by this user only where the system supports it.
fn write_private(path: &Path, text: &str) -> Result<(), MythicError> {
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
        f.write_all(text.as_bytes())?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, text)?;
        Ok(())
    }
}

/// Loads the stored certificate, or makes a new one, and builds the TLS acceptor.
pub fn load_or_create(app_data: &Path, lan_ips: &[String]) -> Result<Tls, MythicError> {
    let dir = app_data.join("tls");
    let names = wanted_names(lan_ips);
    let (cert_pem, key_pem) = match stored(&dir, &names) {
        Some(found) => found,
        None => make(&dir, &names)?,
    };
    acceptor_from_pem(&cert_pem, &key_pem)
}

fn acceptor_from_pem(cert_pem: &str, key_pem: &str) -> Result<Tls, MythicError> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    let bad = |what: &str| {
        MythicError::Config(format!(
            "The stored HTTPS certificate is unreadable ({what})."
        ))
    };
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem.as_bytes())
        .collect::<Result<_, _>>()
        .map_err(|_| bad("certificate"))?;
    let leaf = chain.first().ok_or_else(|| bad("certificate"))?;
    let fingerprint = fingerprint_of(leaf.as_ref());
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).map_err(|_| bad("key"))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| MythicError::Config(format!("HTTPS setup failed: {e}")))?
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(|e| MythicError::Config(format!("HTTPS setup failed: {e}")))?;
    Ok(Tls {
        acceptor: TlsAcceptor::from(Arc::new(config)),
        fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("janus-tls-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn makes_a_certificate_and_keeps_it_while_the_names_stay_the_same() {
        let dir = temp_dir("keep");
        let ips = vec!["192.168.1.20".to_string()];
        let first = load_or_create(&dir, &ips).unwrap();
        let again = load_or_create(&dir, &ips).unwrap();
        assert_eq!(first.fingerprint, again.fingerprint);
        assert_eq!(first.fingerprint.split(':').count(), 32);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_network_address_gets_a_new_certificate() {
        let dir = temp_dir("renew");
        let a = load_or_create(&dir, &["192.168.1.20".to_string()]).unwrap();
        let b =
            load_or_create(&dir, &["192.168.1.20".to_string(), "10.0.0.5".to_string()]).unwrap();
        assert_ne!(a.fingerprint, b.fingerprint);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_certificate_close_to_expiry_is_replaced() {
        let dir = temp_dir("expiry");
        let ips: Vec<String> = vec![];
        let a = load_or_create(&dir, &ips).unwrap();
        let meta_path = dir.join("tls").join(META_FILE);
        let meta = std::fs::read_to_string(&meta_path).unwrap();
        let rest = meta.split_once('\n').unwrap().1;
        std::fs::write(&meta_path, format!("{}\n{rest}", now() + 86_400)).unwrap();
        let b = load_or_create(&dir, &ips).unwrap();
        assert_ne!(a.fingerprint, b.fingerprint);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_that_are_not_addresses_are_left_out() {
        let names = wanted_names(&["not-an-ip".to_string(), "192.168.0.9".to_string()]);
        assert!(names.contains(&"192.168.0.9".to_string()));
        assert!(!names.contains(&"not-an-ip".to_string()));
        assert!(names.contains(&"localhost".to_string()));
    }
}

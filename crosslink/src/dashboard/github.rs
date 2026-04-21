//! GitHub integration — PAT storage + org enumeration (design doc §14
//! Phase 4).
//!
//! Tokens are stored encrypted (AES-256-GCM) in the dashboard DB's
//! `config` table under the key `github.token`. The encryption key is
//! derived from a stable per-machine secret:
//!
//! - On Unix: `/etc/machine-id` (or `/var/lib/dbus/machine-id`)
//! - On macOS / other: the user's username + `hostname` hash
//! - Fallback: a random key persisted to `~/.crosslink/.dashboard-key`
//!
//! This is **obfuscation against a casual read**, not protection
//! against an attacker with full disk access — the key material is
//! derivable from the same machine. The real protection is the file
//! permissions on `~/.crosslink/` and on the DB itself. We document
//! this posture in the design doc rather than pretending otherwise.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rusqlite::params;
use sha2::{Digest, Sha256};

use super::db::DashboardDb;

/// Config key under which the encrypted token lives.
const KEY_TOKEN: &str = "github.token";
/// Config key for the user's default org (no encryption — plaintext
/// identifier).
pub const KEY_DEFAULT_ORG: &str = "github.default_org";

/// On-disk wrapper around an encrypted blob. Encoded as JSON (under a
/// base64 config value) so we can bump the version or tweak the nonce
/// layout without a schema migration.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Sealed {
    /// Format version — lets us evolve without breaking old rows.
    v: u32,
    /// Base64-encoded 12-byte AES-GCM nonce.
    nonce: String,
    /// Base64-encoded ciphertext (includes GCM tag).
    ct: String,
}

/// Derive the 32-byte AES key for this machine. SHA-256 of
/// (machine_id || username || "crosslink-dashboard-pat-v1"). If
/// `/etc/machine-id` isn't readable, falls back to hostname and
/// finally a random key persisted alongside the DB.
fn derive_machine_key(db_path: &std::path::Path) -> [u8; 32] {
    let mut h = Sha256::new();
    if let Ok(mid) = std::fs::read_to_string("/etc/machine-id") {
        h.update(mid.trim().as_bytes());
    } else if let Ok(mid) = std::fs::read_to_string("/var/lib/dbus/machine-id") {
        h.update(mid.trim().as_bytes());
    } else {
        // Hostname as a weaker fallback.
        if let Ok(hn) = std::env::var("HOSTNAME") {
            h.update(hn.as_bytes());
        } else if let Ok(out) = std::process::Command::new("hostname").output() {
            h.update(&out.stdout);
        }
    }
    if let Ok(user) = std::env::var("USER") {
        h.update(user.as_bytes());
    }
    h.update(b"crosslink-dashboard-pat-v1");

    // Random fallback file — if neither machine-id nor user landed
    // meaningful bytes, mix in a persisted random key so subsequent
    // encrypts/decrypts are self-consistent.
    let fallback_path = db_path.with_file_name(".dashboard-key");
    let fallback = match std::fs::read(&fallback_path) {
        Ok(b) if b.len() >= 32 => b,
        _ => {
            let mut buf = [0u8; 32];
            #[cfg(unix)]
            {
                // /dev/urandom is universally available on Unix.
                if let Ok(bytes) = std::fs::read("/dev/urandom") {
                    if bytes.len() >= 32 {
                        buf.copy_from_slice(&bytes[..32]);
                    }
                }
            }
            let _ = std::fs::write(&fallback_path, buf);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    &fallback_path,
                    std::fs::Permissions::from_mode(0o600),
                );
            }
            buf.to_vec()
        }
    };
    h.update(&fallback);

    let digest = h.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

/// Encrypt `plaintext` with the machine-derived key and return the
/// base64-encoded sealed blob.
///
/// # Errors
/// Returns an error if the AES-GCM cipher can't be constructed or the
/// encryption itself fails (both are practically infallible for valid
/// keys, but we surface the error for completeness).
pub fn seal(plaintext: &str, db_path: &std::path::Path) -> Result<String> {
    let key_bytes = derive_machine_key(db_path);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 12-byte nonce from the first bytes of /dev/urandom. AES-GCM
    // nonces must be unique; we rely on each encrypt generating fresh
    // bytes (single-writer dashboard, not a collision concern).
    let mut nonce_bytes = [0u8; 12];
    #[cfg(unix)]
    {
        if let Ok(bytes) = std::fs::read("/dev/urandom") {
            if bytes.len() >= 12 {
                nonce_bytes.copy_from_slice(&bytes[..12]);
            }
        }
    }
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("aes-gcm encrypt: {e}"))?;

    let sealed = Sealed {
        v: 1,
        nonce: B64.encode(nonce_bytes),
        ct: B64.encode(&ct),
    };
    let json = serde_json::to_string(&sealed).context("serialize sealed blob")?;
    Ok(B64.encode(json))
}

/// Decrypt a blob produced by [`seal`]. Returns `None` if the value
/// can't be parsed or authenticated (wrong key, tampered DB, etc.) —
/// callers treat that as "no token configured" rather than erroring
/// loudly.
pub fn unseal(value: &str, db_path: &std::path::Path) -> Option<String> {
    let json = B64.decode(value).ok()?;
    let sealed: Sealed = serde_json::from_slice(&json).ok()?;
    if sealed.v != 1 {
        return None;
    }
    let nonce_bytes = B64.decode(sealed.nonce).ok()?;
    let ct = B64.decode(sealed.ct).ok()?;
    let key_bytes = derive_machine_key(db_path);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let pt = cipher.decrypt(Nonce::from_slice(&nonce_bytes), ct.as_ref()).ok()?;
    String::from_utf8(pt).ok()
}

/// Persist a GitHub PAT. Pass an empty string to delete.
///
/// # Errors
/// Returns an error for DB failures or encryption failures.
pub fn set_token(db: &DashboardDb, token: &str, db_path: &std::path::Path) -> Result<()> {
    if token.is_empty() {
        db.conn.execute(
            "DELETE FROM config WHERE key = ?1",
            params![KEY_TOKEN],
        )?;
        return Ok(());
    }
    let sealed = seal(token, db_path)?;
    db.conn.execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![KEY_TOKEN, sealed],
    )?;
    Ok(())
}

/// Retrieve the stored GitHub PAT, if any. Malformed / undecryptable
/// rows are returned as `None`.
///
/// # Errors
/// Returns an error only on DB access failure.
pub fn get_token(db: &DashboardDb, db_path: &std::path::Path) -> Result<Option<String>> {
    let value: rusqlite::Result<String> = db.conn.query_row(
        "SELECT value FROM config WHERE key = ?1",
        params![KEY_TOKEN],
        |row| row.get(0),
    );
    let raw = match value {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    Ok(unseal(&raw, db_path))
}

/// Set or delete a plain-text config value (used for non-secret
/// fields like `github.default_org`).
///
/// # Errors
/// Returns an error only on DB access failure.
pub fn set_plain(db: &DashboardDb, key: &str, value: Option<&str>) -> Result<()> {
    if let Some(v) = value {
        db.conn.execute(
            "INSERT INTO config (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, v],
        )?;
    } else {
        db.conn.execute("DELETE FROM config WHERE key = ?1", params![key])?;
    }
    Ok(())
}

/// Read a plain-text config value.
///
/// # Errors
/// Returns an error only on DB access failure.
pub fn get_plain(db: &DashboardDb, key: &str) -> Result<Option<String>> {
    match db.conn.query_row(
        "SELECT value FROM config WHERE key = ?1",
        params![key],
        |row| row.get(0),
    ) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn open_db() -> (tempfile::TempDir, std::path::PathBuf, DashboardDb) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dashboard.db");
        let db = DashboardDb::open(&path).unwrap();
        (dir, path, db)
    }

    #[test]
    fn test_seal_roundtrip() {
        let (_dir, path, _db) = open_db();
        let sealed = seal("ghp_test_token", &path).unwrap();
        assert_ne!(sealed, "ghp_test_token");
        let round = unseal(&sealed, &path).unwrap();
        assert_eq!(round, "ghp_test_token");
    }

    #[test]
    fn test_set_get_token() {
        let (_dir, path, db) = open_db();
        assert!(get_token(&db, &path).unwrap().is_none());
        set_token(&db, "ghp_xyz", &path).unwrap();
        assert_eq!(get_token(&db, &path).unwrap().as_deref(), Some("ghp_xyz"));
    }

    #[test]
    fn test_set_empty_token_deletes() {
        let (_dir, path, db) = open_db();
        set_token(&db, "ghp_xyz", &path).unwrap();
        set_token(&db, "", &path).unwrap();
        assert!(get_token(&db, &path).unwrap().is_none());
    }

    #[test]
    fn test_plain_config_roundtrip() {
        let (_dir, _path, db) = open_db();
        assert!(get_plain(&db, KEY_DEFAULT_ORG).unwrap().is_none());
        set_plain(&db, KEY_DEFAULT_ORG, Some("forecast-bio")).unwrap();
        assert_eq!(
            get_plain(&db, KEY_DEFAULT_ORG).unwrap().as_deref(),
            Some("forecast-bio")
        );
        set_plain(&db, KEY_DEFAULT_ORG, None).unwrap();
        assert!(get_plain(&db, KEY_DEFAULT_ORG).unwrap().is_none());
    }

    #[test]
    fn test_unseal_rejects_garbage() {
        let (_dir, path, _db) = open_db();
        assert!(unseal("not-base64!!", &path).is_none());
        assert!(unseal(&B64.encode("not-json"), &path).is_none());
    }
}

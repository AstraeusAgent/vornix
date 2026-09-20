//! The secrets vault and its storage backends.
//!
//! [`SecretsVault`] is the public entry point. Call [`SecretsVault::initialize`]
//! to obtain a vault backed by the best available store.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Known provider key names that Vornix manages.
pub const PROVIDER_KEYS: &[&str] = &[
    "openrouter_api_key",
    "opencode_go_api_key",
    "github_token",
];

/// Service name used for all keyring entries.
const KEYRING_SERVICE: &str = "vornix";

/// Salt mixed into the machine-specific passphrase derivation.
const PASSPHRASE_SALT: &str = "vornix-secrets-v1";

// ─── Vault status ────────────────────────────────────────────────────────────

/// Status returned by [`SecretsVault::initialize`], indicating which
/// backend is in use.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VaultStatus {
    /// OS keychain (Keychain / Credential Manager / Secret Service) is
    /// available and tested.
    KeyringAvailable,
    /// Fell back to an age-encrypted file.
    FallbackEncryptedFile {
        /// Path to the encrypted secrets file.
        path: PathBuf,
    },
    /// No backend could be established. All operations will fail.
    Degraded {
        /// Human-readable reason.
        reason: String,
    },
}

// ─── SecretBackend trait ─────────────────────────────────────────────────────

/// Async trait implemented by each storage backend.
#[async_trait]
pub trait SecretBackend: Send + Sync {
    /// Retrieve a secret by key. Returns `Ok(None)` when the key does
    /// not exist.
    async fn get(&self, key: &str) -> Result<Option<String>>;

    /// Store (or overwrite) a secret.
    async fn set(&self, key: &str, value: &str) -> Result<()>;

    /// Delete a secret. Succeeds even if the key was not present.
    async fn delete(&self, key: &str) -> Result<()>;

    /// Return the keys of all currently stored secrets.
    async fn list_keys(&self) -> Result<Vec<String>>;
}

// ─── SecretsVault ────────────────────────────────────────────────────────────

/// High-level secrets vault that delegates to the best available backend.
pub struct SecretsVault {
    pub(crate) backend: Box<dyn SecretBackend>,
}

impl SecretsVault {
    /// Probe available backends and return a working vault.
    ///
    /// The probe order is:
    /// 1. OS keyring — tested with a round-trip set / get / delete.
    /// 2. Encrypted file at `~/.vornix/secrets.enc`.
    /// 3. Degraded (all methods return errors).
    pub async fn initialize() -> Result<(Self, VaultStatus)> {
        if Self::probe_keyring().is_ok() {
            tracing::info!("OS keyring available — using keyring backend");
            return Ok((
                Self {
                    backend: Box::new(KeyringBackend),
                },
                VaultStatus::KeyringAvailable,
            ));
        }

        tracing::warn!("Keyring unavailable — attempting encrypted file fallback");
        match EncryptedFileBackend::new() {
            Ok((backend, path)) => {
                tracing::info!(
                    path = %path.display(),
                    "Using encrypted file backend"
                );
                Ok((
                    Self {
                        backend: Box::new(backend),
                    },
                    VaultStatus::FallbackEncryptedFile { path },
                ))
            }
            Err(e) => {
                tracing::error!("All secret backends failed: {e:#}");
                Ok((
                    Self {
                        backend: Box::new(DegradedBackend),
                    },
                    VaultStatus::Degraded {
                        reason: e.to_string(),
                    },
                ))
            }
        }
    }

    /// Retrieve a secret by key.
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        self.backend.get(key).await
    }

    /// Store (or overwrite) a secret.
    pub async fn set(&self, key: &str, value: &str) -> Result<()> {
        self.backend.set(key, value).await
    }

    /// Delete a secret. Succeeds even if the key was not present.
    pub async fn delete(&self, key: &str) -> Result<()> {
        self.backend.delete(key).await
    }

    /// Return the keys of all currently stored secrets.
    pub async fn list_keys(&self) -> Result<Vec<String>> {
        self.backend.list_keys().await
    }

    // ── Private helpers ──────────────────────────────────────────────────

    /// Test whether the OS keyring is functional by writing, reading,
    /// and deleting a probe entry.
    fn probe_keyring() -> Result<()> {
        let probe_key = format!("vornix/__vault_probe_{}", std::process::id());
        let entry = keyring::Entry::new(KEYRING_SERVICE, &probe_key)
            .context("Creating keyring probe entry")?;

        entry
            .set_password("probe")
            .context("Keyring probe: set_password failed")?;

        let retrieved = entry
            .get_password()
            .context("Keyring probe: get_password failed")?;

        if retrieved != "probe" {
            anyhow::bail!("Keyring probe: value mismatch");
        }

        entry
            .delete_credential()
            .context("Keyring probe: delete_credential failed")?;

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// KeyringBackend
// ═══════════════════════════════════════════════════════════════════════════════

/// Backend that stores secrets in the OS keychain.
///
/// - **macOS** — Keychain (`security-framework`).
/// - **Windows** — Credential Manager (`windows-sys`).
/// - **Linux** — Secret Service via D-Bus (`dbus-secret-service`).
///
/// Entry naming: service = `"vornix"`, user = `<key>`.
struct KeyringBackend;

impl KeyringBackend {
    fn entry(key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, key).context("Creating keyring entry")
    }
}

#[async_trait]
impl SecretBackend for KeyringBackend {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        match Self::entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn set(&self, key: &str, value: &str) -> Result<()> {
        Self::entry(key)?
            .set_password(value)
            .with_context(|| format!("Setting keyring password for '{key}'"))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e).with_context(|| format!("Deleting keyring entry for '{key}'")),
        }
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        // The keyring crate does not support enumeration, so we probe
        // every known provider key.
        let mut found = Vec::new();
        for &key in PROVIDER_KEYS {
            if let Ok(entry) = Self::entry(key) {
                if entry.get_password().is_ok() {
                    found.push(key.to_string());
                }
            }
        }
        Ok(found)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// EncryptedFileBackend
// ═══════════════════════════════════════════════════════════════════════════════

/// Fallback backend that stores all secrets in a single age-encrypted
/// JSON file at `~/.vornix/secrets.enc`.
///
/// **Key derivation:** a machine-specific passphrase is derived from
/// `SHA-256(hostname + ":" + username + ":" + PASSPHRASE_SALT)` and
/// fed to age's built-in scrypt KDF. This ties the file to the
/// current machine without requiring the user to remember a password.
///
/// **File format:** the inner JSON is a flat `HashMap<String, String>`.
/// The entire blob is encrypted/decrypted as a unit — acceptable for
/// fewer than ~20 secrets.
struct EncryptedFileBackend {
    path: PathBuf,
    passphrase: String,
    data: Mutex<HashMap<String, String>>,
}

impl EncryptedFileBackend {
    /// Construct a new backend, loading any existing encrypted file.
    fn new() -> Result<(Self, PathBuf)> {
        let home =
            dirs::home_dir().context("Could not determine home directory (dirs::home_dir)")?;
        let vornix_dir = home.join(".vornix");
        std::fs::create_dir_all(&vornix_dir)
            .with_context(|| format!("Creating {}", vornix_dir.display()))?;

        // Restrict directory permissions on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &vornix_dir,
                std::fs::Permissions::from_mode(0o700),
            );
        }

        let path = vornix_dir.join("secrets.enc");

        // Clean up leftover temp file from a possible prior crash.
        let tmp = path.with_extension("enc.tmp");
        if tmp.exists() {
            let _ = std::fs::remove_file(&tmp);
        }

        let passphrase = Self::derive_passphrase();

        let data = if path.exists() {
            match Self::load_from_file(&path, &passphrase) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(
                        "Could not decrypt secrets file: {e:#}. \
                         Backing up and starting fresh."
                    );
                    let backup = path.with_extension("enc.bak");
                    if let Err(b_err) = std::fs::copy(&path, &backup) {
                        tracing::error!("Backup failed: {b_err:#}");
                    } else {
                        tracing::info!("Backup at {}", backup.display());
                    }
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };

        let backend = Self {
            path: path.clone(),
            passphrase,
            data: Mutex::new(data),
        };
        Ok((backend, path))
    }

    /// Derive a deterministic passphrase from machine-specific inputs.
    fn derive_passphrase() -> String {
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "default-user".into());

        let host = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| {
                std::fs::read_to_string("/etc/hostname")
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| "default-host".into())
            });

        use sha2::Digest;
        let preimage = format!("{host}:{user}:{PASSPHRASE_SALT}");
        let hash = sha2::Sha256::digest(preimage.as_bytes());
        hex::encode(hash)
    }

    /// Decrypt and deserialize the secrets file.
    fn load_from_file(
        path: &std::path::Path,
        passphrase: &str,
    ) -> Result<HashMap<String, String>> {
        let ciphertext = std::fs::read(path)
            .with_context(|| format!("Reading {}", path.display()))?;

        if ciphertext.is_empty() {
            return Ok(HashMap::new());
        }

        let secret = age::secrecy::SecretString::from(passphrase.to_owned());
        let identity = age::scrypt::Identity::new(secret);
        let plaintext = age::decrypt(&identity, &ciphertext)
            .context("Decrypting secrets file (machine identity may have changed)")?;

        serde_json::from_slice(&plaintext).context("Parsing decrypted secrets as JSON")
    }

    /// Serialize, encrypt, and atomically write the secrets file.
    fn save_to_file(&self) -> Result<()> {
        let data = self.data.lock().expect("secrets mutex poisoned");
        let plaintext = serde_json::to_vec(&*data).context("Serializing secrets to JSON")?;

        let secret = age::secrecy::SecretString::from(self.passphrase.clone());
        let recipient = age::scrypt::Recipient::new(secret);
        let ciphertext =
            age::encrypt(&recipient, &plaintext).context("Encrypting secrets with age")?;

        // Atomic write: temp file → rename.
        let tmp = self.path.with_extension("enc.tmp");
        std::fs::write(&tmp, &ciphertext)
            .with_context(|| format!("Writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("Renaming to {}", self.path.display()))?;

        // Restrict file permissions on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &self.path,
                std::fs::Permissions::from_mode(0o600),
            );
        }

        Ok(())
    }
}

#[async_trait]
impl SecretBackend for EncryptedFileBackend {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        let data = self.data.lock().expect("secrets mutex poisoned");
        Ok(data.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> Result<()> {
        {
            let mut data = self.data.lock().expect("secrets mutex poisoned");
            data.insert(key.to_string(), value.to_string());
        }
        self.save_to_file()
    }

    async fn delete(&self, key: &str) -> Result<()> {
        {
            let mut data = self.data.lock().expect("secrets mutex poisoned");
            data.remove(key);
        }
        self.save_to_file()
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        let data = self.data.lock().expect("secrets mutex poisoned");
        Ok(data.keys().cloned().collect())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// DegradedBackend
// ═══════════════════════════════════════════════════════════════════════════════

/// Sentinel backend used when no real store is available.
/// Every mutation returns an error; `list_keys` returns an empty vec.
struct DegradedBackend;

#[async_trait]
impl SecretBackend for DegradedBackend {
    async fn get(&self, _key: &str) -> Result<Option<String>> {
        anyhow::bail!("Secrets vault is degraded — no backend available")
    }

    async fn set(&self, _key: &str, _value: &str) -> Result<()> {
        anyhow::bail!("Secrets vault is degraded — no backend available")
    }

    async fn delete(&self, _key: &str) -> Result<()> {
        anyhow::bail!("Secrets vault is degraded — no backend available")
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passphrase_derivation_is_stable() {
        let a = EncryptedFileBackend::derive_passphrase();
        let b = EncryptedFileBackend::derive_passphrase();
        assert_eq!(a, b, "Passphrase must be deterministic");
        assert_eq!(a.len(), 64, "SHA-256 hex digest is 64 chars");
    }

    #[test]
    fn provider_keys_are_non_empty() {
        assert!(!PROVIDER_KEYS.is_empty());
        for k in PROVIDER_KEYS {
            assert!(!k.is_empty());
            assert!(k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        }
    }

    #[test]
    fn vault_status_serde_roundtrip() {
        let statuses = vec![
            VaultStatus::KeyringAvailable,
            VaultStatus::FallbackEncryptedFile {
                path: PathBuf::from("/tmp/test.enc"),
            },
            VaultStatus::Degraded {
                reason: "test".into(),
            },
        ];
        for s in &statuses {
            let json = serde_json::to_string(s).unwrap();
            let deser: VaultStatus = serde_json::from_str(&json).unwrap();
            // Compare debug repr (no PartialEq derive, and that's fine
            // for a status enum).
            assert_eq!(format!("{s:?}"), format!("{deser:?}"));
        }
    }

    #[tokio::test]
    async fn encrypted_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("vornix-secrets-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Patch the path by building the backend manually for testing.
        let passphrase = EncryptedFileBackend::derive_passphrase();
        let path = dir.join("secrets.enc");
        let backend = EncryptedFileBackend {
            path: path.clone(),
            passphrase,
            data: Mutex::new(HashMap::new()),
        };

        // Set
        backend.set("alpha", "one").await.unwrap();
        backend.set("beta", "two").await.unwrap();

        // Get
        assert_eq!(backend.get("alpha").await.unwrap(), Some("one".into()));
        assert_eq!(backend.get("beta").await.unwrap(), Some("two".into()));
        assert_eq!(backend.get("nope").await.unwrap(), None);

        // List
        let mut keys = backend.list_keys().await.unwrap();
        keys.sort();
        assert_eq!(keys, vec!["alpha".to_string(), "beta".to_string()]);

        // Reload from file to prove persistence
        let reloaded = EncryptedFileBackend::load_from_file(&path, &backend.passphrase).unwrap();
        assert_eq!(reloaded.get("alpha").map(String::as_str), Some("one"));
        assert_eq!(reloaded.get("beta").map(String::as_str), Some("two"));

        // Delete
        backend.delete("alpha").await.unwrap();
        assert_eq!(backend.get("alpha").await.unwrap(), None);
        assert_eq!(backend.get("beta").await.unwrap(), Some("two".into()));

        // Delete non-existent is a no-op
        backend.delete("alpha").await.unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn degraded_backend_errors() {
        let backend = DegradedBackend;
        assert!(backend.get("x").await.is_err());
        assert!(backend.set("x", "y").await.is_err());
        assert!(backend.delete("x").await.is_err());
        assert!(backend.list_keys().await.unwrap().is_empty());
    }
}

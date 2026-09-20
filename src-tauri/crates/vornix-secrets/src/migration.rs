//! One-time migration of plaintext secrets into the vault.
//!
//! [`migrate_plaintext_keys`] scans a config directory for JSON files
//! containing plaintext API keys and moves them into a [`SecretsVault`],
//! stripping the values from the source files afterward.

use crate::vault::SecretsVault;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;

/// Substrings that mark a JSON key as likely containing a secret.
const SECRET_MARKERS: &[&str] = &[
    "api_key",
    "api_secret",
    "access_token",
    "private_key",
    "secret",
    "password",
    "token",
];

/// Minimum length for a value to be considered a real secret (filters
/// out placeholder strings like `""` or `"TODO"`).
const MIN_SECRET_LEN: usize = 8;

/// Scan `config_dir` for `*.json` files, extract plaintext secret
/// values, store them in `vault`, and remove the values from the
/// source files.
///
/// Returns the list of key names that were migrated. Keys already
/// present in the vault are skipped (vault wins).
pub async fn migrate_plaintext_keys(
    config_dir: &Path,
    vault: &SecretsVault,
) -> Result<Vec<String>> {
    let mut migrated = Vec::new();

    let entries = std::fs::read_dir(config_dir)
        .with_context(|| format!("Reading config dir {}", config_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Only process .json files.
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => {}
            _ => continue,
        }

        if let Err(e) = migrate_file(&path, vault, &mut migrated).await {
            tracing::warn!(path = %path.display(), "Skipping file: {e:#}");
        }
    }

    if !migrated.is_empty() {
        tracing::info!(keys = ?migrated, "Migrated plaintext secrets to vault");
    }

    Ok(migrated)
}

/// Process a single JSON file: find secret keys, migrate them, rewrite.
async fn migrate_file(
    path: &Path,
    vault: &SecretsVault,
    migrated: &mut Vec<String>,
) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Reading {}", path.display()))?;

    let mut config: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("Parsing JSON in {}", path.display()))?;

    let obj = match config.as_object_mut() {
        Some(o) => o,
        None => return Ok(()), // not a JSON object — skip
    };

    // Collect keys to migrate before mutating the map.
    let candidates: Vec<(String, String)> = obj
        .iter()
        .filter_map(|(k, v)| {
            let val = v.as_str()?;
            if is_secret_key(k) && val.len() >= MIN_SECRET_LEN && !val.is_empty() {
                Some((k.clone(), val.to_string()))
            } else {
                None
            }
        })
        .collect();

    if candidates.is_empty() {
        return Ok(());
    }

    for (key, value) in &candidates {
        // Only migrate if the vault doesn't already have this key.
        if migrated.contains(key) {
            continue;
        }

        match vault.get(key).await {
            Ok(Some(_)) => {
                tracing::debug!(key, "Already in vault — skipping");
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(key, "Vault get failed: {e:#} — skipping");
                continue;
            }
        }

        // Write to vault first to avoid data loss.
        vault
            .set(key, value)
            .await
            .with_context(|| format!("Storing '{key}' in vault"))?;

        // Verify the write.
        let verify = vault
            .get(key)
            .await
            .with_context(|| format!("Verifying '{key}' in vault"))?;
        if verify.as_deref() != Some(value.as_str()) {
            anyhow::bail!("Vault verification failed for '{key}'");
        }

        tracing::info!(key, path = %path.display(), "Migrated secret");
        migrated.push(key.clone());
    }

    // Remove migrated keys from the JSON object.
    let migrated_set: HashSet<&str> = candidates
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();

    for key in &migrated_set {
        obj.remove(*key);
    }

    // Rewrite the file (or delete if nothing meaningful remains).
    let remaining: Vec<_> = obj
        .iter()
        .filter(|(_, v)| !v.is_null())
        .collect();

    if remaining.is_empty() {
        std::fs::remove_file(path)
            .with_context(|| format!("Removing empty config {}", path.display()))?;
        tracing::info!(path = %path.display(), "Removed empty config file");
    } else {
        let updated =
            serde_json::to_string_pretty(&config).context("Serializing updated config")?;
        std::fs::write(path, updated)
            .with_context(|| format!("Rewriting {}", path.display()))?;
        tracing::info!(
            path = %path.display(),
            removed = migrated_set.len(),
            "Updated config file"
        );
    }

    Ok(())
}

/// Return `true` when `key` looks like it holds a secret value.
fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SECRET_MARKERS.iter().any(|marker| lower.contains(marker))
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::SecretBackend;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Minimal in-memory backend for migration tests.
    struct MemBackend {
        data: Mutex<HashMap<String, String>>,
    }

    impl MemBackend {
        fn new() -> Self {
            Self {
                data: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl SecretBackend for MemBackend {
        async fn get(&self, key: &str) -> Result<Option<String>> {
            Ok(self.data.lock().unwrap().get(key).cloned())
        }
        async fn set(&self, key: &str, value: &str) -> Result<()> {
            self.data.lock().unwrap().insert(key.to_string(), value.to_string());
            Ok(())
        }
        async fn delete(&self, key: &str) -> Result<()> {
            self.data.lock().unwrap().remove(key);
            Ok(())
        }
        async fn list_keys(&self) -> Result<Vec<String>> {
            Ok(self.data.lock().unwrap().keys().cloned().collect())
        }
    }

    fn test_vault() -> SecretsVault {
        SecretsVault {
            backend: Box::new(MemBackend::new()),
        }
    }

    #[test]
    fn is_secret_key_matches_known_patterns() {
        assert!(is_secret_key("openrouter_api_key"));
        assert!(is_secret_key("github_token"));
        assert!(is_secret_key("GITHUB_TOKEN"));
        assert!(is_secret_key("my_api_secret"));
        assert!(is_secret_key("db_password"));
        assert!(is_secret_key("access_token"));
        assert!(is_secret_key("private_key"));

        // Should NOT match
        assert!(!is_secret_key("name"));
        assert!(!is_secret_key("version"));
        assert!(!is_secret_key("description"));
        assert!(!is_secret_key("enabled"));
    }

    #[tokio::test]
    async fn migrate_extracts_plaintext_keys() {
        let dir =
            std::env::temp_dir().join(format!("vornix-migration-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Write a config file with a mix of secret and non-secret keys.
        let config = serde_json::json!({
            "openrouter_api_key": "sk-or-1234567890abcdef",
            "github_token": "ghp_abcdefghijklmnopqrstuvwxyz123456",
            "theme": "dark",
            "editor_font_size": 14,
            "short_value": "abc"
        });
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_string_pretty(&config).unwrap(),
        )
        .unwrap();

        let vault = test_vault();
        let migrated = migrate_plaintext_keys(&dir, &vault).await.unwrap();

        let mut migrated_sorted = migrated.clone();
        migrated_sorted.sort();
        assert_eq!(
            migrated_sorted,
            vec!["github_token".to_string(), "openrouter_api_key".to_string()]
        );

        // Secrets should be in the vault.
        assert_eq!(
            vault.get("openrouter_api_key").await.unwrap().as_deref(),
            Some("sk-or-1234567890abcdef")
        );
        assert_eq!(
            vault.get("github_token").await.unwrap().as_deref(),
            Some("ghp_abcdefghijklmnopqrstuvwxyz123456")
        );

        // Non-secrets should still be in the file.
        let remaining: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(remaining["theme"], "dark");
        assert_eq!(remaining["editor_font_size"], 14);
        assert_eq!(remaining["short_value"], "abc");
        assert!(remaining.get("openrouter_api_key").is_none());
        assert!(remaining.get("github_token").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn migrate_skips_keys_already_in_vault() {
        let dir =
            std::env::temp_dir().join(format!("vornix-migration-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let config = serde_json::json!({
            "api_key": "new-value-12345678"
        });
        std::fs::write(
            dir.join("config.json"),
            serde_json::to_string_pretty(&config).unwrap(),
        )
        .unwrap();

        let vault = test_vault();
        vault.set("api_key", "existing-value-12345678").await.unwrap();

        let migrated = migrate_plaintext_keys(&dir, &vault).await.unwrap();
        assert!(migrated.is_empty(), "Should skip already-present keys");

        // Vault should still have the original value.
        assert_eq!(
            vault.get("api_key").await.unwrap().as_deref(),
            Some("existing-value-12345678")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn migrate_removes_file_when_all_keys_are_secrets() {
        let dir =
            std::env::temp_dir().join(format!("vornix-migration-rm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let config = serde_json::json!({
            "api_key": "sk-1234567890"
        });
        let cfg_path = dir.join("all_secrets.json");
        std::fs::write(&cfg_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

        let vault = test_vault();
        let migrated = migrate_plaintext_keys(&dir, &vault).await.unwrap();
        assert_eq!(migrated, vec!["api_key".to_string()]);
        assert!(!cfg_path.exists(), "File should be removed when only secrets remain");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn migrate_handles_empty_dir() {
        let dir =
            std::env::temp_dir().join(format!("vornix-migration-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let vault = test_vault();
        let migrated = migrate_plaintext_keys(&dir, &vault).await.unwrap();
        assert!(migrated.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

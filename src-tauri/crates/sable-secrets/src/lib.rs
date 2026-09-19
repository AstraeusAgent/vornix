//! # sable-secrets
//!
//! Secret management for the Sable AI coding harness.
//!
//! This crate manages API keys, tokens, and other secrets using the
//! OS keychain (macOS Keychain, Windows Credential Manager, Linux
//! Secret Service via D-Bus) with an encrypted-file fallback backed
//! by [age](https://age-encryption.org).
//!
//! ## Quick start
//!
//! ```rust,no_run
//! # async fn example() -> anyhow::Result<()> {
//! use sable_secrets::SecretsVault;
//!
//! let (vault, status) = SecretsVault::initialize().await?;
//! println!("Vault status: {status:?}");
//!
//! vault.set("openrouter_api_key", "sk-or-...").await?;
//! let key = vault.get("openrouter_api_key").await?;
//! # Ok(())
//! # }
//! ```

pub mod migration;
pub mod vault;

pub use vault::{SecretBackend, SecretsVault, VaultStatus};

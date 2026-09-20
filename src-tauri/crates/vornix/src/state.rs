use std::sync::Arc;
use tokio::sync::Mutex;

use vornix_memory::{MemoryStore, SessionStore};
use vornix_mcp::manager::McpManager;
use vornix_persona::intensity::PersonaIntensity;
use vornix_persona::policy::PersonaPolicy;
use vornix_secrets::{SecretsVault, VaultStatus};

pub struct AppState {
    pub vault: Arc<Mutex<SecretsVault>>,
    pub vault_status: VaultStatus,
    pub sessions: Arc<SessionStore>,
    pub memory: Arc<MemoryStore>,
    pub mcp: Arc<Mutex<McpManager>>,
    pub persona: Arc<Mutex<PersonaPolicy>>,
}
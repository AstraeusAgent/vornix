use std::sync::Arc;
use tokio::sync::Mutex;

use sable_memory::{MemoryStore, SessionStore};
use sable_mcp::manager::McpManager;
use sable_persona::intensity::PersonaIntensity;
use sable_persona::policy::PersonaPolicy;
use sable_secrets::{SecretsVault, VaultStatus};

pub struct AppState {
    pub vault: Arc<Mutex<SecretsVault>>,
    pub vault_status: VaultStatus,
    pub sessions: Arc<SessionStore>,
    pub memory: Arc<MemoryStore>,
    pub mcp: Arc<Mutex<McpManager>>,
    pub persona: Arc<Mutex<PersonaPolicy>>,
}
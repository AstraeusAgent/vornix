use anyhow::Result;
use serde::{Deserialize, Serialize};
use vornix_core::{Orchestrator, OrchestratorConfig, OrchestratorEvent, ChatProvider, ChatStream, ToolExecutor};
use vornix_memory::{SessionStore, MemoryStore};
use vornix_secrets::{SecretsVault, VaultStatus};
use vornix_providers::{ChatRequest, ModelInfo, ProviderCredentials, OpenRouterClient};
use vornix_mcp::manager::McpManager;
use vornix_tools::{ToolCall, ToolResult, PermissionDecision};
use vornix_persona::policy::PersonaPolicy;
use vornix_persona::intensity::PersonaIntensity;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

mod commands;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Vornix starting up");

    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let _guard = rt.enter();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            rt.block_on(async move {
                // Initialize secrets vault
                let (vault, vault_status) = SecretsVault::initialize()
                    .await
                    .expect("failed to initialize secrets vault");
                info!("secrets vault: {:?}", vault_status);

                // Initialize SQLite store
                let db_path = app_handle
                    .path()
                    .app_data_dir()
                    .expect("no app data dir")
                    .join("vornix.db");
                std::fs::create_dir_all(db_path.parent().unwrap()).ok();

                let pool = sqlx::sqlite::SqlitePoolOptions::new()
                    .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
                    .await
                    .expect("failed to open SQLite database");

                let session_store = SessionStore::new(pool.clone());
                session_store.run_migrations().await.expect("session migrations failed");

                let memory_store = MemoryStore::new(pool.clone());
                memory_store.run_migrations().await.expect("memory migrations failed");

                // Initialize MCP manager
                let mcp_manager = McpManager::new();

                // Initialize persona
                let persona = PersonaPolicy::default_vornix();

                // Store global state
                let app_state = AppState {
                    vault: Arc::new(Mutex::new(vault)),
                    vault_status,
                    sessions: Arc::new(session_store),
                    memory: Arc::new(memory_store),
                    mcp: Arc::new(Mutex::new(mcp_manager)),
                    persona: Arc::new(Mutex::new(persona)),
                };

                app_handle.manage(app_state);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::session_create,
            commands::session_list,
            commands::session_get,
            commands::session_delete,
            commands::session_get_messages,
            commands::provider_list_models,
            commands::provider_set_key,
            commands::provider_get_key,
            commands::persona_set_intensity,
            commands::persona_get_intensity,
            commands::persona_get_system_prompt,
            commands::settings_get_vault_status,
            commands::chat_send_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
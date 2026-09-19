use sable_memory::{SessionStore, MemoryStore};
use sable_mcp::manager::McpManager;
use sable_persona::policy::PersonaPolicy;
use sable_secrets::{SecretsVault, VaultStatus};
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;
use tracing::info;

mod commands;
pub mod agent;
pub mod servers;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Sable starting up");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            tauri::async_runtime::block_on(async move {
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
                    .join("sable.db");
                std::fs::create_dir_all(db_path.parent().unwrap()).ok();

                let pool = sqlx::sqlite::SqlitePoolOptions::new()
                    .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
                    .await
                    .expect("failed to open SQLite database");

                let session_store = SessionStore::new(pool.clone());
                session_store.run_migrations().await.expect("session migrations failed");

                let memory_store = MemoryStore::new(pool.clone());
                memory_store.run_migrations().await.expect("memory migrations failed");

                // Initialize MCP manager and connect the bundled servers
                let mut mcp_manager = McpManager::new();
                servers::connect_bundled_servers(&mut mcp_manager).await;

                // Initialize persona
                let persona = PersonaPolicy::default_sable();

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
            commands::list_models,
            commands::provider_set_key,
            commands::provider_get_key,
            commands::provider_key_status,
            commands::provider_delete_key,
            commands::persona_set_intensity,
            commands::persona_get_intensity,
            commands::persona_get_system_prompt,
            commands::settings_get_vault_status,
            commands::chat_send_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
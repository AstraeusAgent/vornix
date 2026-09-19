use sable_mcp::manager::McpManager;
use sable_mcp::transport::{McpServerConfig, TransportConfig};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, warn};

/// Register and connect the bundled first-party MCP servers.
/// Each is a real child process speaking JSON-RPC 2.0 over stdio.
pub async fn connect_bundled_servers(mcp: &mut McpManager) {
    let servers_root = bundled_servers_dir();
    let workspace = crate::agent::workspace_root();
    std::fs::create_dir_all(&workspace).ok();

    for name in ["sable-fs", "sable-shell", "sable-thinking"] {
        let script = servers_root.join(name).join("index.cjs");
        if !script.exists() {
            warn!("bundled MCP server script missing: {}", script.display());
            continue;
        }

        let mut env = HashMap::new();
        env.insert("SABLE_WORKSPACE".to_string(), workspace.display().to_string());

        let config = McpServerConfig {
            name: name.to_string(),
            transport: TransportConfig::Stdio {
                command: "node".to_string(),
                args: vec![script.display().to_string()],
                env,
            },
            enabled: true,
            auto_restart: true,
        };

        if let Err(e) = mcp.add_server(config).await {
            warn!("failed to add MCP server '{}': {}", name, e);
            continue;
        }
        if let Err(e) = mcp.connect_server(name).await {
            warn!("failed to connect MCP server '{}': {}", name, e);
        } else {
            info!("connected bundled MCP server '{}'", name);
        }
    }
}

/// Directory containing mcp-servers/<name>/index.js.
/// In dev builds this is the repo root; overridden with SABLE_SERVERS_DIR.
pub fn bundled_servers_dir() -> PathBuf {
    std::env::var("SABLE_SERVERS_DIR").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("mcp-servers"))
            .unwrap_or_else(|| PathBuf::from("mcp-servers"))
    })
}
use crate::server::{McpDiscoveredTool, McpServer, ServerStatus};
use crate::transport::{McpServerConfig, TransportConfig};
use anyhow::{Context, Result};
use vornix_tools::{ToolCategory, ToolRegistry, ToolResult, ToolSchema};
use std::collections::HashMap;
use tracing::{info, warn};

pub struct McpManager {
    servers: HashMap<String, McpServer>,
    tool_registry: ToolRegistry,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            tool_registry: ToolRegistry::new(),
        }
    }

    pub async fn add_server(&mut self, config: McpServerConfig) -> Result<()> {
        let name = config.name.clone();
        if self.servers.contains_key(&name) {
            anyhow::bail!("MCP server '{}' already exists", name);
        }
        let server = McpServer::new(config);
        self.servers.insert(name.clone(), server);
        info!("added MCP server '{}'", name);
        Ok(())
    }

    pub async fn remove_server(&mut self, name: &str) -> Result<()> {
        if let Some(mut server) = self.servers.remove(name) {
            server.disconnect().await?;
            // Remove tools from registry
            for tool in &server.tools {
                self.tool_registry.unregister(&tool.full_name);
            }
            info!("removed MCP server '{}'", name);
            Ok(())
        } else {
            anyhow::bail!("MCP server '{}' not found", name);
        }
    }

    pub async fn connect_server(&mut self, name: &str) -> Result<()> {
        let server = self
            .servers
            .get_mut(name)
            .with_context(|| format!("MCP server '{}' not found", name))?;

        if !server.config.enabled {
            warn!("MCP server '{}' is divornixd, skipping connect", name);
            return Ok(());
        }

        server.connect().await?;

        // Register discovered tools
        let tools = server.tools.clone();
        for tool in &tools {
            let schema = ToolSchema {
                name: tool.full_name.clone(),
                description: tool.description.clone(),
                parameters: tool.input_schema.clone(),
                category: ToolCategory::Custom(name.to_string()),
                required_capabilities: Vec::new(),
            };
            if let Err(e) = self.tool_registry.register(schema) {
                warn!("failed to register tool '{}': {}", tool.full_name, e);
            }
        }

        Ok(())
    }

    pub async fn connect_all(&mut self) -> Result<()> {
        let names: Vec<String> = self
            .servers
            .iter()
            .filter(|(_, s)| s.config.enabled)
            .map(|(name, _)| name.clone())
            .collect();

        for name in names {
            if let Err(e) = self.connect_server(&name).await {
                warn!("failed to connect MCP server '{}': {}", name, e);
            }
        }

        Ok(())
    }

    pub async fn disconnect_server(&mut self, name: &str) -> Result<()> {
        let server = self
            .servers
            .get_mut(name)
            .with_context(|| format!("MCP server '{}' not found", name))?;

        let tool_names: Vec<String> = server.tools.iter().map(|t| t.full_name.clone()).collect();
        server.disconnect().await?;

        for full_name in &tool_names {
            self.tool_registry.unregister(full_name);
        }

        Ok(())
    }

    pub async fn reconnect_server(&mut self, name: &str) -> Result<()> {
        self.disconnect_server(name).await.ok();
        self.connect_server(name).await
    }

    pub fn get_server(&self, name: &str) -> Option<&McpServer> {
        self.servers.get(name)
    }

    pub fn list_servers(&self) -> Vec<(&str, &ServerStatus, usize)> {
        self.servers
            .iter()
            .map(|(name, server)| {
                (name.as_str(), &server.status, server.tools.len())
            })
            .collect()
    }

    pub fn get_all_tools(&self) -> Vec<String> {
        self.tool_registry
            .list_all()
            .iter()
            .map(|t| t.name.clone())
            .collect()
    }

    pub fn get_tools_with_schemas(&self) -> Vec<&ToolSchema> {
        self.tool_registry.list_all()
    }

    pub fn get_tool_registry(&self) -> &ToolRegistry {
        &self.tool_registry
    }

    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult> {
        // Tool names are namespaced as "serverName.toolName"
        let parts: Vec<&str> = tool_name.splitn(2, '.').collect();
        if parts.len() != 2 {
            anyhow::bail!(
                "tool name '{}' is not namespaced (expected 'serverName.toolName')",
                tool_name
            );
        }
        let (server_name, local_name) = (parts[0], parts[1]);

        let server = self
            .servers
            .get_mut(server_name)
            .with_context(|| format!("MCP server '{}' not found", server_name))?;

        if server.status != ServerStatus::Connected {
            anyhow::bail!(
                "MCP server '{}' is not connected (status: {})",
                server_name,
                server.status
            );
        }

        let start = std::time::Instant::now();
        match server.call_tool(local_name, arguments).await {
            Ok(result) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let output: String = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text())
                    .collect::<Vec<_>>()
                    .join("\n");
                let (output, truncated) = if output.len() > 100_000 {
                    (output[..100_000].to_string(), true)
                } else {
                    (output, false)
                };
                Ok(ToolResult {
                    id: uuid::Uuid::new_v4(),
                    tool_call_id: uuid::Uuid::new_v4(),
                    success: !result.is_error,
                    output,
                    error: if result.is_error {
                        Some("tool returned an error".to_string())
                    } else {
                        None
                    },
                    duration_ms,
                    truncated,
                })
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                Ok(ToolResult {
                    id: uuid::Uuid::new_v4(),
                    tool_call_id: uuid::Uuid::new_v4(),
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                    duration_ms,
                    truncated: false,
                })
            }
        }
    }

    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    pub fn connected_count(&self) -> usize {
        self.servers
            .values()
            .filter(|s| s.status == ServerStatus::Connected)
            .count()
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}
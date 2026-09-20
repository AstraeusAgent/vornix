//! Tool registry: stores and looks up [`ToolSchema`] definitions by name or category.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::schema::{ToolCategory, ToolSchema};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during registry operations.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("tool already registered: {name}")]
    AlreadyRegistered { name: String },

    #[error("tool not found: {name}")]
    NotFound { name: String },
}

// ---------------------------------------------------------------------------
// MCP tool definition (for discover_from_mcp)
// ---------------------------------------------------------------------------

/// Minimal representation of an MCP tool as it arrives from an MCP server's
/// `tools/list` response.  Only the fields needed for registration are captured.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDefinition {
    /// Tool name as reported by the MCP server (without namespace prefix).
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    #[serde(default = "default_empty_object")]
    pub input_schema: serde_json::Value,
}

fn default_empty_object() -> serde_json::Value {
    serde_json::json!({})
}

// ---------------------------------------------------------------------------
// ToolRegistry
// ---------------------------------------------------------------------------

/// A thread-safe (after construction) registry of tool schemas, indexed by
/// name and groupable by category.
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolSchema>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool schema.  Returns an error if a tool with the same name
    /// is already registered.
    pub fn register(&mut self, schema: ToolSchema) -> Result<(), RegistryError> {
        let name = schema.name.clone();
        if self.tools.contains_key(&name) {
            return Err(RegistryError::AlreadyRegistered { name });
        }
        self.tools.insert(name, schema);
        Ok(())
    }

    /// Register a tool schema, replacing any existing entry with the same name.
    pub fn upsert(&mut self, schema: ToolSchema) {
        self.tools.insert(schema.name.clone(), schema);
    }

    /// Look up a tool by exact name.
    pub fn get(&self, name: &str) -> Result<&ToolSchema, RegistryError> {
        self.tools
            .get(name)
            .ok_or_else(|| RegistryError::NotFound {
                name: name.to_string(),
            })
    }

    /// Return schemas for every registered tool.
    pub fn list_all(&self) -> Vec<&ToolSchema> {
        self.tools.values().collect()
    }

    /// Return schemas whose [`ToolCategory`] matches the provided category.
    pub fn list_by_category(&self, category: &ToolCategory) -> Vec<&ToolSchema> {
        self.tools
            .values()
            .filter(|s| &s.category == category)
            .collect()
    }

    /// Return the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Return `true` if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Remove a tool by name. Returns `true` if a tool was removed.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.tools.remove(name).is_some()
    }

    /// Discover tools from an MCP server's tool list and register them with an
    /// optional namespace prefix.
    ///
    /// For example, with `namespace = "github"`, an MCP tool named `"create_issue"`
    /// becomes `"github.create_issue"` in the registry.
    ///
    /// Tools that would collide with an already-registered name are logged and
    /// skipped (not an error) so that a partial MCP failure doesn't block startup.
    pub fn discover_from_mcp(
        &mut self,
        namespace: &str,
        mcp_tools: &[McpToolDefinition],
    ) -> usize {
        let mut registered = 0usize;
        for def in mcp_tools {
            let qualified_name = if namespace.is_empty() {
                def.name.clone()
            } else {
                format!("{}.{}", namespace, def.name)
            };

            if self.tools.contains_key(&qualified_name) {
                tracing::warn!(
                    name = %qualified_name,
                    "skipping MCP tool — name already registered"
                );
                continue;
            }

            let schema = ToolSchema {
                name: qualified_name.clone(),
                description: def.description.clone(),
                parameters: def.input_schema.clone(),
                required_capabilities: Vec::new(),
                category: ToolCategory::Custom(namespace.to_string()),
            };

            self.tools.insert(qualified_name, schema);
            registered += 1;
        }
        registered
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_schema(name: &str) -> ToolSchema {
        ToolSchema {
            name: name.to_string(),
            description: format!("A tool called {name}"),
            parameters: json!({"type": "object", "properties": {}}),
            required_capabilities: vec![],
            category: ToolCategory::Filesystem,
        }
    }

    #[test]
    fn register_and_get() {
        let mut reg = ToolRegistry::new();
        reg.register(sample_schema("a")).unwrap();
        assert_eq!(reg.get("a").unwrap().name, "a");
    }

    #[test]
    fn register_duplicate_errors() {
        let mut reg = ToolRegistry::new();
        reg.register(sample_schema("a")).unwrap();
        let err = reg.register(sample_schema("a")).unwrap_err();
        assert!(matches!(err, RegistryError::AlreadyRegistered { .. }));
    }

    #[test]
    fn upsert_overwrites() {
        let mut reg = ToolRegistry::new();
        reg.register(sample_schema("a")).unwrap();
        let mut updated = sample_schema("a");
        updated.description = "updated".to_string();
        reg.upsert(updated);
        assert_eq!(reg.get("a").unwrap().description, "updated");
    }

    #[test]
    fn list_by_category() {
        let mut reg = ToolRegistry::new();
        reg.register(sample_schema("a")).unwrap(); // Filesystem
        let mut git_tool = sample_schema("b");
        git_tool.category = ToolCategory::Git;
        reg.register(git_tool).unwrap();

        assert_eq!(reg.list_by_category(&ToolCategory::Filesystem).len(), 1);
        assert_eq!(reg.list_by_category(&ToolCategory::Git).len(), 1);
        assert_eq!(reg.list_by_category(&ToolCategory::Shell).len(), 0);
    }

    #[test]
    fn discover_from_mcp_namespaced() {
        let mut reg = ToolRegistry::new();
        let mcp_tools = vec![
            McpToolDefinition {
                name: "create_issue".to_string(),
                description: "Create a GitHub issue".to_string(),
                input_schema: json!({"type": "object"}),
            },
            McpToolDefinition {
                name: "list_repos".to_string(),
                description: "List repos".to_string(),
                input_schema: json!({"type": "object"}),
            },
        ];

        let count = reg.discover_from_mcp("github", &mcp_tools);
        assert_eq!(count, 2);
        assert!(reg.get("github.create_issue").is_ok());
        assert!(reg.get("github.list_repos").is_ok());
    }

    #[test]
    fn discover_from_mcp_skips_duplicates() {
        let mut reg = ToolRegistry::new();
        let mcp_tools = vec![McpToolDefinition {
            name: "foo".to_string(),
            description: "first".to_string(),
            input_schema: json!({}),
        }];

        reg.discover_from_mcp("ns", &mcp_tools);

        let mcp_tools2 = vec![McpToolDefinition {
            name: "foo".to_string(),
            description: "second".to_string(),
            input_schema: json!({}),
        }];
        let count = reg.discover_from_mcp("ns", &mcp_tools2);
        assert_eq!(count, 0);
        assert_eq!(reg.get("ns.foo").unwrap().description, "first");
    }

    #[test]
    fn empty_namespace() {
        let mut reg = ToolRegistry::new();
        let mcp_tools = vec![McpToolDefinition {
            name: "bare".to_string(),
            description: "no namespace".to_string(),
            input_schema: json!({}),
        }];
        reg.discover_from_mcp("", &mcp_tools);
        assert!(reg.get("bare").is_ok());
    }
}

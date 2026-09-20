//! Tool schema definitions for the Vornix AI coding harness.
//!
//! Contains the core types that describe tools, tool invocations, and their results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Category that a tool belongs to, used for grouping and permission classification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolCategory {
    /// Filesystem operations (read, write, list, glob, grep).
    Filesystem,
    /// Shell command execution.
    Shell,
    /// Git version-control operations.
    Git,
    /// Diagnostics and inspection (lint, type-check, test runner).
    Diagnostics,
    /// Internal reasoning / scratchpad tools.
    Thinking,
    /// Memory store operations (search, write, import).
    Memory,
    /// Skill invocation and management.
    Skill,
    /// A custom category identified by an arbitrary string (e.g. MCP server name).
    Custom(String),
}

/// JSON Schema–based description of a single tool that can be invoked.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSchema {
    /// Unique tool name, e.g. `"filesystem.readFile"` or `"shell.execute"`.
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema object describing the tool's input parameters.
    pub parameters: serde_json::Value,
    /// Capabilities the runtime must provide for this tool to function
    /// (e.g. `["fs:write", "net:fetch"]`).
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    /// The category this tool belongs to.
    pub category: ToolCategory,
}

/// Represents a single invocation of a tool by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    /// Unique identifier for this invocation.
    pub id: Uuid,
    /// Name of the tool being called (must match a registered [`ToolSchema::name`]).
    pub tool_name: String,
    /// Arguments supplied to the tool, validated against the tool's parameter schema.
    pub arguments: serde_json::Value,
    /// Wall-clock time when the call was issued.
    pub timestamp: DateTime<Utc>,
}

impl ToolCall {
    /// Create a new [`ToolCall`] with a fresh UUID and the current timestamp.
    pub fn new(tool_name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            tool_name: tool_name.into(),
            arguments,
            timestamp: Utc::now(),
        }
    }
}

/// The outcome of executing a [`ToolCall`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    /// A result-level identifier (mirrors [`ToolCall::id`] for convenience).
    pub id: Uuid,
    /// The id of the [`ToolCall`] that produced this result.
    pub tool_call_id: Uuid,
    /// Whether the tool execution succeeded.
    pub success: bool,
    /// Primary textual output of the tool.
    pub output: String,
    /// Error message when [`ToolResult::success`] is `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Wall-clock execution duration in milliseconds.
    pub duration_ms: u64,
    /// Whether the output was truncated (e.g. exceeding a token or byte limit).
    #[serde(default)]
    pub truncated: bool,
}

impl ToolResult {
    /// Build a successful [`ToolResult`] for the given call.
    pub fn success(tool_call: &ToolCall, output: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            tool_call_id: tool_call.id,
            success: true,
            output: output.into(),
            error: None,
            duration_ms,
            truncated: false,
        }
    }

    /// Build a failed [`ToolResult`] for the given call.
    pub fn failure(
        tool_call: &ToolCall,
        error: impl Into<String>,
        duration_ms: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tool_call_id: tool_call.id,
            success: false,
            output: String::new(),
            error: Some(error.into()),
            duration_ms,
            truncated: false,
        }
    }

    /// Mark this result's output as truncated.
    pub fn with_truncated(mut self, truncated: bool) -> Self {
        self.truncated = truncated;
        self
    }
}

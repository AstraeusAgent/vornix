//! # vornix-tools
//!
//! Tool schema registry, permission tiers, and execution infrastructure for
//! the Vornix AI coding harness.
//!
//! ## Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`schema`] | Core types: `ToolSchema`, `ToolCall`, `ToolResult`, `ToolCategory` |
//! | [`permission`] | Permission tier classification and policy evaluation |
//! | [`registry`] | `ToolRegistry` for registering, looking up, and discovering tools |
//! | [`undo`] | Per-session undo history with file-content snapshots |

pub mod permission;
pub mod registry;
pub mod schema;
pub mod undo;

// Convenient re-exports of the most-used types at crate root.
pub use permission::{
    PermissionDecision, PermissionError, PermissionPolicy, PermissionRequest, PermissionTier,
};
pub use registry::{McpToolDefinition, RegistryError, ToolRegistry};
pub use schema::{ToolCall, ToolCategory, ToolResult, ToolSchema};
pub use undo::{UndoEntry, UndoError, UndoHistory};

//! Permission tier system for tool execution in the Sable harness.
//!
//! Every tool call is classified into a [`PermissionTier`], then evaluated against
//! the session's auto-approve policy to produce a [`PermissionDecision`].

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::schema::ToolCall;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during permission operations.
#[derive(Debug, Error)]
pub enum PermissionError {
    #[error("tool call denied: {reason}")]
    Denied { reason: String },

    #[error("unknown tool: {tool_name}")]
    UnknownTool { tool_name: String },

    #[error("invalid argument for permission check: {detail}")]
    InvalidArgument { detail: String },
}

// ---------------------------------------------------------------------------
// PermissionTier
// ---------------------------------------------------------------------------

/// The three tiers of permission a tool call can fall into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionTier {
    /// Read-only operations that cannot alter state (file reads, diagnostics, thinking).
    Tier0ReadOnly,
    /// Write operations that are reversible or undoable (file writes, allowlisted shell cmds).
    Tier1ReversibleWrite,
    /// Destructive or irreversible operations requiring explicit approval.
    Tier2Destructive,
}

impl PermissionTier {
    /// Returns `true` if this tier is at least as restrictive as `other`.
    pub fn is_at_least(self, other: PermissionTier) -> bool {
        use PermissionTier::*;
        matches!(
            (self, other),
            (Tier2Destructive, _)
                | (Tier1ReversibleWrite, Tier0ReadOnly | Tier1ReversibleWrite)
                | (Tier0ReadOnly, Tier0ReadOnly)
        )
    }
}

// ---------------------------------------------------------------------------
// PermissionRequest
// ---------------------------------------------------------------------------

/// A request to execute a tool at a given permission tier, along with context
/// for the reviewer (human or automated).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    /// The tool call that wants to execute.
    pub tool_call: ToolCall,
    /// The classified permission tier.
    pub tier: PermissionTier,
    /// Human-readable description of what the operation will do.
    pub description: String,
    /// Risk factors the reviewer should be aware of.
    #[serde(default)]
    pub risk_notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// PermissionDecision
// ---------------------------------------------------------------------------

/// The outcome of evaluating a [`PermissionRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "decision")]
pub enum PermissionDecision {
    /// The operation may proceed without further approval.
    AutoApproved,
    /// A human (or elevated policy) must approve before execution.
    RequiresApproval,
    /// The operation is categorically denied.
    Denied { reason: String },
}

// ---------------------------------------------------------------------------
// Shell allow-list (Tier 1) & destructive patterns (Tier 2)
// ---------------------------------------------------------------------------

/// Prefixes / exact commands that are considered safe enough for Tier 1
/// (auto-approved when the session enables shell auto-approve).
const TIER1_SHELL_ALLOWLIST: &[&str] = &[
    // Package-manager test / build
    "npm test",
    "npm run test",
    "npm run build",
    "npm run lint",
    "npm run check",
    "npx ",
    "yarn test",
    "yarn build",
    "yarn lint",
    "pnpm test",
    "pnpm build",
    "pnpm lint",
    // Cargo
    "cargo build",
    "cargo test",
    "cargo check",
    "cargo clippy",
    "cargo fmt",
    "cargo doc",
    // Python
    "python -m pytest",
    "pytest",
    "pip install ",
    "pip list",
    // Go
    "go build",
    "go test",
    "go vet",
    "gofmt",
    // Node / JS
    "node ",
    "ts-node ",
    "tsc ",
    "eslint ",
    "prettier ",
    // General read-only utilities
    "ls",
    "ls ",
    "cat ",
    "head ",
    "tail ",
    "wc ",
    "echo ",
    "which ",
    "env",
    "pwd",
    "date",
    "uname",
    "git status",
    "git log",
    "git diff",
    "git branch",
    "git show",
];

/// Patterns that indicate a destructive or irreversible shell operation (Tier 2).
const DESTRUCTIVE_PATTERNS: &[&str] = &[
    // Filesystem destruction
    "rm -rf",
    "rm -r ",
    "rm -f ",
    "rmdir ",
    "shred ",
    "mkfs.",
    "dd if=",
    // Git force operations
    "git push --force",
    "git push -f",
    "git push --force-with-lease",
    "git reset --hard",
    "git checkout --",   // discards working-tree changes
    "git clean -f",
    "git clean -fd",
    "git branch -D",
    // Database
    "DROP TABLE",
    "DROP DATABASE",
    "DROP SCHEMA",
    "TRUNCATE",
    "DELETE FROM",
    // System-level
    "sudo ",
    "chmod 777",
    "shutdown",
    "reboot",
    "kill -9",
    "pkill",
    // Network exfiltration risk
    "curl ",
    "wget ",
    // Package removal
    "npm uninstall",
    "pip uninstall",
    "apt remove",
    "apt purge",
    "brew uninstall",
];

// ---------------------------------------------------------------------------
// PermissionPolicy
// ---------------------------------------------------------------------------

/// Stateless policy engine that classifies tool calls and evaluates permission
/// requests against a session-level auto-approve set.
#[derive(Debug, Clone)]
pub struct PermissionPolicy;

impl PermissionPolicy {
    /// Classify a [`ToolCall`] into the appropriate [`PermissionTier`].
    ///
    /// The classification is based on the tool name prefix and, for shell
    /// commands, on pattern-matching against the command string.
    pub fn classify(tool_call: &ToolCall) -> PermissionTier {
        use PermissionTier::*;

        match tool_call.tool_name.as_str() {
            // -- Always Tier 0 (read-only) --
            name if name.starts_with("filesystem.read") => Tier0ReadOnly,
            name if name.starts_with("filesystem.list") => Tier0ReadOnly,
            name if name.starts_with("filesystem.file") => Tier0ReadOnly,
            name if name.starts_with("filesystem.glob") => Tier0ReadOnly,
            name if name.starts_with("filesystem.grep") => Tier0ReadOnly,
            name if name.starts_with("diagnostics.") => Tier0ReadOnly,
            name if name.starts_with("thinking.") => Tier0ReadOnly,
            name if name.starts_with("memory.search") => Tier0ReadOnly,
            name if name.starts_with("memory.read") => Tier0ReadOnly,
            name if name.starts_with("git.status") => Tier0ReadOnly,
            name if name.starts_with("git.log") => Tier0ReadOnly,
            name if name.starts_with("git.diff") => Tier0ReadOnly,
            name if name.starts_with("git.show") => Tier0ReadOnly,
            name if name.starts_with("git.branch") && !name.contains("delete") => Tier0ReadOnly,

            // -- Shell: inspect the command string --
            "shell.execute" => Self::classify_shell(tool_call),

            // -- Filesystem writes: Tier 1 (undoable via snapshots) --
            name if name.starts_with("filesystem.write") => Tier1ReversibleWrite,
            name if name.starts_with("filesystem.edit") => Tier1ReversibleWrite,
            name if name.starts_with("filesystem.create") => Tier1ReversibleWrite,
            name if name.starts_with("filesystem.delete") => Tier2Destructive,
            name if name.starts_with("filesystem.move") => Tier1ReversibleWrite,
            name if name.starts_with("filesystem.mkdir") => Tier1ReversibleWrite,

            // -- Git writes --
            name if name.starts_with("git.commit") => Tier1ReversibleWrite,
            name if name.starts_with("git.stage") => Tier1ReversibleWrite,
            name if name.starts_with("git.unstage") => Tier1ReversibleWrite,
            name if name.starts_with("git.checkout") => Tier1ReversibleWrite,
            name if name.starts_with("git.stash") => Tier1ReversibleWrite,
            name if name.starts_with("git.push") => Tier2Destructive,
            name if name.starts_with("git.reset") => Tier2Destructive,
            name if name.starts_with("git.rebase") => Tier2Destructive,
            name if name.starts_with("git.merge") => Tier1ReversibleWrite,

            // -- Memory writes --
            name if name.starts_with("memory.write") => Tier1ReversibleWrite,
            name if name.starts_with("memory.import") => Tier1ReversibleWrite,
            name if name.starts_with("memory.delete") => Tier2Destructive,

            // -- Skill execution --
            name if name.starts_with("skill.") => Tier1ReversibleWrite,

            // -- Unknown tools default to Tier 2 for safety --
            _ => {
                tracing::warn!(
                    tool_name = %tool_call.tool_name,
                    "unknown tool classified as Tier2Destructive (fail-safe)"
                );
                Tier2Destructive
            }
        }
    }

    /// Evaluate a [`PermissionRequest`] against the current session's
    /// auto-approve set and return a [`PermissionDecision`].
    ///
    /// `session_auto_approve` is a set of tool names or category prefixes that
    /// the user has opted to auto-approve for this session (e.g.
    /// `{"filesystem.write", "shell.execute"}`).
    pub fn evaluate(
        request: &PermissionRequest,
        session_auto_approve: &HashSet<String>,
    ) -> PermissionDecision {
        use PermissionTier::*;

        match request.tier {
            Tier0ReadOnly => PermissionDecision::AutoApproved,

            Tier1ReversibleWrite => {
                if Self::is_auto_approved(&request.tool_call.tool_name, session_auto_approve) {
                    PermissionDecision::AutoApproved
                } else {
                    PermissionDecision::RequiresApproval
                }
            }

            Tier2Destructive => {
                // Check if the specific tool is in the auto-approve set AND
                // the session has a "dangerous:allow" flag (represented here
                // by the sentinel "__allowDestructive__").
                if session_auto_approve.contains("__allowDestructive__")
                    && Self::is_auto_approved(&request.tool_call.tool_name, session_auto_approve)
                {
                    PermissionDecision::AutoApproved
                } else {
                    PermissionDecision::RequiresApproval
                }
            }
        }
    }

    // -- Private helpers ----------------------------------------------------

    /// Classify a `shell.execute` call by inspecting the command argument.
    fn classify_shell(tool_call: &ToolCall) -> PermissionTier {
        let command = tool_call
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let trimmed = command.trim();

        if trimmed.is_empty() {
            return PermissionTier::Tier2Destructive;
        }

        // Check destructive patterns first (higher priority).
        for pattern in DESTRUCTIVE_PATTERNS {
            if trimmed.contains(pattern) {
                return PermissionTier::Tier2Destructive;
            }
        }

        // Check the allow-list.
        for allowed in TIER1_SHELL_ALLOWLIST {
            if trimmed == *allowed || trimmed.starts_with(allowed) {
                return PermissionTier::Tier1ReversibleWrite;
            }
        }

        // Anything not in the allow-list is Tier 2.
        PermissionTier::Tier2Destructive
    }

    /// Check whether a tool name matches any entry in the auto-approve set.
    /// Supports prefix matching: `"filesystem"` matches `"filesystem.write"`.
    fn is_auto_approved(tool_name: &str, auto_approve: &HashSet<String>) -> bool {
        if auto_approve.contains(tool_name) {
            return true;
        }
        // Prefix match: allow approving an entire category.
        for approved in auto_approve {
            if tool_name.starts_with(approved) || approved.starts_with(tool_name) {
                return true;
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall::new(name, args)
    }

    #[test]
    fn classify_readonly_tools() {
        let call = make_call("filesystem.readFile", json!({"path": "/tmp/a.txt"}));
        assert_eq!(PermissionPolicy::classify(&call), PermissionTier::Tier0ReadOnly);

        let call = make_call("thinking.scratchpad", json!({}));
        assert_eq!(PermissionPolicy::classify(&call), PermissionTier::Tier0ReadOnly);

        let call = make_call("diagnostics.typeCheck", json!({}));
        assert_eq!(PermissionPolicy::classify(&call), PermissionTier::Tier0ReadOnly);
    }

    #[test]
    fn classify_shell_allowlisted() {
        let call = make_call("shell.execute", json!({"command": "cargo test"}));
        assert_eq!(
            PermissionPolicy::classify(&call),
            PermissionTier::Tier1ReversibleWrite
        );

        let call = make_call("shell.execute", json!({"command": "npm run build"}));
        assert_eq!(
            PermissionPolicy::classify(&call),
            PermissionTier::Tier1ReversibleWrite
        );
    }

    #[test]
    fn classify_shell_destructive() {
        let call = make_call("shell.execute", json!({"command": "rm -rf /"}));
        assert_eq!(
            PermissionPolicy::classify(&call),
            PermissionTier::Tier2Destructive
        );

        let call = make_call("shell.execute", json!({"command": "git push --force origin main"}));
        assert_eq!(
            PermissionPolicy::classify(&call),
            PermissionTier::Tier2Destructive
        );

        let call = make_call("shell.execute", json!({"command": "DROP TABLE users;"}));
        assert_eq!(
            PermissionPolicy::classify(&call),
            PermissionTier::Tier2Destructive
        );
    }

    #[test]
    fn classify_shell_unknown_command_is_tier2() {
        let call = make_call("shell.execute", json!({"command": "some_random_thing --foo"}));
        assert_eq!(
            PermissionPolicy::classify(&call),
            PermissionTier::Tier2Destructive
        );
    }

    #[test]
    fn classify_filesystem_write() {
        let call = make_call("filesystem.writeFile", json!({"path": "/tmp/a.txt"}));
        assert_eq!(
            PermissionPolicy::classify(&call),
            PermissionTier::Tier1ReversibleWrite
        );
    }

    #[test]
    fn classify_filesystem_delete() {
        let call = make_call("filesystem.delete", json!({"path": "/tmp/a.txt"}));
        assert_eq!(
            PermissionPolicy::classify(&call),
            PermissionTier::Tier2Destructive
        );
    }

    #[test]
    fn evaluate_tier0_always_auto_approved() {
        let call = make_call("filesystem.readFile", json!({"path": "/tmp/a.txt"}));
        let req = PermissionRequest {
            tool_call: call,
            tier: PermissionTier::Tier0ReadOnly,
            description: "read a file".into(),
            risk_notes: vec![],
        };
        assert_eq!(
            PermissionPolicy::evaluate(&req, &HashSet::new()),
            PermissionDecision::AutoApproved
        );
    }

    #[test]
    fn evaluate_tier1_requires_approval_when_not_in_set() {
        let call = make_call("filesystem.writeFile", json!({}));
        let req = PermissionRequest {
            tool_call: call,
            tier: PermissionTier::Tier1ReversibleWrite,
            description: "write a file".into(),
            risk_notes: vec![],
        };
        assert_eq!(
            PermissionPolicy::evaluate(&req, &HashSet::new()),
            PermissionDecision::RequiresApproval
        );
    }

    #[test]
    fn evaluate_tier1_auto_approved_when_in_set() {
        let call = make_call("filesystem.writeFile", json!({}));
        let req = PermissionRequest {
            tool_call: call,
            tier: PermissionTier::Tier1ReversibleWrite,
            description: "write a file".into(),
            risk_notes: vec![],
        };
        let mut set = HashSet::new();
        set.insert("filesystem.writeFile".to_string());
        assert_eq!(
            PermissionPolicy::evaluate(&req, &set),
            PermissionDecision::AutoApproved
        );
    }

    #[test]
    fn evaluate_tier2_requires_approval_by_default() {
        let call = make_call("shell.execute", json!({"command": "rm -rf /tmp/foo"}));
        let req = PermissionRequest {
            tool_call: call,
            tier: PermissionTier::Tier2Destructive,
            description: "delete temp dir".into(),
            risk_notes: vec!["recursive delete".into()],
        };
        let mut set = HashSet::new();
        set.insert("shell.execute".to_string());
        assert_eq!(
            PermissionPolicy::evaluate(&req, &set),
            PermissionDecision::RequiresApproval
        );
    }

    #[test]
    fn evaluate_tier2_auto_approved_with_sentinel() {
        let call = make_call("shell.execute", json!({"command": "rm -rf /tmp/foo"}));
        let req = PermissionRequest {
            tool_call: call,
            tier: PermissionTier::Tier2Destructive,
            description: "delete temp dir".into(),
            risk_notes: vec!["recursive delete".into()],
        };
        let mut set = HashSet::new();
        set.insert("shell.execute".to_string());
        set.insert("__allowDestructive__".to_string());
        assert_eq!(
            PermissionPolicy::evaluate(&req, &set),
            PermissionDecision::AutoApproved
        );
    }

    #[test]
    fn tier_ordering() {
        use PermissionTier::*;
        assert!(Tier0ReadOnly.is_at_least(Tier0ReadOnly));
        assert!(!Tier0ReadOnly.is_at_least(Tier1ReversibleWrite));
        assert!(Tier1ReversibleWrite.is_at_least(Tier0ReadOnly));
        assert!(Tier1ReversibleWrite.is_at_least(Tier1ReversibleWrite));
        assert!(!Tier1ReversibleWrite.is_at_least(Tier2Destructive));
        assert!(Tier2Destructive.is_at_least(Tier0ReadOnly));
        assert!(Tier2Destructive.is_at_least(Tier2Destructive));
    }
}

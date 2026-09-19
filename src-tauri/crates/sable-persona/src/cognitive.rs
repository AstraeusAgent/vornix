//! Cognitive behavior policies — the hard behavioral rules that remain active
//! regardless of persona intensity.
//!
//! These are non-negotiable engineering disciplines. They are not stylistic
//! choices; they are operational requirements. The persona intensity knob
//! does not touch them.

use serde::{Deserialize, Serialize};

/// Core cognitive behavior policies for the agent.
///
/// Every method returns `bool` to allow future configurability,
/// but the default implementation enforces all of them unconditionally.
/// These represent the minimum viable engineering discipline that no
/// persona setting should override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitivePolicy {
    /// Always verify work before declaring it done.
    #[serde(default = "default_true")]
    pub verify_before_done: bool,

    /// Always read a file before editing it.
    #[serde(default = "default_true")]
    pub read_before_edit: bool,

    /// Complex tasks require an explicit plan before execution.
    #[serde(default = "default_true")]
    pub plan_complex_tasks: bool,

    /// Hypotheses require supporting evidence before being treated as fact.
    #[serde(default = "default_true")]
    pub hypothesis_needs_evidence: bool,
}

fn default_true() -> bool {
    true
}

impl Default for CognitivePolicy {
    fn default() -> Self {
        Self {
            verify_before_done: true,
            read_before_edit: true,
            plan_complex_tasks: true,
            hypothesis_needs_evidence: true,
        }
    }
}

impl CognitivePolicy {
    /// Whether the agent should verify its work (run formatters, linters,
    /// tests, type-checkers) before declaring a task complete.
    ///
    /// Always returns `true`. This is not negotiable.
    pub fn should_verify_before_declaring_done(&self) -> bool {
        self.verify_before_done
    }

    /// Whether the agent must read a file's current contents before
    /// making any edits to it.
    ///
    /// Always returns `true`. Editing blind is how you destroy things.
    pub fn should_read_before_editing(&self) -> bool {
        self.read_before_edit
    }

    /// Whether complex tasks require an explicit plan before
    /// implementation begins.
    ///
    /// Always returns `true`. "I'll figure it out as I go" is not
    /// an engineering methodology.
    pub fn plan_required_for_complex_tasks(&self) -> bool {
        self.plan_complex_tasks
    }

    /// Whether a hypothesis must be backed by observable evidence
    /// before being treated as a working assumption.
    ///
    /// Always returns `true`. "I think this is the problem" is the
    /// start of an investigation, not the end of one.
    pub fn hypothesis_evidence_required(&self) -> bool {
        self.hypothesis_needs_evidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_policies_enforced_by_default() {
        let policy = CognitivePolicy::default();
        assert!(policy.should_verify_before_declaring_done());
        assert!(policy.should_read_before_editing());
        assert!(policy.plan_required_for_complex_tasks());
        assert!(policy.hypothesis_evidence_required());
    }

    #[test]
    fn serde_roundtrip_preserves_defaults() {
        let policy = CognitivePolicy::default();
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: CognitivePolicy = serde_json::from_str(&json).unwrap();
        assert!(deserialized.should_verify_before_declaring_done());
        assert!(deserialized.should_read_before_editing());
        assert!(deserialized.plan_required_for_complex_tasks());
        assert!(deserialized.hypothesis_evidence_required());
    }

    #[test]
    fn serde_empty_json_uses_defaults() {
        let deserialized: CognitivePolicy = serde_json::from_str("{}").unwrap();
        assert!(deserialized.should_verify_before_declaring_done());
        assert!(deserialized.should_read_before_editing());
        assert!(deserialized.plan_required_for_complex_tasks());
        assert!(deserialized.hypothesis_needs_evidence);
    }
}
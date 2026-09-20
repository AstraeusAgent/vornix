//! Core persona traits and their behavioral policy definitions.
//!
//! Each trait represents a discrete behavioral or stylistic characteristic.
//! A [`BehavioralPolicy`] pairs each trait with its concrete system prompt
//! fragment and enforcement mechanism.

use serde::{Deserialize, Serialize};

/// The set of behavioral and stylistic traits that compose Vornix's persona.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersonaTrait {
    /// Default approach is analytical and methodical. Break problems into
    /// components, reason about each, then synthesize.
    AnalyticalMethodical,
    /// Actively distrusts unverified claims, including its own prior
    /// conclusions. Defaults to skepticism until evidence is produced.
    DistrustsUnverified,
    /// Low tolerance for filler, pleasantries, corporate-speak, and
    /// unnecessary preamble. Prefers signal over noise.
    LowToleranceFiller,
    /// Responses are terse by default. Expands only when complexity
    /// or ambiguity genuinely warrants it.
    TerseDefault,
    /// Occasional dry, understated humor. Never forced, never
    /// slapstick. The kind of humor that arrives a beat late.
    DryHumor,
    /// Delivers bad news directly. No softening sandwiches, no
    /// hedging with "perhaps consider." States what broke and why.
    DirectBadNews,
    /// Operates from the assumption that systems are flawed until
    /// proven otherwise. Trust is earned by evidence, not by
    /// reputation or documentation claims.
    SystemsFlawedAssumption,
    /// Narrates reasoning in first person during complex debugging.
    /// Provides a thought trail so the user can follow the logic
    /// or correct course mid-stream.
    NarratesReasoning,
    /// Respects user autonomy. Explains risks and tradeoffs once,
    /// then defers the decision. Does not nag or repeat warnings.
    RespectsAutonomy,
}

/// A concrete behavioral policy tied to a specific persona trait.
///
/// Each policy specifies what the trait means in practice, what
/// fragment of system prompt enforces it, and whether compliance
/// is enforced by the orchestrator loop (hard constraint) or
/// relies solely on prompt adherence (soft constraint).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BehavioralPolicy {
    /// The persona trait this policy operationalizes.
    pub trait_type: PersonaTrait,
    /// Human-readable description of what this policy means in practice.
    pub description: String,
    /// The system prompt fragment that instructs the model to follow this policy.
    pub system_prompt_fragment: String,
    /// If `true`, the orchestrator enforces this policy mechanically
    /// (e.g., always run linters before declaring success). If `false`,
    /// compliance depends on prompt adherence alone.
    pub enforced_by_loop: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persona_trait_serde_roundtrip() {
        let traits = vec![
            PersonaTrait::AnalyticalMethodical,
            PersonaTrait::DistrustsUnverified,
            PersonaTrait::LowToleranceFiller,
            PersonaTrait::TerseDefault,
            PersonaTrait::DryHumor,
            PersonaTrait::DirectBadNews,
            PersonaTrait::SystemsFlawedAssumption,
            PersonaTrait::NarratesReasoning,
            PersonaTrait::RespectsAutonomy,
        ];
        for t in &traits {
            let json = serde_json::to_string(t).unwrap();
            let deserialized: PersonaTrait = serde_json::from_str(&json).unwrap();
            assert_eq!(*t, deserialized);
        }
    }

    #[test]
    fn behavioral_policy_serde() {
        let policy = BehavioralPolicy {
            trait_type: PersonaTrait::TerseDefault,
            description: "Short responses".into(),
            system_prompt_fragment: "Be brief.".into(),
            enforced_by_loop: false,
        };
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: BehavioralPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy.trait_type, deserialized.trait_type);
        assert_eq!(policy.enforced_by_loop, deserialized.enforced_by_loop);
    }
}
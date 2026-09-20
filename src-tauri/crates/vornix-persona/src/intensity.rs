//! Persona intensity control - governs how much of Vornix's voice comes through.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Controls how strongly the persona's voice characteristics are expressed.
///
/// At all intensity levels, behavioral rules (verification discipline,
/// terseness-scaling, directness) remain active. This only affects
/// voice, tone, and stylistic expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersonaIntensity {
    /// Minimal voice. Behavioral rules only. No personality coloring.
    /// Suitable for CI output, log annotations, or contexts where
    /// a strong voice would be noise.
    Off,
    /// Moderate voice. Personality present but restrained.
    /// Less theatrical internal monologue. Still recognizably Vornix,
    /// but the volume knob is at 40%.
    Subtle,
    /// Full voice. All characteristics active, including dry humor,
    /// first-person reasoning narration, and the complete register.
    Full,
}

impl fmt::Display for PersonaIntensity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersonaIntensity::Off => write!(f, "off"),
            PersonaIntensity::Subtle => write!(f, "subtle"),
            PersonaIntensity::Full => write!(f, "full"),
        }
    }
}

impl Default for PersonaIntensity {
    fn default() -> Self {
        Self::Full
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_roundtrip() {
        assert_eq!(PersonaIntensity::Off.to_string(), "off");
        assert_eq!(PersonaIntensity::Subtle.to_string(), "subtle");
        assert_eq!(PersonaIntensity::Full.to_string(), "full");
    }

    #[test]
    fn serde_roundtrip() {
        for variant in [PersonaIntensity::Off, PersonaIntensity::Subtle, PersonaIntensity::Full] {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: PersonaIntensity = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, deserialized);
        }
    }

    #[test]
    fn serde_values() {
        assert_eq!(serde_json::to_string(&PersonaIntensity::Off).unwrap(), "\"off\"");
        assert_eq!(serde_json::to_string(&PersonaIntensity::Subtle).unwrap(), "\"subtle\"");
        assert_eq!(serde_json::to_string(&PersonaIntensity::Full).unwrap(), "\"full\"");
    }

    #[test]
    fn default_is_full() {
        assert_eq!(PersonaIntensity::default(), PersonaIntensity::Full);
    }
}
//! # vornix-persona
//!
//! The persona policy engine for the Vornix AI coding harness.
//!
//! This crate defines how Vornix communicates and approaches problems.
//! It controls voice, tone, behavioral discipline, and cognitive
//! policies through a configurable intensity system.
//!
//! ## Architecture
//!
//! - **`PersonaIntensity`** — the volume knob: Off, Subtle, or Full.
//! - **`PersonaPolicy`** — the engine that generates system prompts
//!   and behavioral instructions based on intensity.
//! - **`CognitivePolicy`** — hard behavioral rules that remain active
//!   regardless of intensity (verify before done, read before edit, etc.).
//! - **`BehavioralPolicy`** — individual trait definitions with their
//!   prompt fragments and enforcement mechanisms.
//!
//! ## Quick start
//!
//! ```rust
//! use vornix_persona::{PersonaPolicy, PersonaIntensity};
//!
//! let mut policy = PersonaPolicy::default_vornix();
//! policy.intensity = PersonaIntensity::Subtle;
//! let system_prompt = policy.generate_system_prompt();
//! let verification = policy.generate_verification_instruction();
//! let debugging = policy.generate_debugging_instruction();
//! ```

pub mod cognitive;
pub mod intensity;
pub mod policy;
pub mod traits;

pub use cognitive::CognitivePolicy;
pub use intensity::PersonaIntensity;
pub use policy::PersonaPolicy;
pub use traits::{BehavioralPolicy, PersonaTrait};
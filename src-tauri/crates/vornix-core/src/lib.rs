//! # vornix-core
//!
//! The agent-loop engine for the Vornix AI coding harness.
//!
//! This crate implements the core orchestration loop that drives the AI agent:
//!
//! - **State machine** ([`state`]) — tracks the agent's lifecycle from `Idle`
//!   through `Planning`, `Acting`, `Observing`, `Reflecting`, `Verifying`,
//!   and back.
//!
//! - **Loop detector** ([`loop_detector`]) — watches for repeated tool-call
//!   patterns that indicate the agent is stuck.
//!
//! - **Context builder** ([`context`]) — assembles the token window for each
//!   provider call and estimates usage via tiktoken.
//!
//! - **Turn** ([`turn`]) — captures all data for a single user→agent cycle.
//!
//! - **Orchestrator** ([`orchestrator`]) — the top-level entry point that
//!   ties everything together. Defines [`ChatProvider`] and [`ToolExecutor`]
//!   traits so the loop is decoupled from specific LLM backends or tool
//!   runtimes.

pub mod context;
pub mod loop_detector;
pub mod orchestrator;
pub mod state;
pub mod turn;

// Convenient re-exports of the most-used types at the crate root.
pub use context::ContextBuilder;
pub use loop_detector::{LoopCheckResult, LoopDetector};
pub use orchestrator::{
    ChatProvider, ChatStream, Orchestrator, OrchestratorConfig, OrchestratorEvent, ToolExecutor,
};
pub use state::{AgentState, AgentStateMachine, IterationStatus, StateMachineError, StateTransition};
pub use turn::{Turn, TurnUsage};

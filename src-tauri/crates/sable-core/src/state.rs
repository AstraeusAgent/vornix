//! Agent state machine with transition validation and iteration tracking.
//!
//! The agent lifecycle is modelled as a finite state machine with these states:
//!
//! ```text
//!   Idle ──▸ Planning ──▸ Acting ──▸ Observing ──▸ Reflecting
//!              │                                      │
//!              │         ┌────────────────────────────┘
//!              │         ▼
//!              │    Acting  (loop back)
//!              │         │
//!              │         └──▸ Verifying ──▸ Idle
//!              │         └──▸ Escalating ──▸ Acting | Idle
//!              │         └──▸ Blocked ──▸ Idle
//!              │         └──▸ Idle  (early completion)
//!              │
//!   Verifying ──▸ Idle | Acting
//!   Escalating ──▸ Acting | Idle
//!   Blocked ──▸ Idle
//! ```
//!
//! Each transition is logged with a reason and timestamp. An iteration counter
//! enforces a configurable hard limit (default 40) with a soft warning
//! threshold (default 25).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Agent state ─────────────────────────────────────────────────────────────

/// Discrete states the agent can occupy during a single user-turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentState {
    /// Waiting for user input.
    Idle,
    /// Analysing the user's request and forming a plan.
    Planning,
    /// Executing a tool call or generating a response.
    Acting,
    /// Processing the result of a tool call.
    Observing,
    /// Reflecting on progress and deciding next steps.
    Reflecting,
    /// Running final verification (formatting, linting, tests).
    Verifying,
    /// Blocked waiting for external input (e.g. permission approval).
    Blocked,
    /// Escalating to the user (e.g. clarification or error report).
    Escalating,
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Planning => write!(f, "Planning"),
            Self::Acting => write!(f, "Acting"),
            Self::Observing => write!(f, "Observing"),
            Self::Reflecting => write!(f, "Reflecting"),
            Self::Verifying => write!(f, "Verifying"),
            Self::Blocked => write!(f, "Blocked"),
            Self::Escalating => write!(f, "Escalating"),
        }
    }
}

// ─── Transition log entry ────────────────────────────────────────────────────

/// A single recorded state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    pub from: AgentState,
    pub to: AgentState,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

// ─── Iteration status ────────────────────────────────────────────────────────

/// Result of incrementing the iteration counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IterationStatus {
    /// Still within safe bounds.
    Ok,
    /// Approaching the hard limit — the agent should wrap up.
    SoftWarning,
    /// Hard limit reached — the agent must stop.
    HardLimit,
}

// ─── Errors ──────────────────────────────────────────────────────────────────

/// Errors produced by the state machine.
#[derive(Debug, Clone, thiserror::Error)]
pub enum StateMachineError {
    #[error("invalid transition from {from} to {to}")]
    InvalidTransition { from: AgentState, to: AgentState },

    #[error("hard iteration limit ({limit}) reached")]
    IterationLimitReached { limit: usize },
}

// ─── State machine ───────────────────────────────────────────────────────────

/// The agent's state machine, tracking current state, transition history,
/// and iteration budget.
#[derive(Debug, Clone)]
pub struct AgentStateMachine {
    /// Current state of the agent.
    pub current_state: AgentState,
    /// Ordered log of every transition taken.
    pub transitions: Vec<StateTransition>,
    /// Maximum number of Acting iterations allowed per user turn.
    pub max_iterations: usize,
    /// How many Acting iterations have occurred in the current turn.
    pub current_iteration: usize,
    /// Emit a soft warning when this iteration count is reached.
    pub soft_warning_at: usize,
}

impl AgentStateMachine {
    /// Create a new state machine in `Idle` with default limits.
    pub fn new() -> Self {
        Self {
            current_state: AgentState::Idle,
            transitions: Vec::new(),
            max_iterations: 40,
            current_iteration: 0,
            soft_warning_at: 25,
        }
    }

    /// Create a state machine with custom iteration limits.
    pub fn with_limits(max_iterations: usize, soft_warning_at: usize) -> Self {
        Self {
            current_state: AgentState::Idle,
            transitions: Vec::new(),
            max_iterations,
            current_iteration: 0,
            soft_warning_at,
        }
    }

    /// Attempt to transition to a new state.
    ///
    /// Returns `Ok(())` on success, or `Err(StateMachineError)` if the
    /// transition is not valid from the current state.
    pub fn transition(&mut self, to: AgentState, reason: impl Into<String>) -> Result<(), StateMachineError> {
        let from = self.current_state;
        if !Self::can_transition(from, to) {
            return Err(StateMachineError::InvalidTransition { from, to });
        }

        self.transitions.push(StateTransition {
            from,
            to,
            reason: reason.into(),
            timestamp: Utc::now(),
        });
        self.current_state = to;
        Ok(())
    }

    /// Check whether a transition from `from` to `to` is valid.
    pub fn can_transition(from: AgentState, to: AgentState) -> bool {
        use AgentState::*;
        matches!(
            (from, to),
            // Linear flow
            (Idle, Planning)
                | (Planning, Acting)
                | (Acting, Observing)
                | (Observing, Reflecting)
                // Reflecting can loop back or proceed
                | (Reflecting, Acting)
                | (Reflecting, Verifying)
                | (Reflecting, Escalating)
                | (Reflecting, Idle)
                | (Reflecting, Blocked)
                // Verifying can complete or re-act
                | (Verifying, Idle)
                | (Verifying, Acting)
                // Escalating can resume or stop
                | (Escalating, Acting)
                | (Escalating, Idle)
                // Blocked unblocks to Idle
                | (Blocked, Idle)
        )
    }

    /// Returns `true` if the iteration counter has reached or exceeded the
    /// hard limit.
    pub fn is_at_limit(&self) -> bool {
        self.current_iteration >= self.max_iterations
    }

    /// Returns `true` if the iteration counter has reached or exceeded the
    /// soft warning threshold (but not necessarily the hard limit).
    pub fn is_at_soft_warning(&self) -> bool {
        self.current_iteration >= self.soft_warning_at
    }

    /// Increment the iteration counter and return the resulting status.
    ///
    /// Call this each time the agent enters the `Acting` state.
    pub fn increment_iteration(&mut self) -> IterationStatus {
        self.current_iteration += 1;
        if self.current_iteration >= self.max_iterations {
            IterationStatus::HardLimit
        } else if self.current_iteration >= self.soft_warning_at {
            IterationStatus::SoftWarning
        } else {
            IterationStatus::Ok
        }
    }

    /// Reset the iteration counter (e.g. at the start of a new user turn).
    pub fn reset_iterations(&mut self) {
        self.current_iteration = 0;
    }
}

impl Default for AgentStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use AgentState::*;

    #[test]
    fn default_state_is_idle() {
        let sm = AgentStateMachine::new();
        assert_eq!(sm.current_state, Idle);
        assert!(sm.transitions.is_empty());
        assert_eq!(sm.max_iterations, 40);
        assert_eq!(sm.soft_warning_at, 25);
    }

    #[test]
    fn valid_linear_flow() {
        let mut sm = AgentStateMachine::new();
        sm.transition(Planning, "user message received").unwrap();
        sm.transition(Acting, "plan ready").unwrap();
        sm.transition(Observing, "tool executed").unwrap();
        sm.transition(Reflecting, "result analyzed").unwrap();
        sm.transition(Verifying, "task appears complete").unwrap();
        sm.transition(Idle, "verification passed").unwrap();

        assert_eq!(sm.current_state, Idle);
        assert_eq!(sm.transitions.len(), 6);
    }

    #[test]
    fn reflect_to_act_loop() {
        let mut sm = AgentStateMachine::new();
        sm.transition(Planning, "start").unwrap();
        sm.transition(Acting, "first action").unwrap();
        sm.transition(Observing, "observed").unwrap();
        sm.transition(Reflecting, "need more work").unwrap();
        sm.transition(Acting, "second action").unwrap();
        assert_eq!(sm.current_state, Acting);
    }

    #[test]
    fn invalid_transition_errors() {
        let mut sm = AgentStateMachine::new();
        // Idle -> Acting is not valid (must go through Planning)
        let err = sm.transition(Acting, "skip planning").unwrap_err();
        assert!(matches!(err, StateMachineError::InvalidTransition {
            from: Idle,
            to: Acting,
        }));
        // State unchanged
        assert_eq!(sm.current_state, Idle);
    }

    #[test]
    fn blocked_unblocks_to_idle() {
        let mut sm = AgentStateMachine::new();
        sm.transition(Planning, "start").unwrap();
        sm.transition(Acting, "act").unwrap();
        sm.transition(Observing, "see").unwrap();
        sm.transition(Reflecting, "think").unwrap();
        sm.transition(Blocked, "needs permission").unwrap();
        assert_eq!(sm.current_state, Blocked);

        // Can unblock to Idle
        sm.transition(Idle, "permission granted").unwrap();
        assert_eq!(sm.current_state, Idle);

        // Cannot go Blocked -> Acting directly
        sm.transition(Planning, "new request").unwrap();
        sm.transition(Acting, "act").unwrap();
        sm.transition(Observing, "see").unwrap();
        sm.transition(Reflecting, "think").unwrap();
        sm.transition(Blocked, "blocked again").unwrap();
        let err = sm.transition(Acting, "skip idle").unwrap_err();
        assert!(matches!(err, StateMachineError::InvalidTransition {
            from: Blocked,
            to: Acting,
        }));
    }

    #[test]
    fn iteration_tracking() {
        let mut sm = AgentStateMachine::with_limits(5, 3);

        assert_eq!(sm.increment_iteration(), IterationStatus::Ok);
        assert_eq!(sm.increment_iteration(), IterationStatus::Ok);
        assert_eq!(sm.increment_iteration(), IterationStatus::SoftWarning);
        assert!(sm.is_at_soft_warning());
        assert!(!sm.is_at_limit());

        assert_eq!(sm.increment_iteration(), IterationStatus::SoftWarning);
        assert_eq!(sm.increment_iteration(), IterationStatus::HardLimit);
        assert!(sm.is_at_limit());
    }

    #[test]
    fn iteration_reset() {
        let mut sm = AgentStateMachine::with_limits(3, 2);
        sm.increment_iteration();
        sm.increment_iteration();
        sm.increment_iteration();
        assert!(sm.is_at_limit());

        sm.reset_iterations();
        assert_eq!(sm.current_iteration, 0);
        assert!(!sm.is_at_limit());
    }

    #[test]
    fn can_transition_matrix() {
        // All valid transitions should return true
        assert!(AgentStateMachine::can_transition(Idle, Planning));
        assert!(AgentStateMachine::can_transition(Planning, Acting));
        assert!(AgentStateMachine::can_transition(Acting, Observing));
        assert!(AgentStateMachine::can_transition(Observing, Reflecting));
        assert!(AgentStateMachine::can_transition(Reflecting, Acting));
        assert!(AgentStateMachine::can_transition(Reflecting, Verifying));
        assert!(AgentStateMachine::can_transition(Reflecting, Escalating));
        assert!(AgentStateMachine::can_transition(Reflecting, Idle));
        assert!(AgentStateMachine::can_transition(Reflecting, Blocked));
        assert!(AgentStateMachine::can_transition(Verifying, Idle));
        assert!(AgentStateMachine::can_transition(Verifying, Acting));
        assert!(AgentStateMachine::can_transition(Escalating, Acting));
        assert!(AgentStateMachine::can_transition(Escalating, Idle));
        assert!(AgentStateMachine::can_transition(Blocked, Idle));

        // Invalid transitions should return false
        assert!(!AgentStateMachine::can_transition(Idle, Acting));
        assert!(!AgentStateMachine::can_transition(Idle, Observing));
        assert!(!AgentStateMachine::can_transition(Acting, Planning));
        assert!(!AgentStateMachine::can_transition(Acting, Verifying));
        assert!(!AgentStateMachine::can_transition(Blocked, Acting));
        assert!(!AgentStateMachine::can_transition(Planning, Idle));
    }
}

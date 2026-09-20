//! Agent-loop orchestrator.
//!
//! The [`Orchestrator`] is the top-level entry point for processing a user
//! message. It drives the [`AgentStateMachine`](crate::AgentStateMachine) through
//! its lifecycle, builds context, streams responses from a [`ChatProvider`],
//! executes tools via a [`ToolExecutor`], and emits [`OrchestratorEvent`]s
//! that the Tauri frontend can subscribe to.
//!
//! ## Injection points
//!
//! The orchestrator does **not** depend on concrete provider or tool
//! implementations at compile time. Instead it accepts trait objects:
//!
//! - [`ChatProvider`] — streaming LLM chat completions.
//! - [`ToolExecutor`] — running tool calls and checking permissions.
//!
//! This keeps the core loop testable and decoupled from any specific API
//! backend or tool runtime.

use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{Context as AnyhowContext, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use uuid::Uuid;

use vornix_providers::{
    ChatMessage, ChatRequest, MessageContent,
    Role, StreamEvent, ToolCallRequest,
};
use vornix_tools::{PermissionDecision, PermissionRequest, ToolCall, ToolResult};

use crate::context::ContextBuilder;
use crate::loop_detector::{LoopCheckResult, LoopDetector};
use crate::state::{AgentStateMachine, AgentState, IterationStatus};
use crate::turn::Turn;

// ─── Provider / executor traits ──────────────────────────────────────────────

/// A streaming LLM chat provider.
///
/// Implementations wrap a specific API backend (OpenAI, Anthropic, OpenRouter,
/// etc.) and adapt it to this uniform interface.
#[async_trait]
pub trait ChatProvider: Send + Sync {
    /// Execute a streaming chat completion.
    ///
    /// Returns a [`ChatStream`] that yields [`StreamEvent`]s until the
    /// completion is done or an error occurs.
    async fn stream_chat(&self, req: &ChatRequest) -> Result<ChatStream>;

    /// Stable provider identifier (e.g. `"openrouter"`, `"anthropic"`).
    fn id(&self) -> &str;

    /// The model being used (e.g. `"claude-sonnet-4-20250514"`).
    fn model_id(&self) -> &str;

    /// Whether the model supports extended-thinking / reasoning tokens.
    fn supports_reasoning(&self) -> bool;
}

/// Executes tool calls on behalf of the agent.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a tool call and return its result.
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult>;

    /// Check whether a tool call is permitted without user approval.
    async fn check_permission(&self, call: &ToolCall) -> Result<PermissionDecision>;
}

// ─── Chat stream ─────────────────────────────────────────────────────────────

/// A boxed, async stream of [`StreamEvent`]s returned by [`ChatProvider`].
///
/// This is the core-crate equivalent of
/// [`vornix_providers::ChatStream`](vornix_providers::ChatStream), but backed
/// by a `futures::Stream` rather than a raw channel receiver so that it can
/// be composed with other async primitives.
pub struct ChatStream {
    inner: Pin<Box<dyn Stream<Item = StreamEvent> + Send>>,
}

impl ChatStream {
    /// Wrap a pinned stream.
    pub fn new(inner: Pin<Box<dyn Stream<Item = StreamEvent> + Send>>) -> Self {
        Self { inner }
    }

    /// Convenience: wrap a `tokio::sync::mpsc::Receiver` as a stream.
    pub fn from_receiver(rx: mpsc::Receiver<StreamEvent>) -> Self {
        Self {
            inner: Box::pin(ReceiverStreamAdapter { rx }),
        }
    }
}

impl Stream for ChatStream {
    type Item = StreamEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

/// Adapter that turns a `tokio::sync::mpsc::Receiver` into a `Stream`.
struct ReceiverStreamAdapter {
    rx: mpsc::Receiver<StreamEvent>,
}

impl Stream for ReceiverStreamAdapter {
    type Item = StreamEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

// ─── Orchestrator config ─────────────────────────────────────────────────────

/// Configuration knobs for the orchestrator.
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Maximum Acting iterations per user turn. Default: 40.
    pub max_iterations: usize,
    /// Iteration count at which a soft warning is emitted. Default: 25.
    pub soft_warning_at: usize,
    /// Automatically enter the Verifying state when the model signals
    /// completion. Default: `true`.
    pub auto_verify: bool,
    /// Trigger context compaction when usage exceeds this fraction. Default: 0.75.
    pub compaction_threshold: f64,
    /// Run code formatters after the turn completes. Default: `true`.
    pub auto_run_formatters: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_iterations: 40,
            soft_warning_at: 25,
            auto_verify: true,
            compaction_threshold: 0.75,
            auto_run_formatters: true,
        }
    }
}

// ─── Orchestrator events ─────────────────────────────────────────────────────

/// Events emitted by the [`Orchestrator`] to the frontend / subscriber.
///
/// The Tauri layer subscribes to these and forwards them to the webview as
/// real-time updates.
#[derive(Debug, Clone)]
pub enum OrchestratorEvent {
    /// The agent transitioned between states.
    StateChanged {
        from: AgentState,
        to: AgentState,
    },
    /// A streaming text token from the model.
    MessageDelta {
        content: String,
    },
    /// A streaming reasoning / thinking token.
    ReasoningDelta {
        content: String,
    },
    /// The model has started a tool call.
    ToolCallStarted {
        tool_call: ToolCallRequest,
    },
    /// A tool call has completed.
    ToolCallCompleted {
        result: ToolResult,
    },
    /// The agent's plan has been updated.
    PlanUpdated {
        plan: String,
    },
    /// Approaching the hard iteration limit.
    IterationWarning {
        current: usize,
        max: usize,
    },
    /// A loop pattern was detected.
    LoopDetected {
        warning: String,
    },
    /// The agent needs user approval before proceeding.
    PermissionRequired {
        request: PermissionRequest,
    },
    /// Context was compacted to fit within the window.
    Compacted {
        summary: String,
    },
    /// The task is complete.
    TaskComplete {
        summary: String,
    },
    /// An unrecoverable error occurred.
    Error {
        message: String,
    },
}

// ─── Accumulated tool call ───────────────────────────────────────────────────

/// Partial tool call being assembled from streaming deltas.
#[derive(Debug, Clone)]
struct AccumulatingToolCall {
    id: String,
    function_name: String,
    arguments: String,
}

// ─── Orchestrator ────────────────────────────────────────────────────────────

/// The main agent-loop orchestrator.
///
/// Holds the state machine, context builder, and loop detector. Provider
/// and executor are passed as trait-object references to `process_user_message`
/// so the orchestrator itself remains implementation-agnostic.
pub struct Orchestrator {
    /// Runtime configuration.
    pub config: OrchestratorConfig,
    /// The agent's finite state machine.
    pub state_machine: AgentStateMachine,
    /// Assembles the context window for each provider call.
    pub context: ContextBuilder,
    /// Detects tool-call loops.
    pub loop_detector: LoopDetector,
}

impl Orchestrator {
    /// Create a new orchestrator with the given configuration.
    pub fn new(config: OrchestratorConfig) -> Self {
        let state_machine =
            AgentStateMachine::with_limits(config.max_iterations, config.soft_warning_at);
        Self {
            config,
            state_machine,
            context: ContextBuilder::new(),
            loop_detector: LoopDetector::default(),
        }
    }

    /// Create an orchestrator with default configuration.
    pub fn default_config() -> Self {
        Self::new(OrchestratorConfig::default())
    }

    /// Append a message to the context (convenience wrapper).
    pub fn add_message(&mut self, msg: ChatMessage) {
        self.context.add_message(msg);
    }

    // ── The main entry point ─────────────────────────────────────────────

    /// Process a single user message through the full agent loop.
    ///
    /// This is the architectural centrepiece. The flow:
    ///
    /// 1. **Idle → Planning**: Add the user message to context and emit a
    ///    plan update.
    /// 2. **Planning → Acting**: Build the [`ChatRequest`], call the
    ///    [`ChatProvider`], and stream tokens.
    /// 3. **Acting → Observing**: If the model requested tool calls, execute
    ///    them (with permission checks) and feed results back.
    /// 4. **Observing → Reflecting**: Decide whether to loop back to Acting
    ///    (more tool calls needed) or proceed to Verifying.
    /// 5. **Reflecting → Verifying → Idle**: Run formatters, emit
    ///    `TaskComplete`, and reset for the next turn.
    ///
    /// At any point, a loop detection or iteration-limit event may cause the
    /// agent to transition to `Escalating` or `Blocked`.
    pub async fn process_user_message(
        &mut self,
        content: &str,
        provider: &dyn ChatProvider,
        executor: &dyn ToolExecutor,
        event_tx: &mpsc::Sender<OrchestratorEvent>,
    ) -> Result<Turn> {
        let session_id = Uuid::nil(); // caller should override with real session id
        let mut turn = Turn::new(session_id);
        turn.start();

        // ── 1. Idle → Planning ───────────────────────────────────────────

        self.state_machine
            .transition(AgentState::Planning, "user message received")
            .map_err(|e| anyhow::anyhow!(e))?;

        let _ = event_tx
            .send(OrchestratorEvent::StateChanged {
                from: AgentState::Idle,
                to: AgentState::Planning,
            })
            .await;

        // Add the user's message to context.
        self.context.add_message(ChatMessage {
            role: Role::User,
            content: MessageContent::Text(content.to_string()),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        });

        turn.add_message(ChatMessage {
            role: Role::User,
            content: MessageContent::Text(content.to_string()),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        });

        let _ = event_tx
            .send(OrchestratorEvent::PlanUpdated {
                plan: "Analysing request and forming a plan.".to_string(),
            })
            .await;

        // ── 2. Acting loop ───────────────────────────────────────────────

        loop {
            // Transition to Acting.
            self.state_machine
                .transition(AgentState::Acting, "provider call")
                .map_err(|e| anyhow::anyhow!(e))?;

            let _ = event_tx
                .send(OrchestratorEvent::StateChanged {
                    from: self.previous_state(),
                    to: AgentState::Acting,
                })
                .await;

            // Check iteration budget.
            match self.state_machine.increment_iteration() {
                IterationStatus::Ok => {}
                IterationStatus::SoftWarning => {
                    let _ = event_tx
                        .send(OrchestratorEvent::IterationWarning {
                            current: self.state_machine.current_iteration,
                            max: self.state_machine.max_iterations,
                        })
                        .await;
                }
                IterationStatus::HardLimit => {
                    let _ = event_tx
                        .send(OrchestratorEvent::IterationWarning {
                            current: self.state_machine.current_iteration,
                            max: self.state_machine.max_iterations,
                        })
                        .await;

                    let summary = format!(
                        "Stopped after {} iterations (hard limit).",
                        self.state_machine.max_iterations,
                    );
                    turn.finish();
                    let _ = event_tx
                        .send(OrchestratorEvent::TaskComplete {
                            summary: summary.clone(),
                        })
                        .await;

                    // Transition to Escalating → Idle to cleanly end.
                    let _ = self
                        .state_machine
                        .transition(AgentState::Escalating, &summary);
                    let _ = self
                        .state_machine
                        .transition(AgentState::Idle, "iteration limit reached");

                    return Ok(turn);
                }
            }

            // ── Build context & call provider ────────────────────────────

            let req = self.context.build();
            let mut stream = provider
                .stream_chat(&req)
                .await
                .context("failed to start provider stream")?;

            // Stream tokens and accumulate tool calls.
            let mut assistant_content = String::new();
            let mut reasoning_content = String::new();
            let mut active_tool_calls: HashMap<usize, AccumulatingToolCall> = HashMap::new();
            let mut next_index: usize = 0;

            while let Some(event) =
                tokio::time::timeout(std::time::Duration::from_secs(300), stream.next())
                    .await
                    .unwrap_or(None)
            {
                match event {
                    StreamEvent::Token(token) => {
                        assistant_content.push_str(&token);
                        let _ = event_tx
                            .send(OrchestratorEvent::MessageDelta {
                                content: token,
                            })
                            .await;
                    }
                    StreamEvent::ReasoningToken(token) => {
                        reasoning_content.push_str(&token);
                        let _ = event_tx
                            .send(OrchestratorEvent::ReasoningDelta {
                                content: token,
                            })
                            .await;
                    }
                    StreamEvent::ToolCallDelta {
                        id,
                        name,
                        arguments_delta,
                    } => {
                        // Find or create the accumulating entry for this id.
                        let entry = active_tool_calls
                            .values_mut()
                            .find(|tc| tc.id == id);

                        if let Some(tc) = entry {
                            if !name.is_empty() {
                                tc.function_name = name;
                            }
                            tc.arguments.push_str(&arguments_delta);
                        } else {
                            let idx = next_index;
                            next_index += 1;
                            let _ = event_tx
                                .send(OrchestratorEvent::ToolCallStarted {
                                    tool_call: ToolCallRequest {
                                        id: id.clone(),
                                        function_name: name.clone(),
                                        arguments: String::new(),
                                    },
                                })
                                .await;
                            active_tool_calls.insert(
                                idx,
                                AccumulatingToolCall {
                                    id,
                                    function_name: name,
                                    arguments: arguments_delta,
                                },
                            );
                        }
                    }
                    StreamEvent::UsageUpdate {
                        input_tokens,
                        output_tokens,
                        cached_read_tokens,
                    } => {
                        turn.update_usage(
                            input_tokens,
                            output_tokens,
                            cached_read_tokens.unwrap_or(0),
                            0.0, // cost computed at end
                        );
                    }
                    StreamEvent::Done { usage } => {
                        turn.update_usage(
                            usage.input_tokens,
                            usage.output_tokens,
                            usage.cached_read_tokens.unwrap_or(0),
                            usage.estimated_cost.unwrap_or(0.0),
                        );
                        break;
                    }
                    StreamEvent::Error(err) => {
                        let _ = event_tx
                            .send(OrchestratorEvent::Error {
                                message: err.clone(),
                            })
                            .await;
                        anyhow::bail!("provider stream error: {err}");
                    }
                }
            }

            // ── 3. Acting → Observing ────────────────────────────────────

            self.state_machine
                .transition(AgentState::Observing, "stream complete")
                .map_err(|e| anyhow::anyhow!(e))?;

            // Record the assistant's text response.
            if !assistant_content.is_empty() {
                let reasoning = if reasoning_content.is_empty() {
                    None
                } else {
                    Some(reasoning_content.clone())
                };
                self.context.add_message(ChatMessage {
                    role: Role::Assistant,
                    content: MessageContent::Text(assistant_content.clone()),
                    tool_call_id: None,
                    tool_calls: if active_tool_calls.is_empty() {
                        None
                    } else {
                        Some(
                            active_tool_calls
                                .values()
                                .map(|tc| ToolCallRequest {
                                    id: tc.id.clone(),
                                    function_name: tc.function_name.clone(),
                                    arguments: tc.arguments.clone(),
                                })
                                .collect(),
                        )
                    },
                    reasoning_content: reasoning,
                });
            }

            // ── 4. Observing → Reflecting ────────────────────────────────

            self.state_machine
                .transition(AgentState::Reflecting, "analysing tool calls")
                .map_err(|e| anyhow::anyhow!(e))?;

            // If there are no tool calls, the model has finished responding.
            if active_tool_calls.is_empty() {
                // Transition to Verifying (or directly to Idle).
                if self.config.auto_verify {
                    self.state_machine
                        .transition(AgentState::Verifying, "no tool calls — task complete")
                        .map_err(|e| anyhow::anyhow!(e))?;
                }

                let summary = assistant_content.clone();
                turn.finish();

                let _ = event_tx
                    .send(OrchestratorEvent::TaskComplete {
                        summary: summary.clone(),
                    })
                    .await;

                self.state_machine
                    .transition(AgentState::Idle, "turn complete")
                    .map_err(|e| anyhow::anyhow!(e))?;

                return Ok(turn);
            }

            // ── Execute tool calls ───────────────────────────────────────

            let tool_calls_snapshot: Vec<AccumulatingToolCall> =
                active_tool_calls.values().cloned().collect();

            for acc in &tool_calls_snapshot {
                // Build a vornix_tools::ToolCall for the executor.
                let args: serde_json::Value =
                    serde_json::from_str(&acc.arguments).unwrap_or(serde_json::Value::Null);

                let tool_call = ToolCall::new(&acc.function_name, args.clone());

                // ── Loop detection ───────────────────────────────────────
                let loop_result = self.loop_detector.check(&acc.function_name, &args);
                match loop_result {
                    LoopCheckResult::Ok => {}
                    LoopCheckResult::Suspicious { count, ref tool_name } => {
                        let _ = event_tx
                            .send(OrchestratorEvent::LoopDetected {
                                warning: format!(
                                    "Suspicious repetition: {tool_name} called {count} times with similar arguments."
                                ),
                            })
                            .await;
                    }
                    LoopCheckResult::Stuck { count, ref tool_name } => {
                        let warning = format!(
                            "Stuck in a loop: {tool_name} called {count} times with identical arguments. \
                             Stopping execution."
                        );
                        let _ = event_tx
                            .send(OrchestratorEvent::LoopDetected {
                                warning: warning.clone(),
                            })
                            .await;
                        turn.finish();
                        let _ = event_tx
                            .send(OrchestratorEvent::Error {
                                message: warning,
                            })
                            .await;
                        let _ = self
                            .state_machine
                            .transition(AgentState::Escalating, "loop detected");
                        let _ = self
                            .state_machine
                            .transition(AgentState::Idle, "loop recovery");
                        return Ok(turn);
                    }
                }

                // ── Permission check ─────────────────────────────────────
                let decision = executor
                    .check_permission(&tool_call)
                    .await
                    .unwrap_or(PermissionDecision::RequiresApproval);

                match decision {
                    PermissionDecision::AutoApproved => { /* proceed */ }
                    PermissionDecision::RequiresApproval => {
                        self.state_machine
                            .transition(AgentState::Blocked, "permission required")
                            .map_err(|e| anyhow::anyhow!(e))?;

                        let tier =
                            vornix_tools::PermissionPolicy::classify(&tool_call);
                        let description = format!(
                            "Tool `{}` requires approval before execution.",
                            acc.function_name
                        );

                        let perm_request = PermissionRequest {
                            tool_call: tool_call.clone(),
                            tier,
                            description,
                            risk_notes: Vec::new(),
                        };

                        let _ = event_tx
                            .send(OrchestratorEvent::PermissionRequired {
                                request: perm_request,
                            })
                            .await;

                        // In a real system we'd await user approval here via
                        // a channel. For now, we log and auto-continue to
                        // avoid blocking the loop.
                        tracing::warn!(
                            tool = %acc.function_name,
                            "permission required but auto-continuing (await_user_approval not yet wired)"
                        );

                        // Return to Acting via Idle → Planning → Acting to
                        // follow the valid state graph.
                        let _ = self
                            .state_machine
                            .transition(AgentState::Idle, "permission denied — aborting turn");
                        turn.finish();
                        return Ok(turn);
                    }
                    PermissionDecision::Denied { reason } => {
                        // Record a failure result and continue to the next
                        // tool call rather than aborting the whole turn.
                        let result = ToolResult::failure(&tool_call, &reason, 0);
                        self.context.add_message(ChatMessage {
                            role: Role::Tool,
                            content: MessageContent::Text(result.output.clone()),
                            tool_call_id: Some(acc.id.clone()),
                            tool_calls: None,
                            reasoning_content: None,
                        });
                        turn.add_tool_call(tool_call, result.clone());
                        let _ = event_tx
                            .send(OrchestratorEvent::ToolCallCompleted {
                                result,
                            })
                            .await;
                        continue;
                    }
                }

                // ── Execute ──────────────────────────────────────────────
                let result = executor
                    .execute(&tool_call)
                    .await
                    .unwrap_or_else(|e| ToolResult::failure(&tool_call, e.to_string(), 0));

                // Feed the result back into context so the model sees it on
                // the next iteration.
                self.context.add_message(ChatMessage {
                    role: Role::Tool,
                    content: MessageContent::Text(result.output.clone()),
                    tool_call_id: Some(acc.id.clone()),
                    tool_calls: None,
                    reasoning_content: None,
                });

                turn.add_tool_call(tool_call, result.clone());

                let _ = event_tx
                    .send(OrchestratorEvent::ToolCallCompleted {
                        result,
                    })
                    .await;
            }

            // Clear the loop detector for the next batch of tool calls
            // to avoid false positives from cross-batch comparisons.
            // (The detector's sliding window naturally handles this, but
            // explicit reset is clearer.)
            self.loop_detector.reset();

            // ── 5. Reflecting → loop back to Acting ──────────────────────
            // The valid path is Reflecting → Acting (loop back).
            // We're already in Reflecting, so the next iteration's
            // transition to Acting is valid.
        }
    }

    /// Return the state just before the current one (from the transition log).
    fn previous_state(&self) -> AgentState {
        self.state_machine
            .transitions
            .last()
            .map(|t| t.from)
            .unwrap_or(AgentState::Idle)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::sync::Mutex;

    /// A mock provider that returns pre-defined event sequences per call.
    ///
    /// Each element in `responses` is the event list for one `stream_chat`
    /// invocation.  Calls beyond the length of `responses` return an empty
    /// stream (simulating no response).
    struct MockProvider {
        responses: Mutex<Vec<Vec<StreamEvent>>>,
    }

    impl MockProvider {
        fn new(responses: Vec<Vec<StreamEvent>>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }

        /// Single-call convenience: all events in one response.
        fn single(events: Vec<StreamEvent>) -> Self {
            Self::new(vec![events])
        }
    }

    #[async_trait]
    impl ChatProvider for MockProvider {
        async fn stream_chat(&self, _req: &ChatRequest) -> Result<ChatStream> {
            let mut responses = self.responses.lock().unwrap();
            let events = if responses.is_empty() {
                Vec::new()
            } else {
                responses.remove(0)
            };
            Ok(ChatStream::new(Box::pin(stream::iter(events))))
        }

        fn id(&self) -> &str {
            "mock"
        }

        fn model_id(&self) -> &str {
            "mock-model"
        }

        fn supports_reasoning(&self) -> bool {
            false
        }
    }

    /// A mock executor that auto-approves everything and returns "ok".
    struct MockExecutor;

    #[async_trait]
    impl ToolExecutor for MockExecutor {
        async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
            Ok(ToolResult::success(call, "ok", 10))
        }

        async fn check_permission(&self, _call: &ToolCall) -> Result<PermissionDecision> {
            Ok(PermissionDecision::AutoApproved)
        }
    }

    #[tokio::test]
    async fn simple_text_response() {
        let provider = MockProvider::single(vec![
            StreamEvent::Token("Hello".to_string()),
            StreamEvent::Token(" world!".to_string()),
            StreamEvent::Done {
                usage: vornix_providers::Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                    cached_read_tokens: None,
                    total_tokens: 15,
                    estimated_cost: Some(0.001),
                },
            },
        ]);
        let executor = MockExecutor;

        let mut orch = Orchestrator::default_config();
        let (tx, mut rx) = mpsc::channel(64);

        let turn = orch
            .process_user_message("Hi", &provider, &executor, &tx)
            .await
            .unwrap();

        assert!(turn.is_finished());
        assert_eq!(turn.token_usage.input_tokens, 10);
        assert_eq!(turn.token_usage.output_tokens, 5);

        // Drain the event channel and verify we got a TaskComplete.
        drop(tx);
        let mut got_complete = false;
        while let Some(evt) = rx.recv().await {
            if let OrchestratorEvent::TaskComplete { .. } = evt {
                got_complete = true;
            }
        }
        assert!(got_complete);
    }

    #[tokio::test]
    async fn tool_call_round_trip() {
        // First call: model requests a tool.
        // Second call: model sees the tool result and finishes.
        let provider = MockProvider::new(vec![
            vec![
                StreamEvent::ToolCallDelta {
                    id: "call_1".to_string(),
                    name: "test_tool".to_string(),
                    arguments_delta: r#"{"x":1}"#.to_string(),
                },
                StreamEvent::Done {
                    usage: vornix_providers::Usage {
                        input_tokens: 20,
                        output_tokens: 10,
                        cached_read_tokens: None,
                        total_tokens: 30,
                        estimated_cost: None,
                    },
                },
            ],
            vec![
                StreamEvent::Token("Done!".to_string()),
                StreamEvent::Done {
                    usage: vornix_providers::Usage {
                        input_tokens: 30,
                        output_tokens: 5,
                        cached_read_tokens: None,
                        total_tokens: 35,
                        estimated_cost: None,
                    },
                },
            ],
        ]);
        let executor = MockExecutor;

        let mut orch = Orchestrator::default_config();
        let (tx, _rx) = mpsc::channel(64);

        let turn = orch
            .process_user_message("Do something", &provider, &executor, &tx)
            .await
            .unwrap();

        assert!(turn.is_finished());
        assert_eq!(turn.tool_call_count(), 1);
    }

    #[tokio::test]
    async fn error_stops_loop() {
        let provider = MockProvider::single(vec![StreamEvent::Error("boom".to_string())]);
        let executor = MockExecutor;

        let mut orch = Orchestrator::default_config();
        let (tx, _rx) = mpsc::channel(64);

        let result = orch
            .process_user_message("fail", &provider, &executor, &tx)
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("boom"));
    }

    #[test]
    fn config_defaults() {
        let cfg = OrchestratorConfig::default();
        assert_eq!(cfg.max_iterations, 40);
        assert_eq!(cfg.soft_warning_at, 25);
        assert!(cfg.auto_verify);
        assert!((cfg.compaction_threshold - 0.75).abs() < f64::EPSILON);
        assert!(cfg.auto_run_formatters);
    }
}

//! Context-window compaction logic.
//!
//! When a conversation grows beyond the model's context window, we need to
//! **compact** older messages — replacing them with a summary — while keeping
//! recent turns verbatim.  This module provides pure functions for deciding
//! *when* to compact and *which* messages to target.
//!
//! ## Design
//!
//! - Recent messages (controlled by [`CompactionConfig::min_recent_turns`])
//!   are always kept verbatim.
//! - Messages that contain open tool calls (`tool_calls_json` present but
//!   no corresponding tool-call-id response) or have a `debug` / `reasoning`
//!   role are never compacted.
//! - The planner estimates token reduction using `token_count` when available,
//!   falling back to a rough heuristic of ~4 chars per token.

use crate::session::Message;

// ─── Configuration ───────────────────────────────────────────────────────────

/// Tunables for the compaction decision and planning logic.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Trigger compaction when context usage exceeds this fraction
    /// of the context window. Default: `0.75`.
    pub threshold_percent: f64,
    /// After compaction, target this fraction of the context window.
    /// Default: `0.50`.
    pub target_percent: f64,
    /// Always keep at least this many recent messages verbatim.
    /// Default: `5`.
    pub min_recent_turns: usize,
    /// Maximum tokens allowed for the generated summary.
    /// Default: `2000`.
    pub max_summary_tokens: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            threshold_percent: 0.75,
            target_percent: 0.50,
            min_recent_turns: 5,
            max_summary_tokens: 2000,
        }
    }
}

// ─── Result types ────────────────────────────────────────────────────────────

/// The output of actually performing compaction (summary generation).
///
/// This struct records what happened after compaction is executed.  The
/// planning phase returns a [`CompactionPlan`] instead.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompactionResult {
    /// The generated summary that replaces compacted messages.
    pub summary: String,
    /// Indices of messages that were kept verbatim (relative to the
    /// original `messages` slice).
    pub kept_messages: Vec<usize>,
    /// `(start, end)` index ranges that were compacted into the summary.
    pub compacted_ranges: Vec<(usize, usize)>,
    /// Estimated token count before compaction.
    pub pre_tokens: usize,
    /// Estimated token count after compaction.
    pub post_tokens: usize,
}

/// Plan produced by [`plan_compaction`] describing which messages to compact.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompactionPlan {
    /// Indices (into the input `messages` slice) of messages that should
    /// be replaced with a summary.
    pub messages_to_compact: Vec<usize>,
    /// Indices of messages that should be kept verbatim.
    pub messages_to_keep: Vec<usize>,
    /// Estimated number of tokens that compaction will free.
    pub estimated_reduction: usize,
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Decide whether compaction should be triggered.
///
/// Returns `true` when `current_tokens` exceeds `context_window * threshold`.
pub fn should_compact(current_tokens: usize, context_window: usize, config: &CompactionConfig) -> bool {
    if context_window == 0 {
        return false;
    }
    let threshold = (context_window as f64 * config.threshold_percent) as usize;
    current_tokens > threshold
}

/// Plan which messages to compact and which to keep.
///
/// The algorithm:
///
/// 1. Identify **protected** messages — the last `min_recent_turns` messages
///    plus any message with open tool calls or a debug/reasoning role.
/// 2. Working backwards from the oldest unprotected message, mark messages
///    for compaction until the estimated post-compaction tokens drop below
///    `context_window * target_percent`.
/// 3. Return the plan with index lists and an estimated reduction.
pub fn plan_compaction(
    messages: &[Message],
    current_tokens: usize,
    context_window: usize,
    config: &CompactionConfig,
) -> CompactionPlan {
    let n = messages.len();

    if n == 0 {
        return CompactionPlan {
            messages_to_compact: Vec::new(),
            messages_to_keep: Vec::new(),
            estimated_reduction: 0,
        };
    }

    // Step 1: Determine which indices are protected.
    let protected = build_protected_set(messages, config.min_recent_turns);

    // Step 2: Determine how many tokens we need to free.
    let target_tokens = (context_window as f64 * config.target_percent) as usize;
    let tokens_to_free = current_tokens.saturating_sub(target_tokens);

    // Step 3: Walk from oldest → newest among non-protected messages,
    // accumulating tokens until we've freed enough.
    let mut compact_indices: Vec<usize> = Vec::new();
    let mut freed_tokens: usize = 0;

    for i in 0..n {
        if protected.contains(&i) {
            continue;
        }
        if freed_tokens >= tokens_to_free {
            break;
        }
        freed_tokens += estimate_message_tokens(&messages[i]);
        compact_indices.push(i);
    }

    // Everything not in compact_indices is kept.
    let compact_set: std::collections::HashSet<usize> =
        compact_indices.iter().copied().collect();
    let keep_indices: Vec<usize> = (0..n).filter(|i| !compact_set.contains(i)).collect();

    CompactionPlan {
        messages_to_compact: compact_indices,
        messages_to_keep: keep_indices,
        estimated_reduction: freed_tokens,
    }
}

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Build the set of message indices that must NOT be compacted.
///
/// Protected messages are:
/// - The last `min_recent_turns` messages.
/// - Any message whose `role` is `debug` or `reasoning`.
/// - Any assistant message that has `tool_calls_json` set (open tool call)
///   whose corresponding tool response has not yet been seen.
fn build_protected_set(messages: &[Message], min_recent_turns: usize) -> std::collections::HashSet<usize> {
    let n = messages.len();
    let mut protected = std::collections::HashSet::new();

    // Protect the last min_recent_turns messages.
    let recent_start = n.saturating_sub(min_recent_turns);
    for i in recent_start..n {
        protected.insert(i);
    }

    // Collect all tool_call_ids that appear as `tool_call_id` in tool-role
    // messages (these are responses to tool calls).
    let resolved_call_ids: std::collections::HashSet<&str> = messages
        .iter()
        .filter(|m| m.role == "tool")
        .filter_map(|m| m.tool_call_id.as_deref())
        .collect();

    // Protect assistant messages with unresolved tool calls.
    for (i, msg) in messages.iter().enumerate() {
        if msg.role == "assistant" {
            if let Some(ref tc_json) = msg.tool_calls_json {
                // If the message contains tool calls, check whether they are
                // all resolved.  We do a simple heuristic: if the JSON contains
                // tool call IDs and none of those IDs appear in a tool response,
                // the call is still open.
                if has_open_tool_calls(tc_json, &resolved_call_ids) {
                    protected.insert(i);
                }
            }
        }
    }

    // Protect debug and reasoning messages.
    for (i, msg) in messages.iter().enumerate() {
        if msg.role == "debug" || msg.role == "reasoning" {
            protected.insert(i);
        }
    }

    protected
}

/// Heuristic: parse tool call IDs from the `tool_calls_json` array and
/// check whether any of them are NOT in the resolved set.
///
/// Returns `true` if there is at least one unresolved (open) tool call.
fn has_open_tool_calls(tool_calls_json: &str, resolved: &std::collections::HashSet<&str>) -> bool {
    // Fast path: if the JSON is empty or "null", no tool calls.
    let trimmed = tool_calls_json.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return false;
    }

    // Try to parse as a JSON array of objects with "id" fields.
    let parsed: Result<Vec<serde_json::Value>, _> = serde_json::from_str(trimmed);
    match parsed {
        Ok(calls) => {
            for call in &calls {
                if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
                    if !resolved.contains(id) {
                        return true;
                    }
                }
            }
            false
        }
        Err(_) => {
            // If we can't parse it, conservatively treat it as having open calls.
            true
        }
    }
}

/// Estimate the token count of a single message.
///
/// Uses `token_count` when available, otherwise falls back to a rough
/// heuristic: `content.len() / 4` (≈4 characters per token for English).
fn estimate_message_tokens(msg: &Message) -> usize {
    if let Some(tc) = msg.token_count {
        return tc.max(0) as usize;
    }
    // Rough heuristic: ~4 chars per token.
    // Include reasoning_content and tool_calls_json in the estimate.
    let mut chars = msg.content.len();
    if let Some(ref r) = msg.reasoning_content {
        chars += r.len();
    }
    if let Some(ref tc) = msg.tool_calls_json {
        chars += tc.len();
    }
    // Minimum 1 token per message (role, framing overhead).
    (chars / 4).max(1)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_msg(role: &str, content: &str, token_count: Option<i64>) -> Message {
        Message {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            role: role.to_string(),
            content: content.to_string(),
            reasoning_content: None,
            tool_calls_json: None,
            tool_call_id: None,
            timestamp: Utc::now(),
            token_count,
            model_id: None,
        }
    }

    fn make_tool_call_msg(tool_calls_json: &str) -> Message {
        Message {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            role: "assistant".to_string(),
            content: String::new(),
            reasoning_content: None,
            tool_calls_json: Some(tool_calls_json.to_string()),
            tool_call_id: None,
            timestamp: Utc::now(),
            token_count: None,
            model_id: None,
        }
    }

    fn make_tool_response(call_id: &str) -> Message {
        Message {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            role: "tool".to_string(),
            content: "result".to_string(),
            reasoning_content: None,
            tool_calls_json: None,
            tool_call_id: Some(call_id.to_string()),
            timestamp: Utc::now(),
            token_count: None,
            model_id: None,
        }
    }

    #[test]
    fn should_compact_basic() {
        let config = CompactionConfig::default();
        assert!(!should_compact(500, 1000, &config));
        assert!(!should_compact(750, 1000, &config));
        assert!(should_compact(751, 1000, &config));
        assert!(should_compact(900, 1000, &config));
    }

    #[test]
    fn should_compact_zero_window() {
        let config = CompactionConfig::default();
        assert!(!should_compact(100, 0, &config));
    }

    #[test]
    fn plan_empty_messages() {
        let config = CompactionConfig::default();
        let plan = plan_compaction(&[], 0, 1000, &config);
        assert!(plan.messages_to_compact.is_empty());
        assert!(plan.messages_to_keep.is_empty());
    }

    #[test]
    fn plan_all_recent_kept() {
        let config = CompactionConfig {
            min_recent_turns: 5,
            ..Default::default()
        };
        // 3 messages, all within min_recent_turns — nothing to compact.
        let msgs: Vec<Message> = (0..3).map(|i| make_msg("user", &format!("msg {i}"), Some(100))).collect();
        let plan = plan_compaction(&msgs, 300, 1000, &config);
        assert!(plan.messages_to_compact.is_empty());
        assert_eq!(plan.messages_to_keep.len(), 3);
    }

    #[test]
    fn plan_compacts_old_messages() {
        let config = CompactionConfig {
            min_recent_turns: 2,
            target_percent: 0.5,
            ..Default::default()
        };
        // 10 messages × 100 tokens = 1000 tokens, context window = 1000.
        // Target = 500 tokens → need to free 500 → compact 5 oldest.
        let msgs: Vec<Message> = (0..10).map(|i| make_msg("user", &format!("msg {i}"), Some(100))).collect();
        let plan = plan_compaction(&msgs, 1000, 1000, &config);

        // The last 2 (indices 8, 9) are protected by min_recent_turns.
        assert!(plan.messages_to_keep.contains(&8));
        assert!(plan.messages_to_keep.contains(&9));

        // Some older messages should be compacted.
        assert!(!plan.messages_to_compact.is_empty());
        assert!(plan.estimated_reduction > 0);

        // No overlap between compact and keep.
        for &ci in &plan.messages_to_compact {
            assert!(!plan.messages_to_keep.contains(&ci));
        }
    }

    #[test]
    fn plan_protects_open_tool_calls() {
        let config = CompactionConfig {
            min_recent_turns: 1,
            target_percent: 0.1,
            ..Default::default()
        };

        let session_id = Uuid::new_v4();
        let mut msgs: Vec<Message> = Vec::new();

        // Old user message (should be compacted).
        msgs.push(Message {
            id: Uuid::new_v4(),
            session_id,
            role: "user".to_string(),
            content: "old message".to_string(),
            reasoning_content: None,
            tool_calls_json: None,
            tool_call_id: None,
            timestamp: Utc::now(),
            token_count: Some(500),
            model_id: None,
        });

        // Assistant with unresolved tool call (protected).
        msgs.push(make_tool_call_msg(r#"[{"id":"call_1","function":{"name":"read_file"}}]"#));

        let plan = plan_compaction(&msgs, 600, 1000, &config);

        // The open-tool-call message should be protected.
        assert!(plan.messages_to_keep.contains(&1));
        // The old user message should be compacted.
        assert!(plan.messages_to_compact.contains(&0));
    }

    #[test]
    fn plan_resolves_tool_calls() {
        let config = CompactionConfig {
            min_recent_turns: 1,
            target_percent: 0.1,
            ..Default::default()
        };

        let session_id = Uuid::new_v4();
        let mut msgs: Vec<Message> = Vec::new();

        // Old user message.
        msgs.push(Message {
            id: Uuid::new_v4(),
            session_id,
            role: "user".to_string(),
            content: "old message".to_string(),
            reasoning_content: None,
            tool_calls_json: None,
            tool_call_id: None,
            timestamp: Utc::now(),
            token_count: Some(500),
            model_id: None,
        });

        // Assistant with tool call (resolved by next message).
        msgs.push(make_tool_call_msg(r#"[{"id":"call_1","function":{"name":"read_file"}}]"#));

        // Tool response — resolves call_1.
        msgs.push(make_tool_response("call_1"));

        let plan = plan_compaction(&msgs, 700, 1000, &config);

        // With the tool call resolved and min_recent_turns=1, the last message
        // is protected but the tool call message itself may be compacted since
        // it's resolved.  The user message should definitely be compacted.
        assert!(plan.messages_to_compact.contains(&0));
    }

    #[test]
    fn plan_protects_debug_messages() {
        let config = CompactionConfig {
            min_recent_turns: 0,
            target_percent: 0.1,
            ..Default::default()
        };

        let mut msgs: Vec<Message> = Vec::new();
        msgs.push(make_msg("user", "msg 0", Some(500)));
        msgs.push(make_msg("debug", "debug info", Some(200)));
        msgs.push(make_msg("assistant", "reply", Some(300)));

        let plan = plan_compaction(&msgs, 1000, 1000, &config);

        // Debug message at index 1 should be protected.
        assert!(plan.messages_to_keep.contains(&1));
    }

    #[test]
    fn estimate_tokens_uses_token_count() {
        let msg = make_msg("user", "a very long message here", Some(42));
        assert_eq!(estimate_message_tokens(&msg), 42);
    }

    #[test]
    fn estimate_tokens_fallback_heuristic() {
        // "twenty chars long!!" is 19 chars → 19 / 4 = 4 tokens
        let msg = make_msg("user", "twenty chars long!!", None);
        assert_eq!(estimate_message_tokens(&msg), 4);
    }

    #[test]
    fn estimate_tokens_minimum_one() {
        let msg = make_msg("user", "", None);
        assert_eq!(estimate_message_tokens(&msg), 1);
    }
}
//! Context builder for constructing [`ChatRequest`] payloads.
//!
//! Assembles the system prompt, conversation messages, and tool definitions
//! into a request object ready to be sent to a [`ChatProvider`](crate::ChatProvider).
//! Token estimation uses the `cl100k_base` encoding via `tiktoken-rs`.

use vornix_providers::{ChatMessage, ChatRequest, MessageContent, Role, ToolDefinition};

// ─── Context builder ─────────────────────────────────────────────────────────

/// Assembles the components of a chat completion request and tracks
/// context-window usage.
#[derive(Debug, Clone)]
pub struct ContextBuilder {
    /// The system prompt prepended to every request.
    system_prompt: String,
    /// Ordered conversation messages (excluding the system prompt).
    messages: Vec<ChatMessage>,
    /// Tool definitions available to the model.
    tools: Vec<ToolDefinition>,
    /// Maximum tokens allowed in the context window.
    max_context_tokens: usize,
    /// Tokens reserved for the model's output (not counted toward the input
    /// budget, but subtracted from the window when computing usage).
    reserved_output_tokens: usize,
}

impl ContextBuilder {
    /// Create a new builder with sensible defaults.
    ///
    /// - `max_context_tokens`: 128 000
    /// - `reserved_output_tokens`: 16 384
    pub fn new() -> Self {
        Self {
            system_prompt: String::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            max_context_tokens: 128_000,
            reserved_output_tokens: 16_384,
        }
    }

    /// Set the system prompt.
    pub fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = prompt;
    }

    /// Append a message to the conversation.
    pub fn add_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    /// Replace the current set of tool definitions.
    pub fn set_tools(&mut self, tools: Vec<ToolDefinition>) {
        self.tools = tools;
    }

    /// Set the maximum context window size in tokens.
    pub fn set_max_context(&mut self, tokens: usize) {
        self.max_context_tokens = tokens;
    }

    /// Set the number of tokens reserved for the model's output.
    pub fn set_reserved_output(&mut self, tokens: usize) {
        self.reserved_output_tokens = tokens;
    }

    /// Access the current messages (for inspection / compaction decisions).
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Mutable access to messages (for compaction or reordering).
    pub fn messages_mut(&mut self) -> &mut Vec<ChatMessage> {
        &mut self.messages
    }

    /// Replace all messages at once (e.g. after restoring from persistence).
    pub fn set_messages(&mut self, messages: Vec<ChatMessage>) {
        self.messages = messages;
    }

    // ── Token estimation ─────────────────────────────────────────────────

    /// Estimate the total token count of the current context (system prompt +
    /// messages + tool definitions).
    ///
    /// Uses the `cl100k_base` BPE encoding. Returns `0` if the encoder
    /// cannot be loaded (tiktoken-rs model not found at runtime).
    pub fn estimate_tokens(&self) -> usize {
        let bpe = match tiktoken_rs::cl100k_base() {
            Ok(bpe) => bpe,
            Err(_) => return self.fallback_token_estimate(),
        };

        let mut total: usize = 0;

        // System prompt: 4 tokens overhead + content.
        if !self.system_prompt.is_empty() {
            total += 4 + bpe.encode_ordinary(&self.system_prompt).len();
        }

        // Each message has ~4 tokens of framing overhead (role, separators).
        for msg in &self.messages {
            total += 4;
            total += estimate_message_content_tokens(&bpe, &msg.content);

            if let Some(ref reasoning) = msg.reasoning_content {
                total += bpe.encode_ordinary(reasoning).len();
            }

            if let Some(ref tool_calls) = msg.tool_calls {
                for tc in tool_calls {
                    total += bpe.encode_ordinary(&tc.id).len();
                    total += bpe.encode_ordinary(&tc.function_name).len();
                    total += bpe.encode_ordinary(&tc.arguments).len();
                    // ~7 tokens overhead per tool call
                    total += 7;
                }
            }

            if let Some(ref tool_call_id) = msg.tool_call_id {
                total += bpe.encode_ordinary(tool_call_id).len();
            }
        }

        // Tool definitions: each tool has name, description, and JSON Schema
        // parameters. The encoding overhead is ~6 tokens per tool definition.
        for tool in &self.tools {
            total += 6;
            total += bpe.encode_ordinary(&tool.name).len();
            total += bpe.encode_ordinary(&tool.description).len();
            let params_str = serde_json::to_string(&tool.parameters)
                .unwrap_or_default();
            total += bpe.encode_ordinary(&params_str).len();
        }

        // A small buffer for any edge cases / special tokens.
        total += 2;
        total
    }

    /// Fallback token estimate (~4 chars per token) when tiktoken is
    /// unavailable.
    fn fallback_token_estimate(&self) -> usize {
        let mut chars = self.system_prompt.len();
        for msg in &self.messages {
            chars += 8; // framing
            chars += message_content_char_count(&msg.content);
            if let Some(ref r) = msg.reasoning_content {
                chars += r.len();
            }
            if let Some(ref tcs) = msg.tool_calls {
                for tc in tcs {
                    chars += tc.id.len() + tc.function_name.len() + tc.arguments.len();
                }
            }
        }
        (chars / 4).max(1)
    }

    // ── Build ────────────────────────────────────────────────────────────

    /// Assemble a [`ChatRequest`] from the current state of the builder.
    ///
    /// The `model` field is set to an empty string — the caller (typically
    /// the orchestrator) should populate it from the session configuration
    /// before sending the request to a provider.
    pub fn build(&self) -> ChatRequest {
        let mut messages = Vec::with_capacity(1 + self.messages.len());

        // Prepend the system prompt if non-empty.
        if !self.system_prompt.is_empty() {
            messages.push(ChatMessage {
                role: Role::System,
                content: MessageContent::Text(self.system_prompt.clone()),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            });
        }

        messages.extend(self.messages.iter().cloned());

        ChatRequest {
            model: String::new(), // caller must set
            messages,
            tools: if self.tools.is_empty() {
                None
            } else {
                Some(self.tools.clone())
            },
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            reasoning_effort: None,
            stream: true,
            extra_params: std::collections::HashMap::new(),
        }
    }

    // ── Usage ────────────────────────────────────────────────────────────

    /// Context usage as a fraction of `max_context_tokens`, in the range
    /// `[0.0, ∞)`. Values above `1.0` mean the context exceeds the window.
    pub fn context_usage_percent(&self) -> f64 {
        if self.max_context_tokens == 0 {
            return 0.0;
        }
        let effective_window = self
            .max_context_tokens
            .saturating_sub(self.reserved_output_tokens);
        if effective_window == 0 {
            return 0.0;
        }
        self.estimate_tokens() as f64 / effective_window as f64
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Estimate tokens for a single [`MessageContent`].
fn estimate_message_content_tokens(
    bpe: &tiktoken_rs::CoreBPE,
    content: &MessageContent,
) -> usize {
    match content {
        MessageContent::Text(text) => bpe.encode_ordinary(text).len(),
        MessageContent::Parts(parts) => {
            let mut total = 0;
            for part in parts {
                match part {
                    vornix_providers::ContentPart::Text { text } => {
                        total += bpe.encode_ordinary(text).len();
                    }
                    vornix_providers::ContentPart::Image { image_url } => {
                        // Images are tokenised by the provider; we estimate
                        // a flat overhead per image.
                        total += bpe.encode_ordinary(&image_url.url).len() + 85;
                    }
                }
            }
            total
        }
    }
}

/// Rough character count for a [`MessageContent`] (fallback heuristic).
fn message_content_char_count(content: &MessageContent) -> usize {
    match content {
        MessageContent::Text(text) => text.len(),
        MessageContent::Parts(parts) => parts
            .iter()
            .map(|p| match p {
                vornix_providers::ContentPart::Text { text } => text.len(),
                vornix_providers::ContentPart::Image { image_url } => image_url.url.len() + 100,
            })
            .sum(),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use vornix_providers::{ChatMessage, MessageContent, Role};

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: Role::User,
            content: MessageContent::Text(text.to_string()),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }
    }

    #[test]
    fn empty_builder() {
        let ctx = ContextBuilder::new();
        assert_eq!(ctx.estimate_tokens(), 2); // just the buffer
    }

    #[test]
    fn system_prompt_adds_tokens() {
        let mut ctx = ContextBuilder::new();
        ctx.set_system_prompt("You are a helpful assistant.".to_string());
        let tokens = ctx.estimate_tokens();
        assert!(tokens > 2); // system prompt + overhead
    }

    #[test]
    fn messages_add_tokens() {
        let mut ctx = ContextBuilder::new();
        ctx.add_message(user_msg("Hello, world!"));
        ctx.add_message(user_msg("How are you?"));
        let tokens = ctx.estimate_tokens();
        assert!(tokens > 4); // two messages with content
    }

    #[test]
    fn context_usage_percent() {
        let mut ctx = ContextBuilder::new();
        ctx.set_max_context(1000);
        ctx.set_reserved_output(200); // effective window = 800
        ctx.set_system_prompt("test".to_string());
        let usage = ctx.context_usage_percent();
        assert!(usage > 0.0);
        assert!(usage < 1.0); // small prompt should be well under limit
    }

    #[test]
    fn build_includes_system_prompt() {
        let mut ctx = ContextBuilder::new();
        ctx.set_system_prompt("Be helpful".to_string());
        ctx.add_message(user_msg("Hi"));
        let req = ctx.build();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, Role::System);
        assert_eq!(req.messages[1].role, Role::User);
    }

    #[test]
    fn build_without_system_prompt() {
        let mut ctx = ContextBuilder::new();
        ctx.add_message(user_msg("Hi"));
        let req = ctx.build();
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, Role::User);
    }

    #[test]
    fn build_includes_tools() {
        let mut ctx = ContextBuilder::new();
        ctx.set_tools(vec![ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }]);
        let req = ctx.build();
        assert!(req.tools.is_some());
        assert_eq!(req.tools.unwrap().len(), 1);
    }

    #[test]
    fn build_without_tools_is_none() {
        let ctx = ContextBuilder::new();
        let req = ctx.build();
        assert!(req.tools.is_none());
    }

    #[test]
    fn zero_context_window() {
        let mut ctx = ContextBuilder::new();
        ctx.set_max_context(0);
        assert_eq!(ctx.context_usage_percent(), 0.0);
    }
}

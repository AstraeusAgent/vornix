use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

// ── Credentials ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCredentials {
    pub api_key: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
}

// ── Role ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

// ── Message content ──────────────────────────────────────────────────────────

/// A single content part inside a multimodal message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    Image { image_url: ImageUrl },
}

/// URL wrapper used by the OpenAI-compatible `image_url` content part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

/// Either a plain string or an array of multimodal content parts.
///
/// Serialises as a bare string when `Text`, and as a JSON array when `Parts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

// ── Chat message ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRequest>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

// ── Tooling ──────────────────────────────────────────────────────────────────

/// Describes a tool (function) the model may call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the function parameters.
    pub parameters: serde_json::Value,
}

/// A tool-call request emitted by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub id: String,
    pub function_name: String,
    /// Raw JSON string of arguments.
    pub arguments: String,
}

// ── Reasoning effort ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

// ── Chat request ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    pub stream: bool,
    /// Additional provider-specific parameters merged into the API body.
    #[serde(skip)]
    pub extra_params: HashMap<String, serde_json::Value>,
}

impl ChatRequest {
    /// Serialise to a JSON value suitable for the wire, merging `extra_params`
    /// on top of the normal fields.
    pub fn to_api_body(&self) -> serde_json::Value {
        let mut body = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        if let serde_json::Value::Object(ref mut map) = body {
            for (k, v) in &self.extra_params {
                map.insert(k.clone(), v.clone());
            }
        }
        body
    }
}

// ── Model metadata ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    /// Cost in USD per 1 M prompt tokens.
    pub prompt_per_million: f64,
    /// Cost in USD per 1 M completion tokens.
    pub completion_per_million: f64,
    /// Cost in USD per 1 M cached-read tokens (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_read_per_million: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub provider_id: String,
    pub context_length: u32,
    pub max_output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing: Option<ModelPricing>,
    pub supported_parameters: HashSet<String>,
    pub input_modalities: Vec<String>,
    pub is_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_status: Option<String>,
}

// ── Model capabilities ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub supports_tools: bool,
    pub supports_reasoning: bool,
    pub supports_streaming: bool,
    pub supports_vision: bool,
    pub supports_json_mode: bool,
}

// ── Usage ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_read_tokens: Option<u32>,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,
}

// ── Streaming ────────────────────────────────────────────────────────────────

/// Events emitted by a streaming chat completion.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A regular text token.
    Token(String),
    /// A reasoning / thinking token (visible in extended-thinking models).
    ReasoningToken(String),
    /// Incremental tool-call information.
    ToolCallDelta {
        id: String,
        name: String,
        arguments_delta: String,
    },
    /// Intermediate usage numbers (some providers emit these mid-stream).
    UsageUpdate {
        input_tokens: u32,
        output_tokens: u32,
        cached_read_tokens: Option<u32>,
    },
    /// Stream finished successfully.
    Done { usage: Usage },
    /// An error occurred during streaming.
    Error(String),
}

/// Receiver half of a streaming chat completion.
pub struct ChatStream {
    rx: tokio::sync::mpsc::Receiver<StreamEvent>,
}

impl ChatStream {
    /// Create a new [`ChatStream`] wrapping the given receiver.
    pub fn new(rx: tokio::sync::mpsc::Receiver<StreamEvent>) -> Self {
        Self { rx }
    }

    /// Await the next event. Returns `None` when the stream is exhausted.
    pub async fn recv(&mut self) -> Option<StreamEvent> {
        self.rx.recv().await
    }
}

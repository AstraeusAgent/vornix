//! # sable-providers
//!
//! Provider abstraction layer for the Sable AI coding harness.
//!
//! Defines the [`Provider`] trait that every LLM backend must implement,
//! along with the shared wire types used in chat requests and streaming
//! responses.

pub mod provider;
pub mod types;

pub use provider::{HealthStatus, Provider};
pub use types::{
    ChatRequest, ChatStream, ChatMessage, ContentPart, ImageUrl, MessageContent,
    ModelCapabilities, ModelInfo, ModelPricing, ProviderCredentials, ReasoningEffort,
    Role, StreamEvent, ToolCallRequest, ToolDefinition, Usage,
};

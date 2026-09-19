use async_trait::async_trait;

use crate::types::*;

/// Health status returned by [`Provider::health_check`].
#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unavailable { error: String },
}

/// Unified interface every LLM provider must implement.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Short, stable identifier (e.g. `"openrouter"`, `"opencode-go"`).
    fn id(&self) -> &'static str;

    /// Human-readable name (e.g. `"OpenRouter"`).
    fn name(&self) -> &'static str;

    /// Fetch the catalogue of models available through this provider.
    async fn list_models(&self, creds: &ProviderCredentials) -> anyhow::Result<Vec<ModelInfo>>;

    /// Execute a streaming chat completion.
    async fn stream_chat(
        &self,
        req: &ChatRequest,
        creds: &ProviderCredentials,
    ) -> anyhow::Result<ChatStream>;

    /// Derive the capabilities of a given model from its metadata.
    fn capabilities(&self, model: &ModelInfo) -> ModelCapabilities;

    /// Run a lightweight probe to determine whether a model is reachable and
    /// functioning.
    async fn health_check(
        &self,
        model_id: &str,
        creds: &ProviderCredentials,
    ) -> anyhow::Result<HealthStatus>;
}

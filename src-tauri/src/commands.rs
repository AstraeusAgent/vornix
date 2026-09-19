use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sable_persona::intensity::PersonaIntensity;
use tauri::State;

#[derive(Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub title: Option<String>,
    pub model_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct MessageInfo {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

#[tauri::command]
pub async fn session_create(
    state: State<'_, AppState>,
    title: Option<String>,
    model_id: Option<String>,
    provider_id: Option<String>,
    project_path: Option<String>,
) -> Result<SessionInfo, String> {
    let session = state
        .sessions
        .create_session(
            title.as_deref(),
            model_id.as_deref(),
            provider_id.as_deref(),
            project_path.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(SessionInfo {
        id: session.id.to_string(),
        title: session.title,
        model_id: session.model_id,
        created_at: session.created_at.to_rfc3339(),
        updated_at: session.updated_at.to_rfc3339(),
    })
}

#[tauri::command]
pub async fn session_list(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<SessionInfo>, String> {
    let sessions = state
        .sessions
        .list_sessions(limit.unwrap_or(50), offset.unwrap_or(0))
        .await
        .map_err(|e| e.to_string())?;

    Ok(sessions
        .into_iter()
        .map(|s| SessionInfo {
            id: s.id.to_string(),
            title: s.title,
            model_id: s.model_id,
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        })
        .collect())
}

#[tauri::command]
pub async fn session_get(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<SessionInfo>, String> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let session = state
        .sessions
        .get_session(uuid)
        .await
        .map_err(|e| e.to_string())?;

    Ok(session.map(|s| SessionInfo {
        id: s.id.to_string(),
        title: s.title,
        model_id: s.model_id,
        created_at: s.created_at.to_rfc3339(),
        updated_at: s.updated_at.to_rfc3339(),
    }))
}

#[tauri::command]
pub async fn session_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    state
        .sessions
        .delete_session(uuid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn session_get_messages(
    state: State<'_, AppState>,
    session_id: String,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<MessageInfo>, String> {
    let uuid = uuid::Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let messages = state
        .sessions
        .get_messages(uuid, limit.unwrap_or(100), offset.unwrap_or(0))
        .await
        .map_err(|e| e.to_string())?;

    Ok(messages
        .into_iter()
        .map(|m| MessageInfo {
            id: m.id.to_string(),
            role: m.role,
            content: m.content,
            timestamp: m.timestamp.to_rfc3339(),
        })
        .collect())
}

#[derive(Serialize, Deserialize)]
pub struct ProviderKeyInfo {
    pub provider_id: String,
    pub has_key: bool,
}

#[tauri::command]
pub async fn provider_list_models(
    _state: State<'_, AppState>,
    provider_id: String,
) -> Result<String, String> {
    match provider_id.as_str() {
        "openrouter" => {
            let client = reqwest::Client::new();
            let resp = client
                .get("https://openrouter.ai/api/v1/models")
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let body = resp.text().await.map_err(|e| e.to_string())?;
            Ok(body)
        }
        "opencode-go" => {
            let client = reqwest::Client::new();
            let resp = client
                .get("https://opencode.ai/zen/go/v1/models")
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let body = resp.text().await.map_err(|e| e.to_string())?;
            Ok(body)
        }
        _ => Err(format!("unknown provider: {}", provider_id)),
    }
}

#[derive(Serialize, Clone)]
pub struct ModelSummary {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub context_length: u64,
    pub prompt_price: Option<f64>,
    pub completion_price: Option<f64>,
    pub supports_reasoning: bool,
    pub supports_tools: bool,
}

/// Parsed model list for the picker UI, merged across providers.
#[tauri::command]
pub async fn list_models(provider_id: String) -> Result<Vec<ModelSummary>, String> {
    let client = reqwest::Client::new();

    let fetch = |url: &str| {
        let client = client.clone();
        let url = url.to_string();
        async move { client.get(&url).send().await?.text().await }
    };

    let body = match provider_id.as_str() {
        "openrouter" => fetch("https://openrouter.ai/api/v1/models")
            .await
            .map_err(|e| e.to_string())?,
        "opencode-go" => fetch("https://opencode.ai/zen/go/v1/models")
            .await
            .map_err(|e| e.to_string())?,
        _ => return Err(format!("unknown provider: {}", provider_id)),
    };

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("failed to parse models response: {}", e))?;

    let entries = json
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();

    let mut models = Vec::new();
    for entry in entries {
        let id = match entry.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };
        let name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();
        let context_length = entry
            .get("context_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // OpenRouter pricing is per-token strings; compute per-million
        let pricing = entry.get("pricing");
        let per_million = |key: &str| -> Option<f64> {
            let raw = pricing?.get(key)?.as_str()?;
            let v: f64 = raw.parse().ok()?;
            Some(v * 1_000_000.0)
        };
        let prompt_price = per_million("prompt");
        let completion_price = per_million("completion");

        let supported = entry
            .get("supported_parameters")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let supports_reasoning = supported
            .iter()
            .any(|p| p.as_str() == Some("reasoning") || p.as_str() == Some("reasoning_effort"));
        let supports_tools = supported.iter().any(|p| p.as_str() == Some("tools"));

        models.push(ModelSummary {
            id,
            name,
            provider_id: provider_id.clone(),
            context_length,
            prompt_price,
            completion_price,
            supports_reasoning,
            supports_tools,
        });
    }

    models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(models)
}

fn key_name_for(provider_id: &str) -> Result<&'static str, String> {
    match provider_id {
        "openrouter" => Ok("openrouter_api_key"),
        "opencode-go" => Ok("opencode_go_api_key"),
        "github" => Ok("github_token"),
        _ => Err(format!("unknown provider: {}", provider_id)),
    }
}

#[tauri::command]
pub async fn provider_set_key(
    state: State<'_, AppState>,
    provider_id: String,
    api_key: String,
) -> Result<(), String> {
    let key_name = key_name_for(&provider_id)?;
    let vault = state.vault.lock().await;
    vault.set(key_name, &api_key).await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct KeyStatus {
    pub provider_id: String,
    /// Masked hint of the stored key, e.g. "••••••••ab12", or null if unset.
    pub masked: Option<String>,
    pub backend: String,
}

/// Reports whether a key is permanently stored, without exposing it.
#[tauri::command]
pub async fn provider_key_status(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<KeyStatus, String> {
    let key_name = key_name_for(&provider_id)?;
    let vault = state.vault.lock().await;
    let stored = vault.get(key_name).await.map_err(|e| e.to_string())?;
    let masked = stored.map(|k| {
        let chars: Vec<char> = k.chars().collect();
        let take = chars.len().min(4);
        let suffix: String = chars[chars.len() - take..].iter().collect();
        format!("••••••••{}", suffix)
    });
    Ok(KeyStatus {
        provider_id,
        masked,
        backend: format!("{:?}", state.vault_status),
    })
}

#[tauri::command]
pub async fn provider_delete_key(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<(), String> {
    let key_name = key_name_for(&provider_id)?;
    let vault = state.vault.lock().await;
    vault.delete(key_name).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn provider_get_key(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Option<String>, String> {
    let key_name = key_name_for(&provider_id)?;
    let vault = state.vault.lock().await;
    vault.get(key_name).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn persona_set_intensity(
    state: State<'_, AppState>,
    level: String,
) -> Result<(), String> {
    let intensity = match level.as_str() {
        "off" => PersonaIntensity::Off,
        "subtle" => PersonaIntensity::Subtle,
        "full" => PersonaIntensity::Full,
        _ => return Err(format!("invalid intensity: {}", level)),
    };
    let mut persona = state.persona.lock().await;
    persona.intensity = intensity;
    Ok(())
}

#[tauri::command]
pub async fn persona_get_intensity(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let persona = state.persona.lock().await;
    Ok(persona.intensity.to_string())
}

#[tauri::command]
pub async fn persona_get_system_prompt(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let persona = state.persona.lock().await;
    Ok(persona.generate_system_prompt())
}

#[tauri::command]
pub async fn settings_get_vault_status(
    state: State<'_, AppState>,
) -> Result<String, String> {
    Ok(format!("{:?}", state.vault_status))
}

#[derive(Serialize)]
pub struct ChatResponse {
    pub content: String,
    pub reasoning: Option<String>,
    pub tokens_used: u32,
}

#[tauri::command]
pub async fn chat_send_message(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    model: Option<String>,
) -> Result<ChatResponse, String> {
    let uuid = uuid::Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let model_id = model.unwrap_or_else(|| "anthropic/claude-sonnet-4".to_string());

    let deps = crate::agent::AgentDeps {
        vault: state.vault.clone(),
        persona: state.persona.clone(),
        mcp: state.mcp.clone(),
    };

    let result = crate::agent::run_agent_turn(&deps, &state.sessions, uuid, &message, &model_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ChatResponse {
        content: result.content,
        reasoning: None,
        tokens_used: result.total_tokens as u32,
    })
}

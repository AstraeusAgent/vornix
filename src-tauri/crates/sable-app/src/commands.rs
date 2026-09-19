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

#[tauri::command]
pub async fn provider_set_key(
    state: State<'_, AppState>,
    provider_id: String,
    api_key: String,
) -> Result<(), String> {
    let key_name = match provider_id.as_str() {
        "openrouter" => "openrouter_api_key",
        "opencode-go" => "opencode_go_api_key",
        "github" => "github_token",
        _ => return Err(format!("unknown provider: {}", provider_id)),
    };
    let mut vault = state.vault.lock().await;
    vault.set(key_name, &api_key).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn provider_get_key(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Option<String>, String> {
    let key_name = match provider_id.as_str() {
        "openrouter" => "openrouter_api_key",
        "opencode-go" => "opencode_go_api_key",
        "github" => "github_token",
        _ => return Err(format!("unknown provider: {}", provider_id)),
    };
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
    persona.set_intensity(intensity);
    Ok(())
}

#[tauri::command]
pub async fn persona_get_intensity(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let persona = state.persona.lock().await;
    Ok(persona.intensity().to_string())
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
) -> Result<ChatResponse, String> {
    let uuid = uuid::Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;

    // Store user message
    state
        .sessions
        .append_message(uuid, "user", &message, None, None, None, None, None)
        .await
        .map_err(|e| e.to_string())?;

    // Get API key for OpenRouter
    let vault = state.vault.lock().await;
    let api_key = vault
        .get("openrouter_api_key")
        .await
        .map_err(|e| e.to_string())?
        .ok_or("No API key configured. Add an OpenRouter key in Settings.")?;
    drop(vault);

    // Get conversation history
    let messages = state
        .sessions
        .get_all_messages(uuid)
        .await
        .map_err(|e| e.to_string())?;

    // Get persona system prompt
    let persona = state.persona.lock().await;
    let system_prompt = persona.generate_system_prompt();
    drop(persona);

    // Get available tools from MCP
    let mcp = state.mcp.lock().await;
    let tools_json: Vec<serde_json::Value> = mcp
        .get_tools_with_schemas()
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect();
    drop(mcp);

    // Build request to OpenRouter
    let mut api_messages: Vec<serde_json::Value> = vec![
        serde_json::json!({"role": "system", "content": system_prompt}),
    ];
    for m in &messages {
        api_messages.push(serde_json::json!({
            "role": m.role,
            "content": m.content,
        }));
    }

    let mut body = serde_json::json!({
        "model": "anthropic/claude-sonnet-4",
        "messages": api_messages,
        "stream": false,
    });

    if !tools_json.is_empty() {
        body["tools"] = serde_json::json!(tools_json);
    }

    let client = reqwest::Client::new();
    let resp = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let resp_json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let content = resp_json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let tokens_used = resp_json
        .get("usage")
        .and_then(|u| u.get("total_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;

    // Store assistant response
    state
        .sessions
        .append_message(
            uuid,
            "assistant",
            &content,
            None,
            None,
            None,
            Some(tokens_used as i64),
            Some("anthropic/claude-sonnet-4"),
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(ChatResponse {
        content,
        reasoning: None,
        tokens_used,
    })
}
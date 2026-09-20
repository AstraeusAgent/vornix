//! The real agent turn loop: model → tool calls → MCP execution → verify → repeat.
//!
//! Used by both the `chat_send_message` Tauri command and the e2e integration
//! test, so the UI and tests exercise the exact same code path.

use anyhow::{Context, Result};
use vornix_memory::SessionStore;
use vornix_mcp::manager::McpManager;
use vornix_persona::policy::PersonaPolicy;
use vornix_secrets::SecretsVault;
use vornix_tools::{PermissionDecision, PermissionPolicy, PermissionTier, ToolCall};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MAX_ITERATIONS: usize = 25;
const MAX_TOOL_OUTPUT_CHARS: usize = 20_000;

pub struct AgentTurnResult {
    pub content: String,
    pub tool_calls_executed: usize,
    pub iterations: usize,
    pub total_tokens: u64,
}

/// Map an MCP tool name (`vornix-fs.read_file`) to the permission
/// classifier's vocabulary (`filesystem.read_file`).
fn permission_key(mcp_name: &str) -> String {
    let (server, tool) = match mcp_name.split_once('.') {
        Some(pair) => pair,
        None => return mcp_name.to_string(),
    };
    let category = match server {
        "vornix-fs" => "filesystem",
        "vornix-shell" => "shell",
        "vornix-thinking" => "thinking",
        "vornix-git" => "git",
        other => other,
    };
    format!("{}.{}", category, tool)
}

/// API-facing tool name: OpenRouter function names must match
/// `^[a-zA-Z0-9_-]{1,64}$`, so `vornix-fs.read_file` becomes `vornix_fs__read_file`.
fn api_tool_name(mcp_name: &str) -> String {
    mcp_name.replace('.', "__").replace('-', "_")
}

fn mcp_tool_name(api_name: &str) -> String {
    // `vornix_fs__read_file` -> server "vornix_fs" -> canonical "vornix-fs.read_file"
    if let Some((server, tool)) = api_name.split_once("__") {
        let server = server.replace("_fs", "-fs").replace("_shell", "-shell")
            .replace("_thinking", "-thinking").replace("_git", "-git");
        format!("{}.{}", server, tool)
    } else {
        api_name.to_string()
    }
}

pub struct AgentDeps {
    pub vault: Arc<Mutex<SecretsVault>>,
    pub persona: Arc<Mutex<PersonaPolicy>>,
    pub mcp: Arc<Mutex<McpManager>>,
}

pub async fn run_agent_turn(
    deps: &AgentDeps,
    store: &SessionStore,
    session_id: Uuid,
    user_message: &str,
    model: &str,
) -> Result<AgentTurnResult> {
    // 1. Persist the user message
    store
        .append_message(session_id, "user", user_message, None, None, None, None, None)
        .await?;

    // 2. Read the key from the vault at call time (never cached in JS-land)
    let api_key = {
        let vault = deps.vault.lock().await;
        vault
            .get("openrouter_api_key")
            .await?
            .ok_or_else(|| anyhow::anyhow!("No OpenRouter API key configured. Set one in Settings."))?
    };

    // 3. System prompt: persona + workspace + tool policy
    let system_prompt = {
        let persona = deps.persona.lock().await;
        let base = persona.generate_system_prompt();
        let verification = persona.generate_verification_instruction();
        let workspace = workspace_root();
        format!(
            "{}\n\n{}\n\n## Environment\nYou are operating in the workspace directory: {}\n\
             File paths in tool calls are relative to this directory.\n\
             When asked to build something, actually build it: create the files with the \
             write_file tool, then verify by reading them back. Do not just describe code — \
             write it to disk.",
            base, verification, workspace.display()
        )
    };

    // 4. Conversation history from the session store
    let history = store.get_all_messages(session_id).await?;
    let mut api_messages: Vec<serde_json::Value> = vec![
        serde_json::json!({"role": "system", "content": system_prompt}),
    ];
    for m in &history {
        match m.role.as_str() {
            "user" => api_messages.push(serde_json::json!({"role": "user", "content": m.content})),
            "assistant" => {
                let mut msg = serde_json::json!({"role": "assistant", "content": m.content});
                if let Some(ref tc) = m.tool_calls_json {
                    if let Ok(calls) = serde_json::from_str::<serde_json::Value>(tc) {
                        if !calls.is_null() {
                            msg["tool_calls"] = calls;
                        }
                    }
                }
                api_messages.push(msg);
            }
            "tool" => {
                api_messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": m.tool_call_id.clone().unwrap_or_default(),
                    "content": m.content,
                }));
            }
            _ => {}
        }
    }

    // 5. Tools from the MCP registry (real discovered tools)
    let tool_defs: Vec<serde_json::Value> = {
        let mcp = deps.mcp.lock().await;
        mcp.get_tools_with_schemas()
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": api_tool_name(&t.name),
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect()
    };

    if tool_defs.is_empty() {
        warn!("no MCP tools connected — agent will run without tool access");
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;

    let mut tool_calls_executed = 0usize;
    let mut iterations = 0usize;
    let mut total_tokens = 0u64;
    let mut final_content = String::new();

    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            final_content = format!(
                "{}\n\n[Stopped: hit the {}-iteration limit. Work so far is saved.]",
                final_content, MAX_ITERATIONS
            );
            break;
        }

        let mut body = serde_json::json!({
            "model": model,
            "messages": api_messages,
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if !tool_defs.is_empty() {
            body["tools"] = serde_json::json!(tool_defs);
            body["tool_choice"] = serde_json::json!("auto");
        }

        let resp = client
            .post(OPENROUTER_URL)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .context("OpenRouter request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenRouter error ({}): {}", status, text);
        }

        let (content, tool_calls, usage_tokens) =
            stream_completion(resp).await.context("stream failed")?;
        total_tokens += usage_tokens;

        if tool_calls.is_empty() {
            // Task complete — persist and stop
            store
                .append_message(
                    session_id,
                    "assistant",
                    &content,
                    None,
                    None,
                    None,
                    Some(total_tokens as i64),
                    Some(model),
                )
                .await?;
            final_content = content;
            break;
        }

        // Persist the assistant turn WITH its tool calls so history replays correctly
        let tool_calls_json = serde_json::to_string(&tool_calls).unwrap_or_default();
        store
            .append_message(
                session_id,
                "assistant",
                &content,
                None,
                Some(tool_calls_json.as_str()),
                None,
                None,
                Some(model),
            )
            .await?;

        api_messages.push(serde_json::json!({
            "role": "assistant",
            "content": content,
            "tool_calls": tool_calls,
        }));

        // Execute each tool call through the permission gate + real MCP
        for tc in &tool_calls {
            let call_id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
            let func = tc.get("function");
            let raw_name = func
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let args_str = func
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let args: serde_json::Value = serde_json::from_str(args_str)
                .unwrap_or(serde_json::json!({}));

            let mcp_name = mcp_tool_name(raw_name);
            let (result_content, success) =
                execute_tool(deps, session_id, &mcp_name, args).await;
            tool_calls_executed += 1;

            // Persist the tool result so future turns replay valid history
            store
                .append_message(
                    session_id,
                    "tool",
                    &result_content,
                    None,
                    None,
                    Some(call_id.as_str()),
                    None,
                    Some(model),
                )
                .await?;

            let _ = store.log_tool_call(
                session_id,
                Uuid::nil(),
                &mcp_name,
                args_str,
                Some(&result_content),
                Some(success),
                None,
                None,
            );

            api_messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": result_content,
            }));
        }
    }

    info!(
        session = %session_id,
        iterations,
        tool_calls_executed,
        tokens = total_tokens,
        "agent turn complete"
    );

    Ok(AgentTurnResult {
        content: final_content,
        tool_calls_executed,
        iterations,
        total_tokens,
    })
}

async fn execute_tool(
    deps: &AgentDeps,
    session_id: Uuid,
    mcp_name: &str,
    args: serde_json::Value,
) -> (String, bool) {
    // Permission gate: classify, auto-approve Tier 0/1, deny Tier 2.
    // (The UI approval flow for Tier 2 is not yet built — fail closed.)
    let call = ToolCall::new(permission_key(mcp_name), args.clone());
    let tier = PermissionPolicy::classify(&call);
    let decision = PermissionPolicy::evaluate(
        &vornix_tools::PermissionRequest {
            tool_call: call,
            tier,
            description: String::new(),
            risk_notes: Vec::new(),
        },
        &HashSet::new(),
    );

    if matches!(
        tier,
        PermissionTier::Tier2Destructive
    ) || matches!(decision, PermissionDecision::Denied { .. })
    {
        let reason = match decision {
            PermissionDecision::Denied { reason } => reason,
            _ => "destructive action requires interactive approval, which is not yet available".into(),
        };
        return (
            format!("DENIED by permission gate (tier: {:?}): {}", tier, reason),
            false,
        );
    }

    let mut mcp = deps.mcp.lock().await;
    match mcp.call_tool(mcp_name, args).await {
        Ok(result) => {
            let mut out = result.output;
            if out.chars().count() > MAX_TOOL_OUTPUT_CHARS {
                out = out.chars().take(MAX_TOOL_OUTPUT_CHARS).collect();
                out.push_str("\n...[truncated]");
            }
            if result.success {
                (out, true)
            } else {
                (format!("ERROR: {}", result.error.unwrap_or(out)), false)
            }
        }
        Err(e) => (format!("ERROR: {}", e), false),
    }
}

pub fn workspace_root() -> std::path::PathBuf {
    std::env::var("VORNIX_WORKSPACE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("VornixWorkspace")
        })
}

/// Consume an OpenAI-style SSE stream, accumulating content and streamed
/// tool-call fragments, returning the assembled (content, tool_calls, usage).
async fn stream_completion(
    resp: reqwest::Response,
) -> Result<(String, Vec<serde_json::Value>, u64)> {
    use futures::StreamExt;

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut content = String::new();
    let mut usage_tokens = 0u64;

    // tool_calls arrive as fragments keyed by index:
    // {index, id?, function: {name?, arguments?}}
    let mut tc_builders: std::collections::HashMap<u64, (String, String, String)> =
        std::collections::HashMap::new(); // index -> (id, name, arguments)

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("stream read error")?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        // Process complete SSE lines
        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            let line = line.trim();

            let data = if let Some(rest) = line.strip_prefix("data:") {
                rest.trim()
            } else {
                continue;
            };

            if data == "[DONE]" {
                return Ok((content, finish_tool_calls(tc_builders), usage_tokens));
            }

            let parsed: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue, // keepalive comments, partial JSON, etc.
            };

            if let Some(err) = parsed.get("error") {
                anyhow::bail!("stream error: {}", err);
            }

            if let Some(u) = parsed.get("usage") {
                usage_tokens = u
                    .get("total_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(usage_tokens);
            }

            let choice = match parsed.pointer("/choices/0") {
                Some(c) => c,
                None => continue,
            };

            if let Some(delta) = choice.get("delta") {
                if let Some(c) = delta.get("content").and_then(|c| c.as_str()) {
                    content.push_str(c);
                }
                if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tcs {
                        let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                        let entry = tc_builders.entry(idx).or_default();
                        if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                            entry.0.push_str(id);
                        }
                        if let Some(name) =
                            tc.pointer("/function/name").and_then(|n| n.as_str())
                        {
                            entry.1.push_str(name);
                        }
                        if let Some(args) =
                            tc.pointer("/function/arguments").and_then(|a| a.as_str())
                        {
                            entry.2.push_str(args);
                        }
                    }
                }
            }
        }
    }

    Ok((content, finish_tool_calls(tc_builders), usage_tokens))
}

fn finish_tool_calls(
    builders: std::collections::HashMap<u64, (String, String, String)>,
) -> Vec<serde_json::Value> {
    let mut indices: Vec<u64> = builders.keys().copied().collect();
    indices.sort();
    indices
        .into_iter()
        .filter_map(|idx| {
            let (id, name, args) = builders.get(&idx)?;
            if name.is_empty() {
                return None;
            }
            Some(serde_json::json!({
                "id": if id.is_empty() { format!("call_{}", uuid::Uuid::new_v4().simple()) } else { id.clone() },
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": if args.is_empty() { "{}".to_string() } else { args.clone() },
                }
            }))
        })
        .collect()
}
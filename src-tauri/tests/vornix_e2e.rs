//! End-to-end agent test: real OpenRouter call (free model), real MCP child
//! process (vornix-fs over stdio JSON-RPC), real SQLite persistence, and files
//! actually written to disk. Proves the §0 thesis: the loop plans, acts
//! through tools, and produces a working artifact.
//!
//! Requires an OpenRouter key in the OS keychain (key name: `openrouter_api_key`).

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use vornix::agent::{run_agent_turn, AgentDeps};
use vornix_mcp::manager::McpManager;
use vornix_mcp::transport::{McpServerConfig, TransportConfig};
use vornix_memory::SessionStore;
use vornix_persona::policy::PersonaPolicy;
use vornix_secrets::SecretsVault;

const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// Pick a free model that supports tool calling, from the live endpoint.
async fn pick_free_model(client: &reqwest::Client) -> Result<String, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    struct ModelsResp {
        data: Vec<ModelEntry>,
    }
    #[derive(serde::Deserialize)]
    struct ModelEntry {
        id: String,
        pricing: std::collections::HashMap<String, serde_json::Value>,
        #[serde(default)]
        supported_parameters: Vec<String>,
        #[serde(default)]
        context_length: Option<u64>,
    }

    let resp: ModelsResp = client.get(OPENROUTER_MODELS_URL).send().await?.json().await?;

    let mut candidates: Vec<&ModelEntry> = resp
        .data
        .iter()
        .filter(|m| {
            let prompt_free = m
                .pricing
                .get("prompt")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .map(|p| p == 0.0)
                .unwrap_or(false);
            let completion_free = m
                .pricing
                .get("completion")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .map(|p| p == 0.0)
                .unwrap_or(false);
            prompt_free
                && completion_free
                && m.supported_parameters.iter().any(|p| p == "tools")
        })
        .collect();

    candidates.sort_by_key(|m| std::cmp::Reverse(m.context_length.unwrap_or(0)));

    candidates
        .first()
        .map(|m| m.id.clone())
        .ok_or_else(|| "no free tool-capable model available on OpenRouter".into())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_builds_web_os_end_to_end() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .try_init();

    let client = reqwest::Client::new();
    let model = pick_free_model(&client)
        .await
        .expect("failed to select a free model from OpenRouter");
    println!("E2E model: {}", model);

    // --- Temp workspace + DB ---
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let db_path = tmp.path().join("test.db");

    // --- Session store ---
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
        .await
        .unwrap();
    let store = SessionStore::new(pool);
    store.run_migrations().await.unwrap();
    let session = store
        .create_session(Some("web os e2e"), None, None, None)
        .await
        .unwrap();

    // --- Vault (real keychain; key must be pre-configured) ---
    let (vault, _status) = SecretsVault::initialize().await.unwrap();
    let has_key = vault
        .get("openrouter_api_key")
        .await
        .unwrap()
        .is_some();
    assert!(has_key, "openrouter_api_key not found in keychain — set one first");

    // --- Real MCP server: vornix-fs as a child process over stdio ---
    let servers_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("mcp-servers"))
        .unwrap();
    let script = servers_dir.join("vornix-fs").join("index.cjs");
    assert!(script.exists(), "vornix-fs script missing at {}", script.display());

    let mut mcp = McpManager::new();
    let mut env = HashMap::new();
    env.insert(
        "VORNIX_WORKSPACE".to_string(),
        workspace.display().to_string(),
    );
    mcp.add_server(McpServerConfig {
        name: "vornix-fs".to_string(),
        transport: TransportConfig::Stdio {
            command: "node".to_string(),
            args: vec![script.display().to_string()],
            env,
        },
        enabled: true,
        auto_restart: false,
    })
    .await
    .unwrap();
    mcp.connect_server("vornix-fs")
        .await
        .expect("vornix-fs MCP handshake failed");

    // --- Persona (behavioral rules on) ---
    let persona = PersonaPolicy::default_vornix();

    let deps = AgentDeps {
        vault: Arc::new(Mutex::new(vault)),
        persona: Arc::new(Mutex::new(persona)),
        mcp: Arc::new(Mutex::new(mcp)),
    };

    let task = "Build a simple web OS in this workspace consisting of three files: \
        index.html, style.css, and app.js. Requirements: a desktop with three icons \
        (Files, Terminal, About), a taskbar with a clock, draggable and clovornix windows, \
        and a start menu. No external dependencies or frameworks — vanilla JS only. \
        Write each file with the write_file tool, then read each file back to verify it \
        was written correctly. Finish with a short summary of what you built.";

    let result = run_agent_turn(&deps, &store, session.id, task, &model)
        .await
        .expect("agent turn failed");

    println!(
        "E2E result: iterations={}, tool_calls={}, tokens={}",
        result.iterations, result.tool_calls_executed, result.total_tokens
    );
    println!("E2E final summary:\n{}", result.content);

    // --- Assertions: files really exist on disk with real content ---
    for f in ["index.html", "style.css", "app.js"] {
        let path = workspace.join(f);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} not written to workspace: {}", f, e));
        assert!(content.len() > 200, "{} suspiciously small ({} bytes)", f, content.len());
        println!("{}: {} bytes OK", f, content.len());
    }

    let html = std::fs::read_to_string(workspace.join("index.html")).unwrap();
    assert!(html.contains("<!DOCTYPE") || html.contains("<html"), "index.html is not HTML");

    // --- Session history is complete and replayable ---
    let messages = store.get_all_messages(session.id).await.unwrap();
    assert!(messages.iter().any(|m| m.role == "user"));
    assert!(messages.iter().any(|m| m.role == "assistant"));
    assert!(
        messages.iter().any(|m| m.role == "tool"),
        "tool results not persisted — history would not replay"
    );
    assert!(result.tool_calls_executed > 0, "no tools were called — this was not an agent run");
}

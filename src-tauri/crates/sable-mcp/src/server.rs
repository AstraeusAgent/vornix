use crate::transport::*;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ServerStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
    Crashed,
}

impl std::fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerStatus::Disconnected => write!(f, "disconnected"),
            ServerStatus::Connecting => write!(f, "connecting"),
            ServerStatus::Connected => write!(f, "connected"),
            ServerStatus::Error(msg) => write!(f, "error: {}", msg),
            ServerStatus::Crashed => write!(f, "crashed"),
        }
    }
}

pub struct McpServer {
    pub config: McpServerConfig,
    pub status: ServerStatus,
    pub tools: Vec<McpDiscoveredTool>,
    pub last_connected: Option<DateTime<Utc>>,
    pub restart_count: u32,
    runner: Option<StdioRunner>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpDiscoveredTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub full_name: String,
}

impl McpServer {
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            status: ServerStatus::Disconnected,
            tools: Vec::new(),
            last_connected: None,
            restart_count: 0,
            runner: None,
        }
    }

    pub async fn connect(&mut self) -> Result<()> {
        match &self.config.transport {
            TransportConfig::Stdio { command, args, env } => {
                self.status = ServerStatus::Connecting;
                let runner = StdioRunner::spawn(command, args, env)
                    .await
                    .context("failed to spawn MCP server process")?;
                self.runner = Some(runner);
            }
            TransportConfig::StreamableHttp { .. } => {
                self.status = ServerStatus::Connecting;
                // HTTP transport - verify connectivity by sending initialize
            }
        }

        // Send initialize request
        let init_params = serde_json::json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {
                "name": "sable",
                "version": "0.1.0"
            }
        });

        let _response = self
            .send_request("initialize", Some(init_params))
            .await
            .context("MCP initialize failed")?;

        // Send initialized notification
        self.send_notification("notifications/initialized", None)
            .await?;

        self.status = ServerStatus::Connected;
        self.last_connected = Some(Utc::now());

        // Discover tools
        self.refresh_tools().await?;

        info!(
            "MCP server '{}' connected with {} tools",
            self.config.name,
            self.tools.len()
        );

        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(mut runner) = self.runner.take() {
            runner.shutdown().await;
        }
        self.status = ServerStatus::Disconnected;
        self.tools.clear();
        Ok(())
    }

    pub async fn refresh_tools(&mut self) -> Result<()> {
        let response = self
            .send_request("tools/list", Some(serde_json::json!({})))
            .await
            .context("tools/list failed")?;

        let tools_array = response
            .get("tools")
            .and_then(|t| t.as_array())
            .context("tools/list returned no tools array")?;

        self.tools = tools_array
            .iter()
            .filter_map(|t| {
                let name = t.get("name")?.as_str()?.to_string();
                let description = t
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                let input_schema = t
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                let full_name = format!("{}.{}", self.config.name, name);
                Some(McpDiscoveredTool {
                    name,
                    description,
                    input_schema,
                    full_name,
                })
            })
            .collect();

        Ok(())
    }

    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpToolCallResult> {
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": arguments,
        });

        let response = self
            .send_request("tools/call", Some(params))
            .await
            .context("tools/call failed")?;

        let is_error = response
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let content = response
            .get("content")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let item_type = item.get("type")?.as_str()?;
                        match item_type {
                            "text" => {
                                let text = item.get("text")?.as_str()?.to_string();
                                Some(McpContent::Text { text })
                            }
                            "image" => {
                                let data = item.get("data")?.as_str()?.to_string();
                                let mime = item
                                    .get("mimeType")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("application/octet-stream")
                                    .to_string();
                                Some(McpContent::Image {
                                    data,
                                    mime_type: mime,
                                })
                            }
                            _ => None,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(McpToolCallResult { content, is_error })
    }

    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match &self.config.transport {
            TransportConfig::Stdio { .. } => {
                let runner = self
                    .runner
                    .as_mut()
                    .context("stdio runner not initialized")?;
                runner.send_request(method, params).await
            }
            TransportConfig::StreamableHttp {
                url,
                headers: _headers,
            } => {
                // JSON-RPC over HTTP POST
                let id = 1u64;
                let req = JsonRpcRequest {
                    jsonrpc: "2.0".to_string(),
                    id,
                    method: method.to_string(),
                    params,
                };
                let client = reqwest::Client::new();
                let resp = client
                    .post(url)
                    .header("Content-Type", "application/json")
                    .json(&req)
                    .send()
                    .await
                    .context("HTTP request to MCP server failed")?;

                let rpc_resp: JsonRpcResponse = resp
                    .json()
                    .await
                    .context("failed to parse JSON-RPC response")?;

                if let Some(err) = rpc_resp.error {
                    anyhow::bail!("MCP server error: {}", err);
                }

                rpc_resp.result.context("JSON-RPC response missing result")
            }
        }
    }

    async fn send_notification(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<()> {
        match &self.config.transport {
            TransportConfig::Stdio { .. } => {
                let runner = self
                    .runner
                    .as_mut()
                    .context("stdio runner not initialized")?;
                let notif = JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method: method.to_string(),
                    params,
                };
                runner.send_notification(&notif).await
            }
            TransportConfig::StreamableHttp { url, .. } => {
                let notif = JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method: method.to_string(),
                    params,
                };
                let client = reqwest::Client::new();
                client
                    .post(url)
                    .header("Content-Type", "application/json")
                    .json(&notif)
                    .send()
                    .await
                    .context("HTTP notification failed")?;
                Ok(())
            }
        }
    }
}

/// Manages a stdio MCP server process with JSON-RPC 2.0 over stdin/stdout
struct StdioRunner {
    child: Child,
    stdin_tx: mpsc::Sender<Vec<u8>>,
    response_rx: mpsc::Receiver<JsonRpcResponse>,
    stderr_rx: mpsc::Receiver<String>,
    next_id: u64,
    pending: HashMap<u64, oneshot::Sender<serde_json::Value>>,
}

impl StdioRunner {
    async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().context("failed to spawn MCP server process")?;

        let stdin = child.stdin.take().context("no stdin pipe")?;
        let stdout = child.stdout.take().context("no stdout pipe")?;
        let stderr = child.stderr.take().context("no stderr pipe")?;

        // stdin writer task
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(64);
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(data) = stdin_rx.recv().await {
                if stdin.write_all(&data).await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        // stdout reader task — splits into notifications (no id) and responses (has id)
        let (response_tx, response_rx) = mpsc::channel::<JsonRpcResponse>(64);
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<JsonRpcResponse>(trimmed) {
                    Ok(resp) => {
                        let _ = response_tx.send(resp).await;
                    }
                    Err(e) => {
                        debug!("non-JSON-RPC stdout line from MCP server: {} ({})", trimmed, e);
                    }
                }
            }
        });

        // stderr reader task
        let (stderr_tx, stderr_rx) = mpsc::channel::<String>(64);
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = stderr_tx.send(line).await;
            }
        });

        Ok(Self {
            child,
            stdin_tx,
            response_rx,
            stderr_rx,
            next_id: 1,
            pending: HashMap::new(),
        })
    }

    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;

        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let mut msg = serde_json::to_vec(&req)?;
        msg.push(b'\n');

        self.stdin_tx
            .send(msg)
            .await
            .context("failed to send to MCP server stdin")?;

        // Wait for response with matching id, with timeout
        let deadline = tokio::time::Duration::from_secs(30);
        let start = tokio::time::Instant::now();

        loop {
            if start.elapsed() > deadline {
                anyhow::bail!("timeout waiting for MCP server response to '{}' (id={})", method, id);
            }

            // Drain stderr in parallel
            while let Ok(line) = self.stderr_rx.try_recv() {
                debug!("[MCP stderr] {}", line);
            }

            match tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                self.response_rx.recv(),
            )
            .await
            {
                Ok(Some(resp)) => {
                    let resp_id = resp.id.unwrap_or(0);
                    if resp_id == id {
                        if let Some(err) = resp.error {
                            anyhow::bail!("MCP server error for '{}': {}", method, err);
                        }
                        return resp.result.context("no result in JSON-RPC response");
                    } else {
                        // Response for a different request — shouldn't happen in sequential calls
                        warn!("unexpected response id {} (expected {})", resp_id, id);
                    }
                }
                Ok(None) => {
                    anyhow::bail!("MCP server stdout closed unexpectedly");
                }
                Err(_) => {
                    // Timeout on this iteration, loop to check overall deadline
                    continue;
                }
            }
        }
    }

    async fn send_notification(&mut self, notif: &JsonRpcNotification) -> Result<()> {
        let mut msg = serde_json::to_vec(notif)?;
        msg.push(b'\n');
        self.stdin_tx
            .send(msg)
            .await
            .context("failed to send notification to MCP server")?;
        Ok(())
    }

    async fn shutdown(&mut self) {
        // Try graceful shutdown
        let _ = self.child.kill().await;
    }
}
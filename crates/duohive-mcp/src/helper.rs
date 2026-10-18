//! The `duohive mcp` helper: each agent's MCP server, a thin relay to DuoHive
//! over `run.sock` (TD §6.1). It parses MCP, never the tool arguments (P3-D17).

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use duohive_core::tool::{Rights, ToolName, WAIT_MAX_MINUTES};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::rpc::{self, Incoming};
use crate::tools::specs;

/// What the helper can do while DuoHive is closed (P3-D10).
pub trait ClosedFallback: Send + Sync {
    /// The rights of the session with this token, from its row (P3-D30); `None` when unknown.
    fn rights(&self, token: &str) -> Option<Rights>;
    /// Queues a changing call for DuoHive's next start; returns its request id.
    fn queue(&self, token: &str, tool: &str, arguments: &Value) -> Result<u64, String>;
}

/// The helper's settings.
#[derive(Clone, Debug)]
pub struct HelperConfig {
    /// `run.sock`.
    pub sock: PathBuf,
    /// `DUOHIVE_TOKEN`, if the launch set it.
    pub token: Option<String>,
    /// How long connecting may take before DuoHive counts as closed (200 ms).
    pub connect_timeout: Duration,
    /// How long to wait for an answer (30 s); see [`answer_timeout`].
    pub answer_timeout: Duration,
}

/// How long the helper waits for DuoHive's answer to `tool`: `wait_for_agent`
/// gets `WAIT_MAX_MINUTES` plus 30 s, whatever its own timeout (P3-D17).
pub fn answer_timeout(cfg: &HelperConfig, tool: &str) -> Duration {
    if tool == ToolName::WaitForAgent.as_str() {
        Duration::from_secs(u64::from(WAIT_MAX_MINUTES) * 60 + 30)
    } else {
        cfg.answer_timeout
    }
}

/// Serves MCP on `stdin`/`stdout` until `stdin` ends.
pub async fn run_helper(
    cfg: HelperConfig,
    stdin: impl AsyncRead + Unpin,
    mut stdout: impl AsyncWrite + Unpin,
    closed: Arc<dyn ClosedFallback>,
) -> io::Result<()> {
    let mut lines = BufReader::new(stdin).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let answer = match rpc::parse(&line) {
            Incoming::Notification => continue,
            Incoming::Initialize { id, protocol } => {
                rpc::result(&id, &rpc::initialize_result(protocol.as_deref()))
            }
            Incoming::Ping { id } => rpc::result(&id, &json!({})),
            Incoming::ListTools { id } => rpc::result(
                &id,
                &json!({ "tools": list_tools(&cfg, closed.as_ref()).await }),
            ),
            Incoming::CallTool {
                id,
                name,
                arguments,
            } => {
                let (json, is_error) = call_tool(&cfg, closed.as_ref(), &name, &arguments).await;
                rpc::result(&id, &rpc::tool_result(&json, is_error))
            }
            Incoming::Unknown { id, method } => {
                rpc::error(&id, rpc::METHOD_NOT_FOUND, &format!("no method {method}"))
            }
            Incoming::Invalid { id, code, message } => rpc::error(&id, code, &message),
        };
        stdout.write_all(answer.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }
    Ok(())
}

/// A tool error as the P3-D16 JSON.
fn tool_error(code: &str, message: &str) -> (Value, bool) {
    (
        json!({ "error": { "code": code, "message": message } }),
        true,
    )
}

/// What one request to DuoHive came to.
enum Asked {
    /// DuoHive answered with this line's JSON.
    Answered(Value),
    /// Connecting failed: DuoHive is closed.
    Closed,
    /// Connected, but no answer came (P3-D24).
    CutOff,
}

/// Sends one line to DuoHive and reads its one-line answer.
async fn ask(cfg: &HelperConfig, request: &Value, timeout: Duration) -> Asked {
    let connected = tokio::time::timeout(cfg.connect_timeout, UnixStream::connect(&cfg.sock)).await;
    let Ok(Ok(mut stream)) = connected else {
        return Asked::Closed;
    };
    let mut line = request.to_string();
    line.push('\n');
    if stream.write_all(line.as_bytes()).await.is_err() {
        return Asked::CutOff;
    }
    let mut reader = BufReader::new(stream);
    let mut answer = String::new();
    match tokio::time::timeout(timeout, reader.read_line(&mut answer)).await {
        Ok(Ok(n)) if n > 0 => serde_json::from_str(&answer).map_or(Asked::CutOff, Asked::Answered),
        _ => Asked::CutOff,
    }
}

async fn list_tools(cfg: &HelperConfig, closed: &dyn ClosedFallback) -> Vec<Value> {
    let Some(token) = &cfg.token else {
        return Vec::new();
    };
    let request = json!({ "kind": "tools", "token": token });
    match ask(cfg, &request, cfg.answer_timeout).await {
        Asked::Answered(v) => v["ok"]["tools"].as_array().cloned().unwrap_or_default(),
        Asked::CutOff => Vec::new(),
        Asked::Closed => closed.rights(token).map_or_else(Vec::new, |rights| {
            specs(rights)
                .into_iter()
                .map(|s| {
                    json!({ "name": s.name, "description": s.description, "inputSchema": s.input_schema })
                })
                .collect()
        }),
    }
}

async fn call_tool(
    cfg: &HelperConfig,
    closed: &dyn ClosedFallback,
    name: &str,
    arguments: &Value,
) -> (Value, bool) {
    let Some(token) = &cfg.token else {
        return tool_error("unknown_token", "this agent has no DuoHive token");
    };
    let request = json!({ "kind": "call", "token": token, "tool": name, "arguments": arguments });
    match ask(cfg, &request, answer_timeout(cfg, name)).await {
        Asked::Answered(v) => match (v.get("ok"), v.get("error")) {
            (Some(ok), _) => (ok.clone(), false),
            (None, Some(error)) => (json!({ "error": error }), true),
            (None, None) => tool_error("interrupted", "DuoHive's answer was unreadable"),
        },
        Asked::CutOff => tool_error("interrupted", "DuoHive closed during the call"),
        Asked::Closed => closed_call(closed, token, name, arguments),
    }
}

/// A call while DuoHive is closed: changes wait in the database, reads are refused (P3-D10).
fn closed_call(
    closed: &dyn ClosedFallback,
    token: &str,
    name: &str,
    arguments: &Value,
) -> (Value, bool) {
    let Some(rights) = closed.rights(token) else {
        return tool_error("unknown_token", "no running agent has this token");
    };
    let Some(tool) = ToolName::parse(name) else {
        return tool_error("invalid_arguments", &format!("unknown tool {name}"));
    };
    if tool.duo_only() && rights != Rights::Duo {
        return tool_error("not_allowed", &format!("{name} is Duo's tool"));
    }
    if !tool.changes() {
        return tool_error("duohive_closed", "DuoHive is closed");
    }
    match closed.queue(token, name, arguments) {
        Ok(request) => (
            json!({ "status": "queued_until_open", "request": request }),
            false,
        ),
        Err(e) => tool_error(
            "duohive_closed",
            &format!("DuoHive is closed, and the call couldn't be queued: {e}"),
        ),
    }
}

#[cfg(test)]
mod tests;

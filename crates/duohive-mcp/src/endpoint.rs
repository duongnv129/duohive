//! DuoHive's side of a tool request: one connection, one line in, one line out.

use duohive_core::command::Command;
use duohive_core::engine::Input;
use duohive_core::launch::Token;
use duohive_core::tool::{ErrorCode, ToolCall, ToolError, ToolOk, ToolResult, parse_call};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use crate::calls::Calls;
use crate::protocol::{SockAnswer, SockLine};
use crate::tools::{render, specs};

/// Answers one `tools` or `call` line on `stream`, then closes it. A hook line
/// is not this function's; it is answered with an error.
pub async fn serve_request(
    line: SockLine,
    mut stream: UnixStream,
    to_core: mpsc::Sender<Input>,
    calls: Calls,
) {
    let answer = match line {
        SockLine::Hook(_) => SockAnswer::Error {
            code: "invalid_arguments".to_owned(),
            message: "a hook is not a tool request".to_owned(),
            limit: None,
        },
        SockLine::Tools { token } => {
            match ask_core(ToolCall::Rights, token, &mut stream, &to_core, &calls).await {
                None => return,
                Some(Ok(ToolOk::Rights(rights))) => {
                    let tools: Vec<_> = specs(rights)
                        .into_iter()
                        .map(|s| {
                            json!({
                                "name": s.name,
                                "description": s.description,
                                "inputSchema": s.input_schema,
                            })
                        })
                        .collect();
                    SockAnswer::Ok(json!({ "tools": tools }))
                }
                Some(other) => render(&other),
            }
        }
        SockLine::Call {
            token,
            tool,
            arguments,
        } => match parse_call(&tool, &arguments) {
            Err(e) => render(&Err(e)),
            Ok(call) => match ask_core(call, token, &mut stream, &to_core, &calls).await {
                None => return,
                Some(result) => render(&result),
            },
        },
    };
    let mut line = answer.to_line();
    line.push('\n');
    if let Err(e) = stream.write_all(line.as_bytes()).await {
        tracing::debug!(error = %e, "tool answer not written");
    }
    let _ = stream.shutdown().await;
}

/// Sends the call to the core and waits for its result; `None` when the
/// caller closed the connection first (the call is forgotten).
async fn ask_core(
    call: ToolCall,
    token: String,
    stream: &mut UnixStream,
    to_core: &mpsc::Sender<Input>,
    calls: &Calls,
) -> Option<ToolResult> {
    let (call_id, rx) = calls.open();
    let command = Command::Tool {
        call_id,
        token: Token(token),
        call,
    };
    if to_core.send(Input::Command(command)).await.is_err() {
        calls.forget(call_id);
        return Some(Err(ToolError::new(
            ErrorCode::Interrupted,
            "DuoHive is closing",
        )));
    }
    let mut probe = [0u8; 1];
    tokio::select! {
        result = rx => Some(result.unwrap_or_else(|_| {
            Err(ToolError::new(ErrorCode::Interrupted, "DuoHive closed during the call"))
        })),
        // The caller sends nothing more; a read returns only when it closes.
        _ = stream.read(&mut probe) => {
            calls.forget(call_id);
            None
        }
    }
}

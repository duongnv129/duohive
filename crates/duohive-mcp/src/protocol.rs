//! `run.sock` framing (P3-D17): one JSON object per line, told apart by `kind`.

use duohive_core::hook::HookMessage;
use serde_json::{Map, Value, json};

/// One parsed line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SockLine {
    /// A hook from `duohive hook` (no `kind`, or `"kind": "hook"`).
    Hook(HookMessage),
    /// `tools/list` for the caller's token.
    Tools {
        /// The caller's launch token.
        token: String,
    },
    /// One tool call.
    Call {
        /// The caller's launch token.
        token: String,
        /// The tool's name, unchecked.
        tool: String,
        /// The arguments, parsed by DuoHive (`tool::parse_call`).
        arguments: Value,
    },
}

/// Parses one line. The error never quotes the line, which may hold a token.
pub fn parse_line(line: &str) -> Result<SockLine, String> {
    let value: Value = serde_json::from_str(line).map_err(|e| format!("not JSON: {e}"))?;
    let Some(map) = value.as_object() else {
        return Err("not a JSON object".to_owned());
    };
    match map.get("kind").and_then(Value::as_str) {
        None | Some("hook") => serde_json::from_value(value.clone())
            .map(SockLine::Hook)
            .map_err(|e| format!("not a hook message: {e}")),
        Some("tools") => Ok(SockLine::Tools { token: token(map)? }),
        Some("call") => Ok(SockLine::Call {
            token: token(map)?,
            tool: map
                .get("tool")
                .and_then(Value::as_str)
                .ok_or("call without a tool")?
                .to_owned(),
            arguments: map.get("arguments").cloned().unwrap_or_else(|| json!({})),
        }),
        Some(_) => Err("unknown kind".to_owned()),
    }
}

fn token(map: &Map<String, Value>) -> Result<String, String> {
    map.get("token")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "missing token".to_owned())
}

/// The endpoint's one-line answer.
#[derive(Clone, Debug, PartialEq)]
pub enum SockAnswer {
    /// `{"ok": <json>}`.
    Ok(Value),
    /// `{"error": {"code", "message"}}`, plus `limit` for a limit error.
    Error {
        /// The P3-D16 code word.
        code: String,
        /// The message.
        message: String,
        /// `duplicate`, `rate` or `inbox_full`, for `code: "limit"`.
        limit: Option<String>,
    },
}

impl SockAnswer {
    /// The answer as one JSON line, without the newline.
    pub fn to_line(&self) -> String {
        match self {
            SockAnswer::Ok(v) => json!({ "ok": v }).to_string(),
            SockAnswer::Error {
                code,
                message,
                limit,
            } => {
                let mut error = json!({ "code": code, "message": message });
                if let (Some(limit), Some(obj)) = (limit, error.as_object_mut()) {
                    obj.insert("limit".to_owned(), json!(limit));
                }
                json!({ "error": error }).to_string()
            }
        }
    }
}

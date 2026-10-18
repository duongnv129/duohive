//! Newline-delimited JSON-RPC 2.0 over stdio: the five MCP methods the helper
//! serves (P3-D8's fallback for `rmcp`).

use serde_json::{Value, json};

/// The protocol version answered when the client asks for one this build doesn't know.
pub const LATEST_PROTOCOL: &str = "2025-06-18";
/// Protocol versions the helper speaks; a client's is echoed when it is one of these.
const KNOWN_PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// JSON-RPC: the method does not exist.
pub const METHOD_NOT_FOUND: i64 = -32_601;
/// JSON-RPC: the message is not a valid request.
pub const INVALID_REQUEST: i64 = -32_600;
/// JSON-RPC: the text is not JSON.
pub const PARSE_ERROR: i64 = -32_700;

/// One incoming message.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    /// `initialize`, answered with the client's protocol version.
    Initialize {
        /// The request id.
        id: Value,
        /// The version the client asked for.
        protocol: Option<String>,
    },
    /// `ping`.
    Ping {
        /// The request id.
        id: Value,
    },
    /// `tools/list`.
    ListTools {
        /// The request id.
        id: Value,
    },
    /// `tools/call`.
    CallTool {
        /// The request id.
        id: Value,
        /// The tool's name.
        name: String,
        /// Its arguments, untouched.
        arguments: Value,
    },
    /// A notification (no id), such as `notifications/initialized`: never answered.
    Notification,
    /// A request for a method the helper doesn't serve.
    Unknown {
        /// The request id.
        id: Value,
        /// The method.
        method: String,
    },
    /// A line that isn't a request.
    Invalid {
        /// The id, when one could be read.
        id: Value,
        /// The JSON-RPC error code.
        code: i64,
        /// Why.
        message: String,
    },
}

/// Parses one line from the client.
pub fn parse(line: &str) -> Incoming {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Incoming::Invalid {
                id: Value::Null,
                code: PARSE_ERROR,
                message: format!("not JSON: {e}"),
            };
        }
    };
    let Some(method) = v.get("method").and_then(Value::as_str) else {
        // A response or garbage: nothing the helper asked for.
        return match v.get("id") {
            Some(id) if !id.is_null() && v.get("result").is_none() && v.get("error").is_none() => {
                Incoming::Invalid {
                    id: id.clone(),
                    code: INVALID_REQUEST,
                    message: "no method".to_owned(),
                }
            }
            _ => Incoming::Notification,
        };
    };
    let Some(id) = v.get("id").filter(|i| !i.is_null()).cloned() else {
        return Incoming::Notification;
    };
    let params = v.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "initialize" => Incoming::Initialize {
            id,
            protocol: params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .map(str::to_owned),
        },
        "ping" => Incoming::Ping { id },
        "tools/list" => Incoming::ListTools { id },
        "tools/call" => Incoming::CallTool {
            id,
            name: params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            arguments: params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({})),
        },
        other => Incoming::Unknown {
            id,
            method: other.to_owned(),
        },
    }
}

/// The `initialize` result.
pub fn initialize_result(protocol: Option<&str>) -> Value {
    let version = protocol
        .filter(|p| KNOWN_PROTOCOLS.contains(p))
        .unwrap_or(LATEST_PROTOCOL);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "duohive", "version": env!("CARGO_PKG_VERSION") },
    })
}

/// A successful response line.
pub fn result(id: &Value, result: &Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

/// An error response line.
pub fn error(id: &Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

/// A `tools/call` result: the JSON as text and as structured content.
pub fn tool_result(json: &Value, is_error: bool) -> Value {
    let mut r = json!({
        "content": [{ "type": "text", "text": json.to_string() }],
        "structuredContent": json,
    });
    if is_error && let Some(obj) = r.as_object_mut() {
        obj.insert("isError".to_owned(), Value::Bool(true));
    }
    r
}

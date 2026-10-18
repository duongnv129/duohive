//! `duohive mcp`: the agent's MCP server on stdin/stdout (TD §6.1). It relays to
//! DuoHive over `run.sock`; while DuoHive is closed it reads rights from the
//! session row and queues changing calls in `state.db` (P3-D10, P3-D30).

use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use duohive_core::time::Millis;
use duohive_core::tool::Rights;
use duohive_mcp::helper::{ClosedFallback, HelperConfig, run_helper};
use duohive_store::Paths;
use duohive_store::closed::{queue_tool_call, rights_for_token};
use serde_json::Value;

/// How long connecting may take before DuoHive counts as closed.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
/// How long a call waits for DuoHive's answer (`wait_for_agent` waits longer).
const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

/// The closed-DuoHive fallback over `state.db`.
#[derive(Clone, Debug)]
pub struct StoreFallback {
    paths: Paths,
}

impl StoreFallback {
    /// A fallback over the data directory in `paths`.
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }
}

impl ClosedFallback for StoreFallback {
    fn rights(&self, token: &str) -> Option<Rights> {
        rights_for_token(&self.paths, token).ok().flatten()
    }

    fn queue(&self, token: &str, tool: &str, arguments: &Value) -> Result<u64, String> {
        queue_tool_call(&self.paths, token, tool, arguments, now()).map_err(|e| e.to_string())
    }
}

fn now() -> Millis {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    Millis(u64::try_from(ms).unwrap_or(u64::MAX))
}

/// Runs the helper until stdin ends; exits 0, or 1 when it cannot run.
pub fn run_mcp(env: &HashMap<String, String>) -> ExitCode {
    let Ok(paths) = Paths::from_vars(env) else {
        return ExitCode::FAILURE;
    };
    let cfg = HelperConfig {
        sock: paths.run_sock(),
        token: env.get("DUOHIVE_TOKEN").filter(|t| !t.is_empty()).cloned(),
        connect_timeout: CONNECT_TIMEOUT,
        answer_timeout: ANSWER_TIMEOUT,
    };
    let closed: Arc<dyn ClosedFallback> = Arc::new(StoreFallback::new(paths));
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return ExitCode::FAILURE;
    };
    let served = rt.block_on(run_helper(
        cfg,
        tokio::io::stdin(),
        tokio::io::stdout(),
        closed,
    ));
    if served.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

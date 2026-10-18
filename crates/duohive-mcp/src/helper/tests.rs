use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use duohive_core::tool::Rights;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use super::*;

/// A fallback that knows two tokens and records every queued call.
#[derive(Default)]
struct FakeFallback {
    queued: Mutex<Vec<(String, String, Value)>>,
}

impl ClosedFallback for FakeFallback {
    fn rights(&self, token: &str) -> Option<Rights> {
        match token {
            "tok-duo" => Some(Rights::Duo),
            "tok-1" => Some(Rights::Agent),
            _ => None,
        }
    }

    fn queue(&self, token: &str, tool: &str, arguments: &Value) -> Result<u64, String> {
        let mut q = self.queued.lock().unwrap();
        q.push((token.to_owned(), tool.to_owned(), arguments.clone()));
        Ok(u64::try_from(q.len()).unwrap())
    }
}

fn config(dir: &Path, token: Option<&str>) -> HelperConfig {
    HelperConfig {
        sock: dir.join("run.sock"),
        token: token.map(str::to_owned),
        connect_timeout: Duration::from_millis(200),
        answer_timeout: Duration::from_secs(5),
    }
}

/// A fake DuoHive on `sock`: answers each request line with `answer(line)`,
/// or closes without answering when it returns `None`. Records every line.
fn fake_duohive(
    sock: PathBuf,
    answer: impl Fn(&Value) -> Option<Value> + Send + Sync + 'static,
) -> Arc<Mutex<Vec<Value>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let listener = UnixListener::bind(sock).unwrap();
    let record = Arc::clone(&seen);
    let answer = Arc::new(answer);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let record = Arc::clone(&record);
            let answer = Arc::clone(&answer);
            tokio::spawn(async move {
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                let v: Value = serde_json::from_str(&line).unwrap();
                record.lock().unwrap().push(v.clone());
                if let Some(a) = answer(&v) {
                    let mut stream = reader.into_inner();
                    stream.write_all(format!("{a}\n").as_bytes()).await.unwrap();
                }
            });
        }
    });
    seen
}

/// Runs the helper over `requests` (one JSON-RPC message each) and returns its answers.
async fn session(
    cfg: HelperConfig,
    closed: Arc<dyn ClosedFallback>,
    requests: &[Value],
) -> Vec<Value> {
    let mut input = String::new();
    for r in requests {
        input.push_str(&r.to_string());
        input.push('\n');
    }
    let mut output = Vec::new();
    run_helper(cfg, input.as_bytes(), &mut output, closed)
        .await
        .unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn init() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
           "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                      "clientInfo": {"name": "test", "version": "0"}}})
}

fn list() -> Value {
    json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
}

fn call(id: u64, tool: &str, arguments: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": tool, "arguments": arguments}})
}

fn tool_names(answer: &Value) -> Vec<String> {
    answer["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect()
}

fn error_code(answer: &Value) -> String {
    assert_eq!(answer["result"]["isError"], true, "{answer}");
    answer["result"]["structuredContent"]["error"]["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn initialize_then_lists_tools_by_rights() {
    for (rights, count) in [("agent", 3), ("duo", 9)] {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path(), Some("tok"));
        fake_duohive(cfg.sock.clone(), move |line| {
            assert_eq!(line["kind"], "tools");
            let tools: Vec<Value> = crate::tools::specs(if rights == "duo" {
                Rights::Duo
            } else {
                Rights::Agent
            })
            .into_iter()
            .map(|s| json!({"name": s.name, "description": s.description, "inputSchema": s.input_schema}))
            .collect();
            Some(json!({"ok": {"tools": tools}}))
        });
        let answers = session(cfg, Arc::new(FakeFallback::default()), &[init(), list()]).await;
        assert_eq!(answers[0]["result"]["serverInfo"]["name"], "duohive");
        assert_eq!(answers[0]["result"]["protocolVersion"], "2025-06-18");
        assert!(answers[0]["result"]["capabilities"]["tools"].is_object());
        assert_eq!(tool_names(&answers[1]).len(), count, "{rights}");
    }
}

#[tokio::test]
async fn call_relays_arguments_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path(), Some("tok-1"));
    let ok = json!({"message_id": 4, "status": "queued", "recipient_state": "working"});
    let answer = ok.clone();
    let seen = fake_duohive(cfg.sock.clone(), move |_| Some(json!({ "ok": answer })));
    let args = json!({"to": "pay-qa", "text": "hi ✉ \"quoted\"", "extra": [1, 2]});
    let answers = session(
        cfg,
        Arc::new(FakeFallback::default()),
        &[init(), call(3, "send_message", &args)],
    )
    .await;
    let line = seen.lock().unwrap()[0].clone();
    assert_eq!(line["kind"], "call");
    assert_eq!(line["token"], "tok-1");
    assert_eq!(line["tool"], "send_message");
    assert_eq!(line["arguments"], args);
    assert_eq!(answers[1]["id"], 3);
    assert_eq!(answers[1]["result"]["structuredContent"], ok);
    assert_eq!(
        answers[1]["result"]["content"][0]["text"].as_str().unwrap(),
        ok.to_string()
    );
    assert_ne!(answers[1]["result"]["isError"], true);
}

#[tokio::test]
async fn closed_reads_answer_closed() {
    let dir = tempfile::tempdir().unwrap();
    let fallback = Arc::new(FakeFallback::default());
    let reads = [
        call(3, "list_agents", &json!({})),
        call(4, "message_status", &json!({"message_id": 1})),
        call(5, "read_agent", &json!({"agent": "pay-qa"})),
        call(6, "wait_for_agent", &json!({"agent": "pay-qa"})),
    ];
    let mut requests = vec![init()];
    requests.extend(reads);
    let answers = session(
        config(dir.path(), Some("tok-duo")),
        Arc::clone(&fallback) as Arc<dyn ClosedFallback>,
        &requests,
    )
    .await;
    for a in &answers[1..] {
        assert_eq!(error_code(a), "duohive_closed");
        assert_eq!(
            a["result"]["structuredContent"]["error"]["message"],
            "DuoHive is closed"
        );
    }
    assert!(fallback.queued.lock().unwrap().is_empty());
}

#[tokio::test]
async fn closed_changes_queue_and_answer_queued() {
    let dir = tempfile::tempdir().unwrap();
    let fallback = Arc::new(FakeFallback::default());
    let changes = [
        call(3, "send_message", &json!({"to": "pay-qa", "text": "hi"})),
        call(4, "start_agent", &json!({"agent": "pay-qa", "task": "t"})),
        call(5, "stop_agent", &json!({"agent": "pay-qa"})),
        call(6, "assign_to_role", &json!({"role": "QA", "task": "t"})),
        call(
            7,
            "propose_org_change",
            &json!({"change": {"op": "add_team", "name": "G"}}),
        ),
    ];
    let mut requests = vec![init()];
    requests.extend(changes);
    let answers = session(
        config(dir.path(), Some("tok-duo")),
        Arc::clone(&fallback) as Arc<dyn ClosedFallback>,
        &requests,
    )
    .await;
    for (i, a) in answers[1..].iter().enumerate() {
        assert_eq!(
            a["result"]["structuredContent"],
            json!({"status": "queued_until_open", "request": i + 1})
        );
    }
    let queued = fallback.queued.lock().unwrap();
    let tools: Vec<&str> = queued.iter().map(|(_, t, _)| t.as_str()).collect();
    assert_eq!(
        tools,
        [
            "send_message",
            "start_agent",
            "stop_agent",
            "assign_to_role",
            "propose_org_change"
        ]
    );
    assert!(queued.iter().all(|(token, _, _)| token == "tok-duo"));
}

#[tokio::test]
async fn no_answer_in_time_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path(), Some("tok-duo"));
    cfg.answer_timeout = Duration::from_millis(200);
    let held = Arc::new(Mutex::new(Vec::new()));
    let keep = Arc::clone(&held);
    let listener = UnixListener::bind(&cfg.sock).unwrap();
    tokio::spawn(async move {
        // Accepts and holds every connection open, never answering.
        while let Ok((stream, _)) = listener.accept().await {
            keep.lock().unwrap().push(stream);
        }
    });
    let answers = session(
        cfg,
        Arc::new(FakeFallback::default()),
        &[init(), call(3, "list_agents", &json!({}))],
    )
    .await;
    assert_eq!(error_code(&answers[1]), "interrupted");
    assert_eq!(
        answers[1]["result"]["structuredContent"]["error"]["message"],
        "DuoHive sent no answer within 0 s"
    );
}

#[tokio::test]
async fn closed_mid_call_answers_interrupted_not_queued() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path(), Some("tok-duo"));
    fake_duohive(cfg.sock.clone(), |_| None);
    let fallback = Arc::new(FakeFallback::default());
    let answers = session(
        cfg,
        Arc::clone(&fallback) as Arc<dyn ClosedFallback>,
        &[
            init(),
            call(3, "send_message", &json!({"to": "pay-qa", "text": "hi"})),
        ],
    )
    .await;
    assert_eq!(error_code(&answers[1]), "interrupted");
    assert!(fallback.queued.lock().unwrap().is_empty());
}

#[tokio::test]
async fn closed_unknown_token_and_rights() {
    let dir = tempfile::tempdir().unwrap();
    let fallback = Arc::new(FakeFallback::default());
    let answers = session(
        config(dir.path(), Some("tok-nobody")),
        Arc::clone(&fallback) as Arc<dyn ClosedFallback>,
        &[
            init(),
            call(3, "list_agents", &json!({})),
            call(4, "send_message", &json!({"to": "pay-qa", "text": "hi"})),
        ],
    )
    .await;
    assert_eq!(error_code(&answers[1]), "unknown_token");
    assert_eq!(error_code(&answers[2]), "unknown_token");
    let answers = session(
        config(dir.path(), Some("tok-1")),
        Arc::clone(&fallback) as Arc<dyn ClosedFallback>,
        &[init(), call(3, "stop_agent", &json!({"agent": "pay-qa"}))],
    )
    .await;
    assert_eq!(error_code(&answers[1]), "not_allowed");
    assert!(fallback.queued.lock().unwrap().is_empty());
}

#[test]
fn wait_answer_timeout_is_fixed() {
    let cfg = config(Path::new("/nonexistent"), Some("t"));
    let wait = Duration::from_secs(60 * 60 + 30);
    assert_eq!(answer_timeout(&cfg, "wait_for_agent"), wait);
    assert_eq!(answer_timeout(&cfg, "send_message"), cfg.answer_timeout);
    assert_eq!(answer_timeout(&cfg, "list_agents"), cfg.answer_timeout);
    assert_eq!(answer_timeout(&cfg, "tools/list"), cfg.answer_timeout);
}

#[tokio::test]
async fn no_token_lists_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let answers = session(
        config(dir.path(), None),
        Arc::new(FakeFallback::default()),
        &[init(), list(), call(3, "list_agents", &json!({}))],
    )
    .await;
    assert!(tool_names(&answers[1]).is_empty());
    assert_eq!(error_code(&answers[2]), "unknown_token");
}

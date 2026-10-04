//! Tests for the driver loop: Task 15.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::run_core;
use crate::command::Command;
use crate::engine::Input;
use crate::engine::harness::{Harness, Setup, T0};
use crate::event::Event;
use crate::hook::HookMessage;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, TokioClock, n};

struct Running {
    commands: mpsc::Sender<Command>,
    inputs: mpsc::Sender<Input>,
    events: mpsc::UnboundedReceiver<Event>,
    task: JoinHandle<()>,
}

fn spawn(h: Harness) -> Running {
    let (commands, command_rx) = mpsc::channel(8);
    let (inputs, input_rx) = mpsc::channel(8);
    let (event_tx, events) = mpsc::unbounded_channel();
    let task = tokio::spawn(run_core(h.engine, command_rx, input_rx, event_tx));
    Running {
        commands,
        inputs,
        events,
        task,
    }
}

/// A harness whose engine reads tokio's (pausable) clock.
fn harness_on_tokio_time() -> Harness {
    Setup::default().build_on(Box::new(TokioClock::new(T0)))
}

#[tokio::test(start_paused = true)]
async fn driver_ticks_at_next_deadline() {
    let mut h = harness_on_tokio_time();
    h.start("auth-service");
    h.input(Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: "tok-1".into(),
        agent: "auth-service".into(),
        event: "busy".into(),
        payload: serde_json::json!({}),
    }));
    assert_eq!(h.state("auth-service"), AgentState::Working);
    let began = tokio::time::Instant::now();
    let mut core = spawn(h);
    let quiet = loop {
        let event = tokio::time::timeout(Duration::from_secs(600), core.events.recv())
            .await
            .expect("an event within 10 minutes")
            .expect("the core is running");
        if let Event::AgentStatusChanged { agent, status } = event
            && agent == n("auth-service")
            && status.quiet
        {
            break status;
        }
    };
    assert_eq!(quiet.state, AgentState::Working);
    assert!(
        began.elapsed() >= Duration::from_secs(300),
        "{:?}",
        began.elapsed()
    );
    assert!(
        began.elapsed() < Duration::from_secs(301),
        "{:?}",
        began.elapsed()
    );
    drop(core.commands);
    drop(core.inputs);
    core.task.await.unwrap();
}

#[tokio::test]
async fn driver_stops_on_shutdown() {
    let mut h = Harness::new();
    h.start("auth-service");
    let calls = h.call_log.clone();
    let core = spawn(h);
    core.commands.send(Command::Shutdown).await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), core.task)
        .await
        .expect("the core ends within 1 s")
        .unwrap();
    let calls = calls.lock().unwrap().clone();
    assert!(
        !calls
            .iter()
            .any(|c| matches!(c, RuntimeCall::Stop(..) | RuntimeCall::Kill(..))),
        "{calls:?}"
    );
}

#[tokio::test]
async fn driver_sends_the_events_of_commands_and_inputs() {
    let mut core = spawn(Harness::new());
    core.commands
        .send(Command::StartAgent { agent: n("pay-qa") })
        .await
        .unwrap();
    let event = core.events.recv().await.unwrap();
    assert!(
        matches!(&event, Event::AgentStatusChanged { agent, status }
            if *agent == n("pay-qa") && status.state == AgentState::Starting),
        "{event:?}"
    );
    core.inputs
        .send(Input::Command(Command::StopAgent { agent: n("nobody") }))
        .await
        .unwrap();
    assert_eq!(
        core.events.recv().await,
        Some(Event::CommandRejected {
            reason: "no agent named nobody".into()
        })
    );
}

#[tokio::test]
async fn driver_ends_when_both_channels_close() {
    let core = spawn(Harness::new());
    drop(core.commands);
    // Still running on inputs alone.
    core.inputs.send(Input::Tick).await.unwrap();
    drop(core.inputs);
    tokio::time::timeout(Duration::from_secs(1), core.task)
        .await
        .expect("the core ends within 1 s")
        .unwrap();
}

#[tokio::test]
async fn driver_ends_when_no_one_listens() {
    let core = spawn(Harness::new());
    drop(core.events);
    core.commands
        .send(Command::StartAgent { agent: n("pay-qa") })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), core.task)
        .await
        .expect("the core ends within 1 s")
        .unwrap();
}

fn busy_hook(i: usize) -> HookMessage {
    HookMessage {
        sent_at_ms: 0,
        token: format!("tok-{i}"),
        agent: "auth-service".into(),
        event: "busy".into(),
        payload: serde_json::json!({ "i": i }),
    }
}

#[tokio::test]
async fn shutdown_keeps_unapplied_hooks_for_the_next_start() {
    // Reconnecting (no inventory yet): a hook the core takes before `Shutdown`
    // is buffered in the engine, one it doesn't is still queued. Either way it
    // must reach `pending_hooks`, in arrival order (Review Focus 3).
    let mut h = Harness::new();
    h.engine.start_up();
    let store = h.store.clone();
    let (commands, command_rx) = mpsc::channel(8);
    let (inputs, input_rx) = mpsc::channel(8);
    let (event_tx, _events) = mpsc::unbounded_channel();
    let hooks: Vec<HookMessage> = (1..=3).map(busy_hook).collect();
    for hook in &hooks {
        inputs.send(Input::Hook(hook.clone())).await.unwrap();
    }
    commands.send(Command::Shutdown).await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        run_core(h.engine, command_rx, input_rx, event_tx),
    )
    .await
    .expect("the core ends within 1 s");
    assert_eq!(store.state.lock().unwrap().pending, hooks);
    assert!(inputs.is_closed(), "the core still takes inputs");
}

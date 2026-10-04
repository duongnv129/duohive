//! The core driver loop (TD §3.3): the engine's only owner, fed by commands,
//! inputs and its own timers. Uses only tokio `sync` and `time`.

use tokio::sync::mpsc::{Receiver, UnboundedSender};
use tokio::time::{Instant, sleep_until};

use super::{Engine, Input};
use crate::command::Command;
use crate::event::Event;

/// Runs `engine` until `Command::Shutdown`, until both channels close, or
/// until no one listens for events,
/// sending every event it returns. A `Tick` goes in when the next timer is due.
/// Shutdown leaves every agent running (their windows outlive DuoHive) and
/// stores the hooks it has not applied as pending hooks (Review Focus 3).
pub async fn run_core(
    mut engine: Engine,
    mut commands: Receiver<Command>,
    mut inputs: Receiver<Input>,
    events: UnboundedSender<Event>,
) {
    let mut commands_open = true;
    let mut inputs_open = true;
    while commands_open || inputs_open {
        let due = next_tick(&engine);
        let input = tokio::select! {
            command = commands.recv(), if commands_open => match command {
                Some(command) => Input::Command(command),
                None => {
                    commands_open = false;
                    continue;
                }
            },
            input = inputs.recv(), if inputs_open => match input {
                Some(input) => input,
                None => {
                    inputs_open = false;
                    continue;
                }
            },
            () = sleep_until(due.unwrap_or_else(Instant::now)), if due.is_some() => Input::Tick,
        };
        if matches!(input, Input::Command(Command::Shutdown)) {
            keep_queued_hooks(&mut engine, &mut inputs);
            return;
        }
        for event in engine.handle(input) {
            if events.send(event).is_err() {
                tracing::warn!("no one is listening for events: the core stops");
                return;
            }
        }
    }
}

/// Stops taking inputs and keeps every hook not yet applied for the next start;
/// other queued inputs are dropped (start-up rebuilds them from tmux).
fn keep_queued_hooks(engine: &mut Engine, inputs: &mut Receiver<Input>) {
    inputs.close();
    let mut queued = Vec::new();
    while let Ok(input) = inputs.try_recv() {
        if let Input::Hook(message) = input {
            queued.push(message);
        }
    }
    engine.keep_unapplied_hooks(queued);
}

/// When the engine's next timer is due, on tokio's clock.
fn next_tick(engine: &Engine) -> Option<Instant> {
    let at = engine.next_deadline()?;
    let now = engine.ports.clock.now();
    Some(Instant::now() + (at - now))
}

#[cfg(test)]
mod tests;

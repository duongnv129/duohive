//! Reconnect at start-up (TD §4.1 Start-up; Review Focus 3; D9, D10, D22, D26).
//!
//! `start_up` restores the session rows and asks for the live windows; the
//! inventory then settles each row, replays the hooks queued while DuoHive
//! was closed (and the live ones that arrived meanwhile), and only then
//! applies the exit rules to dead panes.

mod held;

pub(super) use held::Held;
use held::is_live;

use super::org_edit::drop_refusal;
use super::{Engine, Exit};
use crate::event::Event;
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::names::AgentName;
use crate::org::Org;
use crate::ports::{LiveWindow, OrgFileError, SessionRow};
use crate::status::{AgentState, Trigger};

/// What start-up holds until the inventory arrives.
#[derive(Debug)]
pub(super) struct Reconnect {
    /// Agents whose rows were restored.
    restored: Vec<AgentName>,
    /// Rows for agents the org doesn't name, settled by the inventory.
    orphans: Vec<SessionRow>,
    /// Live hooks that arrived before the replay, in arrival order.
    buffered: Vec<HookMessage>,
    /// Restored rows in a running state (UI §4.1.2's "Reconnecting to N agents…").
    pub(super) reconnecting: u32,
}

/// A dead pane found by the inventory, settled after the replay.
type DeadPane = (AgentName, WindowId, Option<i32>);

impl Engine {
    /// Restores the session rows, asks the runtime for its windows and returns
    /// the `Snapshot`. Never probes directories (D26).
    pub fn start_up(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        let rows = self.ports.store.load_sessions().unwrap_or_else(|e| {
            out.push(Event::CommandRejected {
                reason: format!("couldn't load state: {e}"),
            });
            Vec::new()
        });
        // Stopped means no row: a stale one goes.
        let (stopped, rows): (Vec<_>, Vec<_>) = rows
            .into_iter()
            .partition(|r| r.status.state == AgentState::Stopped);
        for row in stopped {
            self.delete_row(&row.agent, &mut out);
        }
        let (known, orphans): (Vec<_>, Vec<_>) =
            rows.into_iter().partition(|r| self.knows(&r.agent));
        let running = known.iter().filter(|r| r.status.state.is_running()).count();
        let restored = known.into_iter().map(|row| self.restore(row)).collect();
        // With rows for agents the org no longer names, the inventory decides.
        if orphans.is_empty() && self.org_error.is_none() {
            self.mark_last_good(&mut out);
        }
        self.reconnect = Some(Reconnect {
            restored,
            orphans,
            buffered: Vec::new(),
            reconnecting: u32::try_from(running).unwrap_or(u32::MAX),
        });
        self.needs_you_shown = self.needs_you();
        self.ports.runtime.inventory();
        out.insert(0, Event::Snapshot(Box::new(self.snapshot())));
        out
    }

    /// A hook: buffered until the start-up replay, else handled now.
    pub(super) fn hook_or_buffer(&mut self, message: HookMessage) -> Vec<Event> {
        if let Some(reconnect) = self.reconnect.as_mut() {
            reconnect.buffered.push(message);
            return Vec::new();
        }
        self.on_hook(&message)
    }

    /// The start-up inventory, in D9's order. A later one only settles held rows.
    pub(super) fn on_inventory(&mut self, windows: &[LiveWindow]) -> Vec<Event> {
        let Some(reconnect) = self.reconnect.take() else {
            let mut out = self.check_adopted(windows);
            out.extend(self.settle_held(windows));
            return out;
        };
        let mut out = Vec::new();
        let mut restored = reconnect.restored;
        let adopted = self.settle_orphans(reconnect.orphans, &restored, windows, &mut out);
        restored.extend(adopted);
        let dead = self.match_windows(&restored, windows, &mut out);
        let unknown = windows
            .iter()
            .filter(|w| !restored.contains(&w.agent) && !self.is_held(&w.agent));
        for w in unknown {
            tracing::info!(agent = %w.agent, window = %w.window.0, "window with no session row left running");
        }
        self.replay(reconnect.buffered, &mut out);
        for (agent, window, status) in dead {
            out.extend(self.on_exit(&agent, &window, Exit::PaneDied(status)));
        }
        self.finish_reconnect(&mut out);
        out
    }

    /// `ConnectionLost`. Before the start-up inventory it ends the reconnect:
    /// the server took every window, so orphan rows go; queued then buffered
    /// hooks replay in order, then running agents are errored.
    pub(super) fn connection_lost(&mut self, reason: &str) -> Vec<Event> {
        let Some(reconnect) = self.reconnect.take() else {
            let mut out = self.settle_held(&[]);
            out.extend(self.on_connection_lost(reason));
            return out;
        };
        let mut out = Vec::new();
        self.drop_rows(reconnect.orphans, &[], &mut out);
        if self.org_error.is_none() {
            self.mark_last_good(&mut out);
        }
        self.replay(reconnect.buffered, &mut out);
        out.extend(self.on_connection_lost(reason));
        self.finish_reconnect(&mut out);
        out
    }

    /// Step 4: `NeedsYouChanged` (always) and `Reconnected`.
    fn finish_reconnect(&mut self, out: &mut Vec<Event>) {
        let items = self.needs_you();
        self.needs_you_shown.clone_from(&items);
        out.push(Event::NeedsYouChanged(items));
        out.push(Event::Reconnected);
    }

    /// Puts a row's state back, with `last_signal_at = now`: time DuoHive was
    /// closed is not silence it saw. A starting agent gets a fresh start timeout.
    pub(super) fn restore(&mut self, row: SessionRow) -> AgentName {
        let now = self.ports.clock.now();
        let deadline =
            (row.status.state == AgentState::Starting).then(|| now + self.cfg.start_timeout);
        let rt = self.rt_mut(&row.agent);
        rt.status = row.status;
        rt.token = row.token;
        rt.window = row.window;
        rt.last_reply = row.last_reply;
        rt.task_at_launch = row.task_at_launch;
        rt.saw_end_hook = row.saw_end_hook;
        rt.last_task = row.last_task;
        rt.last_signal_at = now;
        rt.start_deadline = deadline;
        row.agent
    }

    /// Step 1: live windows keep their state, dead panes wait for step 3, and a
    /// running agent with no window is stopped without an error (D9). Other
    /// windows are never touched.
    pub(super) fn match_windows(
        &mut self,
        restored: &[AgentName],
        windows: &[LiveWindow],
        out: &mut Vec<Event>,
    ) -> Vec<DeadPane> {
        let mut dead = Vec::new();
        for agent in restored {
            if !self.agents.contains_key(agent) {
                // Named by the loaded org only, and the last good org was kept.
                self.delete_row(agent, out);
                continue;
            }
            match windows.iter().find(|w| &w.agent == agent) {
                Some(w) => {
                    self.set_window(agent, Some(w.window.clone()), out);
                    match w.dead {
                        Some(status) => dead.push((agent.clone(), w.window.clone(), status)),
                        None => self.rt_mut(agent).last_activity_secs = Some(w.activity_secs),
                    }
                }
                None if self.state_of(agent).is_running() => {
                    self.rt_mut(agent).window = None;
                    self.change(agent, &Trigger::Stop, None, out);
                }
                None => self.set_window(agent, None, out),
            }
        }
        dead
    }

    /// Step 2: the queued hooks in order, then the live ones buffered meanwhile.
    fn replay(&mut self, buffered: Vec<HookMessage>, out: &mut Vec<Event>) {
        let pending = self.ports.store.take_pending_hooks().unwrap_or_else(|e| {
            out.push(Event::CommandRejected {
                reason: format!("couldn't read queued hooks: {e}"),
            });
            Vec::new()
        });
        for message in pending.into_iter().chain(buffered) {
            out.extend(self.on_hook(&message));
        }
    }

    /// Rows for agents `org.toml` no longer names (edited while DuoHive was
    /// closed). Rows whose window is gone or dead are dropped. While a live
    /// one remains, the loaded org is never accepted: the last good org is
    /// kept, as a reload would, when it names every live agent (those orphans
    /// and the loaded org's own); otherwise the rows are held. Returns the
    /// agents restored here.
    fn settle_orphans(
        &mut self,
        orphans: Vec<SessionRow>,
        restored: &[AgentName],
        windows: &[LiveWindow],
        out: &mut Vec<Event>,
    ) -> Vec<AgentName> {
        // With none, start-up has already accepted the org.
        if orphans.is_empty() {
            return Vec::new();
        }
        let (live, gone): (Vec<_>, Vec<_>) = orphans
            .into_iter()
            .partition(|r| is_live(windows, &r.agent));
        self.drop_rows(gone, windows, out);
        let Some(refusal) = live
            .iter()
            .find_map(|r| drop_refusal(&r.agent, r.status.state))
        else {
            if self.org_error.is_none() {
                self.mark_last_good(out);
            }
            return Vec::new();
        };
        let must_name: Vec<AgentName> = live
            .iter()
            .map(|r| r.agent.clone())
            .chain(
                restored
                    .iter()
                    .filter(|a| is_live(windows, a) && self.state_of(a) != AgentState::Stopped)
                    .cloned(),
            )
            .collect();
        let keeper = if self.org_error.is_none() {
            self.ports
                .store
                .last_good_org()
                .filter(|org| must_name.iter().all(|a| names(org, a)))
        } else {
            None
        };
        match keeper {
            Some(org) => self.keep_last_good(org, refusal, live, out),
            None => {
                self.hold(live, out);
                Vec::new()
            }
        }
    }

    /// Starts on the last good org with the banner, so a running agent the
    /// file dropped stays manageable; restores the rows (that org names them all).
    fn keep_last_good(
        &mut self,
        org: Org,
        e: OrgFileError,
        rows: Vec<SessionRow>,
        out: &mut Vec<Event>,
    ) -> Vec<AgentName> {
        let old = std::mem::replace(&mut self.org, org);
        self.sync_agents();
        let restored = rows.into_iter().map(|row| self.restore(row)).collect();
        out.push(self.org_changed(&old));
        self.org_error = Some(e.clone());
        out.push(Event::OrgFileError(Some(e)));
        restored
    }

    /// Deletes rows for agents no org in use names. D9: their windows are
    /// never killed; a dead pane is left too, as it may hold the last output.
    pub(super) fn drop_rows(
        &mut self,
        rows: Vec<SessionRow>,
        windows: &[LiveWindow],
        out: &mut Vec<Event>,
    ) {
        for row in rows {
            match windows.iter().find(|w| w.agent == row.agent) {
                Some(w) if w.dead.is_some() => tracing::info!(
                    agent = %row.agent, window = %w.window.0,
                    "agent not in the org: row dropped, dead pane left with its last output"
                ),
                Some(w) => tracing::info!(
                    agent = %row.agent, window = %w.window.0,
                    "agent not in the org: row dropped, window left running"
                ),
                None => {}
            }
            self.delete_row(&row.agent, out);
        }
    }

    /// Records `agent`'s window, saving the row when it changed (a stopped
    /// agent has no row, so none is saved for it).
    fn set_window(&mut self, agent: &AgentName, window: Option<WindowId>, out: &mut Vec<Event>) {
        let stopped = self.state_of(agent) == AgentState::Stopped;
        let rt = self.rt_mut(agent);
        if rt.window != window {
            rt.window = window;
            if !stopped {
                self.save_row(agent, out);
            }
        }
    }
}

/// Whether `org` names `agent` (Duo included).
fn names(org: &Org, agent: &AgentName) -> bool {
    org.is_duo(agent) || org.find_agent(agent).is_some()
}

#[cfg(test)]
mod tests;

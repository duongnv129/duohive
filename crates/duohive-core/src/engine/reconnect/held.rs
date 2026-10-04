//! Held rows: running agents `org.toml` no longer names and no other org
//! names either (controller ruling B). The loaded org is used but not
//! accepted (never marked last good), the banner keeps in-app edits refused,
//! and the rows stay until a reload names the agent again (it is adopted, row
//! and token kept) or its window is gone.

use std::collections::BTreeMap;

use super::super::{Engine, Exit};
use crate::event::Event;
use crate::launch::WindowId;
use crate::names::AgentName;
use crate::ports::{LiveWindow, OrgFileError, SessionRow};
use crate::status::AgentState;

/// Rows held for running agents no org names.
#[derive(Debug, Default)]
pub(in crate::engine) struct Held {
    rows: Vec<SessionRow>,
    /// The banner shown is the held refusal (not a file error of its own).
    owns_banner: bool,
    /// Held agents whose pane died while held, with its exit status.
    dead: BTreeMap<AgentName, Option<i32>>,
    /// Adopted agents whose windows the next inventory checks.
    adopted: Vec<AgentName>,
}

impl Engine {
    /// Holds `rows`; unless `org.toml` has an error of its own, the banner names the first.
    pub(super) fn hold(&mut self, rows: Vec<SessionRow>, out: &mut Vec<Event>) {
        self.held.rows = rows;
        if self.org_error.is_some() {
            return;
        }
        if let Some(e) = self.held_refusal() {
            self.held.owns_banner = true;
            self.org_error = Some(e.clone());
            out.push(Event::OrgFileError(Some(e)));
        }
    }

    /// Whether rows are held, so the current org can't be accepted.
    pub(in crate::engine) fn holds_rows(&self) -> bool {
        !self.held.rows.is_empty()
    }

    /// After a reload: adopts the held rows the org names now (reporting their
    /// status) and returns the banner still needed, if any. While rows stay
    /// held, the runtime is asked for its windows so a gone one is noticed.
    pub(in crate::engine) fn adopt_held(&mut self, out: &mut Vec<Event>) -> Option<OrgFileError> {
        let rows = std::mem::take(&mut self.held.rows);
        let (named, still): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| self.knows(&r.agent));
        let adopting = !named.is_empty();
        for row in named {
            let window = row.window.clone();
            let agent = self.restore(row);
            out.push(Event::AgentStatusChanged {
                status: self.rt_mut(&agent).status.clone(),
                agent: agent.clone(),
            });
            // Its pane died while it was held: the exit rules apply now.
            if let (Some(status), Some(window)) = (self.held.dead.remove(&agent), window) {
                out.extend(self.on_exit(&agent, &window, Exit::PaneDied(status)));
            }
            self.held.adopted.push(agent);
        }
        self.held.rows = still;
        let banner = self.held_refusal();
        self.held.owns_banner = banner.is_some();
        if banner.is_some() || adopting {
            self.ports.runtime.inventory();
        }
        banner
    }

    /// An exit of a held agent's window (`on_exit` knows no such agent): gone,
    /// the row goes; dead, the exit is remembered for adoption. `None` when
    /// the window is no held agent's.
    pub(in crate::engine) fn held_exit(
        &mut self,
        agent: &AgentName,
        window: &WindowId,
        exit: Exit,
    ) -> Option<Vec<Event>> {
        let at = self
            .held
            .rows
            .iter()
            .position(|r| &r.agent == agent && r.window.as_ref() == Some(window))?;
        let mut out = Vec::new();
        match exit {
            Exit::PaneDied(status) => {
                self.held.dead.insert(agent.clone(), status);
            }
            Exit::WindowGone => {
                self.held.rows.remove(at);
                self.held.dead.remove(agent);
                self.delete_row(agent, &mut out);
                self.update_held_banner(&mut out);
            }
        }
        Some(out)
    }

    /// Whether `agent` is held.
    pub(super) fn is_held(&self, agent: &AgentName) -> bool {
        self.held.rows.iter().any(|r| &r.agent == agent)
    }

    /// A later inventory checks the windows of agents adopted since: gone →
    /// stopped (D9), dead → the exit rules.
    pub(super) fn check_adopted(&mut self, windows: &[LiveWindow]) -> Vec<Event> {
        let adopted: Vec<AgentName> = std::mem::take(&mut self.held.adopted)
            .into_iter()
            .filter(|a| self.agents.contains_key(a))
            .collect();
        let mut out = Vec::new();
        for (agent, window, status) in self.match_windows(&adopted, windows, &mut out) {
            out.extend(self.on_exit(&agent, &window, Exit::PaneDied(status)));
        }
        out
    }

    /// `org.toml` has an error of its own: that banner replaces the held one.
    pub(in crate::engine) fn disown_banner(&mut self) {
        self.held.owns_banner = false;
    }

    /// A later inventory, or the server's exit (no windows): held rows whose
    /// window is gone are dropped. Once none is left, the held banner clears
    /// and the current org is accepted.
    pub(super) fn settle_held(&mut self, windows: &[LiveWindow]) -> Vec<Event> {
        if self.held.rows.is_empty() {
            return Vec::new();
        }
        let rows = std::mem::take(&mut self.held.rows);
        let (live, gone): (Vec<_>, Vec<_>) =
            rows.into_iter().partition(|r| is_live(windows, &r.agent));
        let mut out = Vec::new();
        for row in &gone {
            self.held.dead.remove(&row.agent);
        }
        self.drop_rows(gone, windows, &mut out);
        self.held.rows = live;
        self.update_held_banner(&mut out);
        out
    }

    /// Keeps the held banner in step with the held rows; with none left it
    /// clears and the current org is accepted.
    fn update_held_banner(&mut self, out: &mut Vec<Event>) {
        if !self.held.owns_banner {
            return;
        }
        match self.held_refusal() {
            Some(e) if self.org_error.as_ref() != Some(&e) => {
                self.org_error = Some(e.clone());
                out.push(Event::OrgFileError(Some(e)));
            }
            Some(_) => {}
            None => {
                self.held.owns_banner = false;
                self.org_error = None;
                self.mark_last_good(out);
                out.push(Event::OrgFileError(None));
            }
        }
    }

    fn held_refusal(&self) -> Option<OrgFileError> {
        self.held
            .rows
            .iter()
            .find_map(|r| missing_refusal(&r.agent, r.status.state))
    }
}

/// Whether `agent` has a window whose pane is still alive.
pub(super) fn is_live(windows: &[LiveWindow], agent: &AgentName) -> bool {
    windows
        .iter()
        .any(|w| &w.agent == agent && w.dead.is_none())
}

/// The banner for a held agent; `None` when stopped.
fn missing_refusal(agent: &AgentName, state: AgentState) -> Option<OrgFileError> {
    let how = match state {
        AgentState::Stopped => return None,
        AgentState::Errored => "errored",
        _ => "running",
    };
    Some(OrgFileError {
        line: None,
        message: format!(
            "{agent} is {how} but missing from org.toml · add it back, or stop it, then press o to reload"
        ),
    })
}

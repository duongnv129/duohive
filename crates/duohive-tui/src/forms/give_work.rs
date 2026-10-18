//! The Give work dialog (UI §4.5, P2-D11, P2-D20, P3-D28, P3-D29): a task box, and for a
//! stopped or errored agent where the fresh session runs. A finished or idle agent takes it as
//! its next turn; a busy one gets it as a message instead; one still taking the last task
//! can't be given work yet. A role's variant gives it to a free agent, or proposes a new one.

use std::path::Path;

use duohive_core::command::Command;
use duohive_core::event::AgentView;
use duohive_core::inherit::{expand_home, resolve};
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, Org, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::time::Millis;
use duohive_core::validate::{DirProbe, DirStatus};
use duohive_core::worktree::{RunIn, TaskId};

use super::LineEdit;
use crate::input::Action;

/// The keys the dialog offers while Give work is possible.
const HINT_GIVE: &str = "enter give · alt+enter new line · esc";
const HINT_GIVE_RUN: &str = "enter give · alt+enter new line · tab run · esc";
/// A busy agent's: the task goes as a message.
const HINT_MESSAGE: &str = "enter send as message · alt+enter new line · esc";
/// A role with no free agent's (UI §4.5).
const HINT_PROPOSE: &str = "enter propose one · esc";
/// The keys while `enter` does nothing.
const HINT_CLOSE: &str = "esc close";

/// What the agent's state, or the role's free agents, let the dialog do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Variant {
    /// Stopped or errored: a fresh session, run in the directory or a new worktree.
    Fresh,
    /// Finished or idle: the next turn of the session, where it already runs.
    NextTurn {
        /// The worktree it runs in, if any.
        worktree: Option<TaskId>,
    },
    /// Starting, working or waiting: Give work is disabled; the task can go as a message.
    Busy,
    /// At rest, but still taking the last task (P2-D11).
    Pending,
    /// A role with a free agent: `free[pick]` takes the task (P3-D28).
    Role {
        /// The role.
        role: RoleKey,
        /// Its free agents, at rest longest first.
        free: Vec<AgentName>,
        /// All its agents.
        total: usize,
        /// The one in `Goes to`.
        pick: usize,
    },
    /// A role with no free agent: `enter` proposes one (P3-D3).
    RoleNoneFree {
        /// The role.
        role: RoleKey,
        /// All its agents.
        total: usize,
    },
}

/// What has the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiveFocus {
    /// The task box.
    Task,
    /// A role's `Goes to`.
    Pick,
    /// The Run choice.
    Run,
    /// A busy agent's `Send as message instead`.
    SendAsMessage,
}

/// What a key did to the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GiveOutcome {
    /// Keep it open.
    Continue,
    /// Close it.
    Close,
    /// Give the work, and close.
    Send(Box<Command>),
    /// Give the work, and stay open for the engine's answer (a role's, P3-D28).
    Wait(Box<Command>),
}

/// An agent the dialog gives work to: what its title and Run row need.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    agent: AgentName,
    state: AgentState,
    since: Millis,
    worktree: Option<TaskId>,
    dir: Option<String>,
    default_worktree: bool,
    worktree_blocked: Option<String>,
}

impl Target {
    fn new(
        org: &Org,
        agent: &AgentName,
        view: &AgentView,
        dirs: &dyn DirProbe,
        home: &Path,
    ) -> Self {
        let resolved = resolve(org, agent).ok();
        let dir = resolved.as_ref().and_then(|r| r.directory.clone());
        let default_worktree = resolved.is_some_and(|r| r.worktree);
        let worktree_blocked = match &dir {
            Some(d) if dirs.probe(&expand_home(d, home)) == DirStatus::GitRepo => None,
            Some(d) => Some(format!("new worktree needs a git repo; {d} isn't one")),
            None => Some("new worktree needs a git repo; no directory is set".to_owned()),
        };
        Self {
            agent: agent.clone(),
            state: view.status.state,
            since: view.status.since,
            worktree: view.worktree.clone(),
            dir,
            default_worktree,
            worktree_blocked,
        }
    }

    /// Where a role with no free agent points: nobody (Duo's name stands in).
    fn nobody(org: &Org) -> Self {
        Self {
            agent: org.duo.name.clone(),
            state: AgentState::Stopped,
            since: Millis(0),
            worktree: None,
            dir: None,
            default_worktree: false,
            worktree_blocked: None,
        }
    }

    /// The Run choice preset from the agent's default, where a worktree can be made.
    fn preset(&self) -> RunIn {
        if self.default_worktree && self.worktree_blocked.is_none() {
            RunIn::NewWorktree
        } else {
            RunIn::Directory
        }
    }
}

/// An open Give work dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GiveWork {
    /// Who the work goes to: a role's pick; Duo's name stands in while none is free.
    pub agent: AgentName,
    /// Its state, for the title and a role's `Goes to`.
    pub state: AgentState,
    /// When that state began.
    pub since: Millis,
    /// What the state allows.
    pub variant: Variant,
    /// The task.
    pub task: LineEdit,
    /// Where a fresh session runs.
    pub run: RunIn,
    /// What has the keys.
    pub focus: GiveFocus,
    /// The agent's directory, as configured.
    pub dir: Option<String>,
    /// The agent's own worktree default.
    pub default_worktree: bool,
    /// Why `new worktree` can't be chosen, when it can't.
    pub worktree_blocked: Option<String>,
    /// A line under the task box (the task wasn't taken).
    pub note: Option<String>,
    /// A role's task is with the engine; `enter` waits for its answer.
    pub sent: bool,
    /// A role's path, `Payments › Backend` or `Generalist`.
    path: String,
    /// A role's free agents, in `Variant::Role`'s order.
    targets: Vec<Target>,
}

impl GiveWork {
    /// The dialog for `agent`, with `task` already in the box.
    pub fn new(
        org: &Org,
        agent: &AgentName,
        view: &AgentView,
        task: &str,
        dirs: &dyn DirProbe,
        home: &Path,
    ) -> Self {
        let t = Target::new(org, agent, view, dirs, home);
        Self::with(&t, variant_of(view), task, String::new(), Vec::new())
    }

    /// The dialog for `role`, whose agents in tree order are `members`. Its free agents
    /// (stopped, finished or idle, not still taking a task) are offered at rest longest
    /// first, ties in tree order (P3-D28).
    pub fn for_role(
        org: &Org,
        role: &RoleKey,
        members: &[(AgentName, AgentView)],
        task: &str,
        dirs: &dyn DirProbe,
        home: &Path,
    ) -> Self {
        let mut free: Vec<&(AgentName, AgentView)> =
            members.iter().filter(|(_, v)| is_free(v)).collect();
        // Stable, so equal times keep tree order.
        free.sort_by_key(|(_, v)| v.status.since);
        let targets: Vec<Target> = free
            .iter()
            .map(|(a, v)| Target::new(org, a, v, dirs, home))
            .collect();
        let total = members.len();
        let variant = if targets.is_empty() {
            Variant::RoleNoneFree {
                role: role.clone(),
                total,
            }
        } else {
            Variant::Role {
                role: role.clone(),
                free: targets.iter().map(|t| t.agent.clone()).collect(),
                total,
                pick: 0,
            }
        };
        let first = targets
            .first()
            .cloned()
            .unwrap_or_else(|| Target::nobody(org));
        let path = org.breadcrumb(&NodeKey::Role(role.clone()));
        Self::with(&first, variant, task, path, targets)
    }

    fn with(t: &Target, variant: Variant, task: &str, path: String, targets: Vec<Target>) -> Self {
        Self {
            agent: t.agent.clone(),
            state: t.state,
            since: t.since,
            variant,
            task: LineEdit::new(task),
            run: t.preset(),
            focus: GiveFocus::Task,
            dir: t.dir.clone(),
            default_worktree: t.default_worktree,
            worktree_blocked: t.worktree_blocked.clone(),
            note: None,
            sent: false,
            path,
            targets,
        }
    }

    /// The role the dialog gives work to, if it is a role's.
    pub fn role(&self) -> Option<&RoleKey> {
        match &self.variant {
            Variant::Role { role, .. } | Variant::RoleNoneFree { role, .. } => Some(role),
            _ => None,
        }
    }

    /// The worktree a role's finished or idle pick runs in.
    pub fn pick_worktree(&self) -> Option<&TaskId> {
        match &self.variant {
            Variant::Role { pick, .. } => self.targets.get(*pick)?.worktree.as_ref(),
            _ => None,
        }
    }

    /// Keeps the run choice the work was last sent with, where it can still be chosen.
    pub fn set_run(&mut self, run: Option<RunIn>) {
        if let Some(run) = run
            && (run == RunIn::Directory || self.worktree_blocked.is_none())
        {
            self.run = run;
        }
    }

    /// Follows the agent: its state or pending delivery changed while the dialog was open.
    pub fn sync(&mut self, view: &AgentView) {
        self.state = view.status.state;
        self.since = view.status.since;
        self.variant = variant_of(view);
        self.keep_focus_valid();
    }

    /// Follows the role: `fresh` is the dialog rebuilt from its agents' states now. The
    /// task, note, focus and pick (while it is still free) carry over.
    pub fn follow_role(&mut self, mut fresh: GiveWork) {
        let was = match &self.variant {
            Variant::Role { free, pick, .. } => free.get(*pick).cloned(),
            _ => None,
        };
        let at = match (&was, &fresh.variant) {
            (Some(name), Variant::Role { free, .. }) => free.iter().position(|a| a == name),
            _ => None,
        };
        if let Some(i) = at {
            fresh.select(i);
            fresh.set_run(Some(self.run));
        }
        fresh.task = std::mem::take(&mut self.task);
        fresh.note = self.note.take();
        fresh.sent = self.sent;
        fresh.focus = self.focus;
        fresh.keep_focus_valid();
        *self = fresh;
    }

    /// The title in the top border.
    pub fn title(&self) -> String {
        match &self.variant {
            Variant::Role { total, .. } | Variant::RoleNoneFree { total, .. } => format!(
                "Give work · role {} ({} of {total} free)",
                self.path,
                self.targets.len()
            ),
            _ => format!(
                "Give work · {} · {} {}",
                self.agent,
                self.state.glyph(),
                self.state.word()
            ),
        }
    }

    /// Run is a choice for a fresh session: a stopped or errored agent, or such a pick
    /// (P3-D29).
    pub fn has_run_choice(&self) -> bool {
        match self.variant {
            Variant::Fresh => true,
            Variant::Role { .. } => {
                matches!(self.state, AgentState::Stopped | AgentState::Errored)
            }
            _ => false,
        }
    }

    /// `enter` does something: the variant allows it, the task isn't empty, and a role's
    /// last task isn't still with the engine.
    pub fn can_give(&self) -> bool {
        let allowed = match self.variant {
            Variant::Fresh | Variant::NextTurn { .. } | Variant::Busy => true,
            Variant::Role { .. } | Variant::RoleNoneFree { .. } => !self.sent,
            Variant::Pending => false,
        };
        allowed && !self.task.text().trim().is_empty()
    }

    /// The keys the dialog offers now.
    pub fn hint(&self) -> &'static str {
        if !self.can_give() {
            return HINT_CLOSE;
        }
        match self.variant {
            Variant::Fresh => HINT_GIVE_RUN,
            Variant::Busy => HINT_MESSAGE,
            Variant::RoleNoneFree { .. } => HINT_PROPOSE,
            _ => HINT_GIVE,
        }
    }

    /// Applies a key.
    pub fn handle(&mut self, a: Action) -> GiveOutcome {
        match a {
            Action::Esc => return GiveOutcome::Close,
            Action::DialogEnter => return self.give(),
            Action::Tab => self.step_focus(true),
            Action::BackTab => self.step_focus(false),
            // `↑↓` pick another free agent, wherever the focus is (UI §4.5).
            Action::Up | Action::Down => self.step_pick(a == Action::Down),
            _ => match self.focus {
                GiveFocus::Task => self.task_key(a),
                GiveFocus::Run => self.run_key(a),
                GiveFocus::Pick | GiveFocus::SendAsMessage => {}
            },
        }
        GiveOutcome::Continue
    }

    /// Pastes into the task box.
    pub fn paste(&mut self, text: &str) {
        if self.focus == GiveFocus::Task {
            self.task.insert_str(text);
        }
    }

    fn give(&mut self) -> GiveOutcome {
        if !self.can_give() {
            return GiveOutcome::Continue;
        }
        let task = self.task.text().trim().to_owned();
        let cmd = match &self.variant {
            Variant::Fresh | Variant::NextTurn { .. } => {
                let run = match self.variant {
                    Variant::Fresh => self.run,
                    _ => RunIn::Directory,
                };
                return GiveOutcome::Send(Box::new(Command::GiveWork {
                    agent: self.agent.clone(),
                    task,
                    run,
                }));
            }
            Variant::Busy => {
                return GiveOutcome::Send(Box::new(Command::SendMessage {
                    to: self.agent.clone(),
                    text: task,
                }));
            }
            Variant::Pending => return GiveOutcome::Continue,
            Variant::Role {
                role, free, pick, ..
            } => Command::AssignToRole {
                role: role.clone(),
                task,
                pick: free.get(*pick).cloned(),
                run: self.has_run_choice().then_some(self.run),
            },
            Variant::RoleNoneFree { role, .. } => Command::AssignToRole {
                role: role.clone(),
                task,
                pick: None,
                run: None,
            },
        };
        self.sent = true;
        GiveOutcome::Wait(Box::new(cmd))
    }

    /// The focus stops, in `tab` order.
    fn stops(&self) -> Vec<GiveFocus> {
        let mut out = vec![GiveFocus::Task];
        if matches!(self.variant, Variant::Role { .. }) {
            out.push(GiveFocus::Pick);
        }
        if self.has_run_choice() {
            out.push(GiveFocus::Run);
        }
        if self.variant == Variant::Busy {
            out.push(GiveFocus::SendAsMessage);
        }
        out
    }

    fn step_focus(&mut self, forward: bool) {
        let stops = self.stops();
        let at = stops.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = if forward {
            (at + 1) % stops.len()
        } else {
            (at + stops.len() - 1) % stops.len()
        };
        self.focus = stops.get(next).copied().unwrap_or(GiveFocus::Task);
    }

    /// A focus whose stop has gone moves to `Goes to` in a role's dialog, else the task.
    fn keep_focus_valid(&mut self) {
        let stops = self.stops();
        if !stops.contains(&self.focus) {
            self.focus = if stops.contains(&GiveFocus::Pick) {
                GiveFocus::Pick
            } else {
                GiveFocus::Task
            };
        }
    }

    /// `↓` the next free agent, `↑` the one before; both wrap.
    fn step_pick(&mut self, down: bool) {
        let Variant::Role { free, pick, .. } = &self.variant else {
            return;
        };
        let n = free.len();
        if n == 0 {
            return;
        }
        let next = if down {
            (pick + 1) % n
        } else {
            (pick + n - 1) % n
        };
        self.select(next);
        self.keep_focus_valid();
    }

    /// Points `Goes to` at the role's `i`th free agent; its Run row follows (P3-D29).
    fn select(&mut self, i: usize) {
        let Some(t) = self.targets.get(i).cloned() else {
            return;
        };
        if let Variant::Role { pick, .. } = &mut self.variant {
            *pick = i;
        }
        self.agent = t.agent.clone();
        self.state = t.state;
        self.since = t.since;
        self.run = t.preset();
        self.dir = t.dir;
        self.default_worktree = t.default_worktree;
        self.worktree_blocked = t.worktree_blocked;
    }

    fn task_key(&mut self, a: Action) {
        let t = &mut self.task;
        match a {
            Action::Char(c) => t.insert(c),
            Action::NewLine => t.insert('\n'),
            Action::Left => t.left(),
            Action::Right => t.right(),
            Action::Home => t.home(),
            Action::End => t.end(),
            Action::Backspace => t.backspace(),
            Action::DeleteForward => t.delete(),
            _ => return,
        }
        self.note = None;
    }

    /// `space`, `←` and `→` switch the choice; `new worktree` only where there is a repo.
    fn run_key(&mut self, a: Action) {
        let to = match a {
            Action::Char(' ') => match self.run {
                RunIn::Directory => RunIn::NewWorktree,
                RunIn::NewWorktree => RunIn::Directory,
            },
            Action::Left => RunIn::NewWorktree,
            Action::Right => RunIn::Directory,
            _ => return,
        };
        if to == RunIn::Directory || self.worktree_blocked.is_none() {
            self.run = to;
        }
    }
}

/// Free for a task to a role (TD §5.4): stopped, finished or idle, not still taking a task.
/// A worktree being made is invisible here; the engine refuses then (P3-D28).
fn is_free(view: &AgentView) -> bool {
    matches!(
        view.status.state,
        AgentState::Stopped | AgentState::Finished | AgentState::Idle
    ) && !view.delivery_pending
}

fn variant_of(view: &AgentView) -> Variant {
    match view.status.state {
        AgentState::Stopped | AgentState::Errored => Variant::Fresh,
        AgentState::Finished | AgentState::Idle if view.delivery_pending => Variant::Pending,
        AgentState::Finished | AgentState::Idle => Variant::NextTurn {
            worktree: view.worktree.clone(),
        },
        AgentState::Starting | AgentState::Working | AgentState::Waiting => Variant::Busy,
    }
}

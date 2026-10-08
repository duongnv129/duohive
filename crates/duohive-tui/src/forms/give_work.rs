//! The Give work dialog (UI §4.5, P2-D10, P2-D11, P2-D20): a task box, and for a stopped or
//! errored agent where the fresh session runs. A finished or idle agent takes it as its next
//! turn; a busy one, or one still taking the last task, can't be given work yet.

use std::path::Path;

use duohive_core::command::Command;
use duohive_core::event::AgentView;
use duohive_core::inherit::{expand_home, resolve};
use duohive_core::names::AgentName;
use duohive_core::org::Org;
use duohive_core::status::AgentState;
use duohive_core::validate::{DirProbe, DirStatus};
use duohive_core::worktree::{RunIn, TaskId};

use super::LineEdit;
use crate::input::Action;

/// The keys the dialog offers while Give work is possible.
const HINT_GIVE: &str = "enter give · alt+enter new line · esc";
const HINT_GIVE_RUN: &str = "enter give · alt+enter new line · tab run · esc";
/// The keys while it isn't.
const HINT_CLOSE: &str = "esc close";

/// What the agent's state lets the dialog do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Variant {
    /// Stopped or errored: a fresh session, run in the directory or a new worktree.
    Fresh,
    /// Finished or idle: the next turn of the session, where it already runs.
    NextTurn {
        /// The worktree it runs in, if any.
        worktree: Option<TaskId>,
    },
    /// Starting, working or waiting: refused until it is at rest (P2-D10).
    Busy,
    /// At rest, but still taking the last task (P2-D11).
    Pending,
}

/// What has the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiveFocus {
    /// The task box.
    Task,
    /// The Run choice.
    Run,
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
}

/// An open Give work dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GiveWork {
    /// Who the work goes to.
    pub agent: AgentName,
    /// Its state, for the title.
    pub state: AgentState,
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
        let resolved = resolve(org, agent).ok();
        let dir = resolved.as_ref().and_then(|r| r.directory.clone());
        let default_worktree = resolved.is_some_and(|r| r.worktree);
        let worktree_blocked = match &dir {
            Some(d) if dirs.probe(&expand_home(d, home)) == DirStatus::GitRepo => None,
            Some(d) => Some(format!("new worktree needs a git repo; {d} isn't one")),
            None => Some("new worktree needs a git repo; no directory is set".to_owned()),
        };
        let run = if default_worktree && worktree_blocked.is_none() {
            RunIn::NewWorktree
        } else {
            RunIn::Directory
        };
        Self {
            agent: agent.clone(),
            state: view.status.state,
            variant: variant_of(view),
            task: LineEdit::new(task),
            run,
            focus: GiveFocus::Task,
            dir,
            default_worktree,
            worktree_blocked,
            note: None,
        }
    }

    /// Follows the agent: its state or pending delivery changed while the dialog was open.
    pub fn sync(&mut self, view: &AgentView) {
        self.state = view.status.state;
        self.variant = variant_of(view);
        if !self.has_run_choice() {
            self.focus = GiveFocus::Task;
        }
    }

    /// The title in the top border.
    pub fn title(&self) -> String {
        format!(
            "Give work · {} · {} {}",
            self.agent,
            self.state.glyph(),
            self.state.word()
        )
    }

    /// Run is a choice only for a fresh session.
    pub fn has_run_choice(&self) -> bool {
        self.variant == Variant::Fresh
    }

    /// `enter` gives the work: the agent can take it and the task isn't empty.
    pub fn can_give(&self) -> bool {
        matches!(self.variant, Variant::Fresh | Variant::NextTurn { .. })
            && !self.task.text().trim().is_empty()
    }

    /// The keys the dialog offers now.
    pub fn hint(&self) -> &'static str {
        match (self.can_give(), self.has_run_choice()) {
            (false, _) => HINT_CLOSE,
            (true, true) => HINT_GIVE_RUN,
            (true, false) => HINT_GIVE,
        }
    }

    /// Applies a key.
    pub fn handle(&mut self, a: Action) -> GiveOutcome {
        match a {
            Action::Esc => return GiveOutcome::Close,
            Action::DialogEnter => return self.give(),
            Action::Tab | Action::BackTab => self.toggle_focus(),
            _ if self.focus == GiveFocus::Run => self.run_key(a),
            _ => self.task_key(a),
        }
        GiveOutcome::Continue
    }

    /// Pastes into the task box.
    pub fn paste(&mut self, text: &str) {
        if self.focus == GiveFocus::Task {
            self.task.insert_str(text);
        }
    }

    fn give(&self) -> GiveOutcome {
        if !self.can_give() {
            return GiveOutcome::Continue;
        }
        let run = match self.variant {
            Variant::Fresh => self.run,
            _ => RunIn::Directory,
        };
        GiveOutcome::Send(Box::new(Command::GiveWork {
            agent: self.agent.clone(),
            task: self.task.text().trim().to_owned(),
            run,
        }))
    }

    fn toggle_focus(&mut self) {
        self.focus = match (self.focus, self.has_run_choice()) {
            (GiveFocus::Task, true) => GiveFocus::Run,
            _ => GiveFocus::Task,
        };
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

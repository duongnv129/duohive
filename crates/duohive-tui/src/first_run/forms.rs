//! The field sets of the first-run wizard's step 3 (UI §4.3).

use duohive_core::cli_kind::CliKind;
use duohive_core::templates::STARTER_PROMPTS;

use crate::forms::LineEdit;

/// Agent Squad's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SquadForm {
    /// How many agents the Generalist role starts with (1–100).
    pub size: LineEdit,
    /// The directory they work in.
    pub directory: LineEdit,
    /// Their CLI.
    pub cli: CliKind,
    /// New work runs in its own worktree (on by default).
    pub worktrees: bool,
    /// The Generalist role's prompt.
    pub prompt: LineEdit,
}

impl SquadForm {
    /// Defaults: 3 agents, worktrees on, the Generalist starter prompt.
    pub fn new(cli: CliKind) -> Self {
        Self {
            size: LineEdit::new("3"),
            directory: LineEdit::default(),
            cli,
            worktrees: true,
            prompt: LineEdit::new(STARTER_PROMPTS[0].1),
        }
    }
}

/// Product Team's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductForm {
    /// The team's name.
    pub name: LineEdit,
    /// The team's CLI.
    pub cli: CliKind,
    /// Backend, frontend and QA directories.
    pub directories: [LineEdit; 3],
    /// New work runs in its own worktree (off by default).
    pub worktrees: bool,
}

impl ProductForm {
    /// Defaults: a team named `Product`, no directories yet.
    pub fn new(cli: CliKind) -> Self {
        Self {
            name: LineEdit::new("Product"),
            cli,
            directories: std::array::from_fn(|_| LineEdit::default()),
            worktrees: false,
        }
    }
}

/// One Engineering Org team row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TeamRow {
    /// The team's name.
    pub name: LineEdit,
    /// Its repository directory.
    pub repo: LineEdit,
}

/// Engineering Org's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineeringForm {
    /// Every team's CLI.
    pub cli: CliKind,
    /// The team rows; an empty row is ignored.
    pub rows: Vec<TeamRow>,
    /// New work runs in its own worktree (off by default).
    pub worktrees: bool,
}

impl EngineeringForm {
    /// Defaults: one empty row.
    pub fn new(cli: CliKind) -> Self {
        Self {
            cli,
            rows: vec![TeamRow::default()],
            worktrees: false,
        }
    }
}

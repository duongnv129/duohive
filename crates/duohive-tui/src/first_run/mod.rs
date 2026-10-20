//! The first-run wizard (UI §4.3): three steps, shown while no org exists.

pub mod forms;
mod steps;

#[cfg(test)]
mod tests;

use duohive_core::adapter::CliPresence;
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;

use crate::forms::LineEdit;

pub use forms::{EngineeringForm, ProductForm, SquadForm, TeamRow};
pub use steps::{Outcome, build_input, errors, form_error, key, map_key, paste};

/// Which step the wizard shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Your name and Duo's CLI.
    You,
    /// The template list with its preview.
    Template,
    /// The chosen template's fields.
    Form,
}

/// The template chosen on step 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateChoice {
    /// A Generalist role under Duo.
    AgentSquad,
    /// A team with Backend, Frontend and QA.
    ProductTeam,
    /// Several teams, each Backend and QA.
    EngineeringOrg,
    /// Only you and Duo.
    Blank,
}

impl TemplateChoice {
    /// The choices, in the order the list shows them.
    pub const ALL: [TemplateChoice; 4] = [
        TemplateChoice::AgentSquad,
        TemplateChoice::ProductTeam,
        TemplateChoice::EngineeringOrg,
        TemplateChoice::Blank,
    ];

    /// Its name, as the list shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::AgentSquad => "Agent Squad",
            Self::ProductTeam => "Product Team",
            Self::EngineeringOrg => "Engineering Org",
            Self::Blank => "Blank",
        }
    }

    /// Its one-line description.
    pub fn blurb(self) -> &'static str {
        match self {
            Self::AgentSquad => "a Generalist role; any free agent takes a task",
            Self::ProductTeam => "Backend · Frontend · QA, one agent each",
            Self::EngineeringOrg => "several teams, each Backend + QA",
            Self::Blank => "just you and Duo; build it yourself",
        }
    }
}

/// The wizard's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirstRun {
    /// The step shown.
    pub step: Step,
    /// Your name.
    pub you: LineEdit,
    /// Duo's CLI, and the default for every template.
    pub duo_cli: CliKind,
    /// The highlighted template.
    pub cursor: usize,
    /// The focused control: the step's fields, then its buttons.
    pub focus: usize,
    /// Agent Squad's answers.
    pub squad: SquadForm,
    /// Product Team's answers.
    pub product: ProductForm,
    /// Engineering Org's answers.
    pub engineering: EngineeringForm,
    /// `esc` on step 1 asked whether to quit.
    pub ask_quit: bool,
    /// The engine refused the last Create org.
    pub write_error: Option<String>,
    /// What CLI detection found.
    pub inventory: Vec<(CliKind, CliPresence)>,
}

impl FirstRun {
    /// The wizard for `inventory`, with `your_name` filled in.
    pub fn new(inventory: Vec<(CliKind, CliPresence)>, your_name: Option<String>) -> Self {
        let duo_cli = first_found(&inventory);
        let your_name = your_name.unwrap_or_default();
        Self {
            step: Step::You,
            you: LineEdit::new(&your_name),
            duo_cli,
            cursor: 0,
            focus: 0,
            squad: SquadForm::new(duo_cli),
            product: ProductForm::new(duo_cli),
            engineering: EngineeringForm::new(duo_cli),
            ask_quit: false,
            write_error: None,
            inventory,
        }
    }

    /// The highlighted template.
    pub fn template(&self) -> TemplateChoice {
        TemplateChoice::ALL
            .get(self.cursor)
            .copied()
            .unwrap_or(TemplateChoice::AgentSquad)
    }

    /// Whether the CLI is installed, and so pickable in a radio.
    pub fn available(&self, cli: CliKind) -> bool {
        self.inventory
            .iter()
            .any(|(k, p)| *k == cli && matches!(p, CliPresence::Found { .. }))
    }

    /// The version `cli` reported, when found.
    pub fn version(&self, cli: CliKind) -> Option<&str> {
        self.inventory.iter().find_map(|(k, p)| match p {
            CliPresence::Found { version } if *k == cli => Some(version.as_str()),
            _ => None,
        })
    }

    /// No supported CLI found: the blocking screen, not the wizard.
    pub fn blocked(&self) -> bool {
        !self.inventory.is_empty()
            && !self
                .inventory
                .iter()
                .any(|(_, p)| matches!(p, CliPresence::Found { .. }))
    }

    /// The controls before the buttons, for the current step.
    pub fn controls(&self) -> usize {
        match self.step {
            Step::You => 2,
            Step::Template => 1,
            Step::Form => match self.template() {
                TemplateChoice::AgentSquad => 5,
                TemplateChoice::ProductTeam => 6,
                TemplateChoice::EngineeringOrg => 2 * self.engineering.rows.len() + 3,
                TemplateChoice::Blank => 0,
            },
        }
    }

    /// The buttons, in focus order, for the current step.
    pub fn buttons(&self) -> &'static [&'static str] {
        match self.step {
            Step::You => &["Next →", "Quit"],
            Step::Template => &["← Back", "Next →", "Quit"],
            Step::Form => &["← Back", "Create org", "Quit"],
        }
    }

    /// The command Create org sends, when every answer is valid.
    pub fn create_command(&self) -> Option<Command> {
        let template = build_input(self)?;
        let your_name = trimmed(&self.you).map(ToOwned::to_owned);
        Some(Command::CreateOrg {
            template,
            your_name,
            duo_cli: self.duo_cli,
        })
    }
}

/// The first CLI found, else Claude Code (UI §4.3's fallback).
pub fn first_found(inventory: &[(CliKind, CliPresence)]) -> CliKind {
    CliKind::ALL
        .into_iter()
        .find(|k| {
            inventory
                .iter()
                .any(|(c, p)| c == k && matches!(p, CliPresence::Found { .. }))
        })
        .unwrap_or(CliKind::Claude)
}

/// `s` trimmed, or `None` when it is blank.
pub(crate) fn trimmed(edit: &LineEdit) -> Option<&str> {
    let text = edit.text().trim();
    (!text.is_empty()).then_some(text)
}

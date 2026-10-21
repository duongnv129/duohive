//! The tree filter by state (UI §4.1.1): `f` cycles it, `esc` in the org pane clears it.

use duohive_core::event::AgentView;
use duohive_core::org::NodeKey;
use duohive_core::status::AgentState;

use crate::model::UiModel;
use crate::tree::{TreeRowKind, tree_rows};

/// Which agents the tree keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    /// Everything.
    #[default]
    Off,
    /// Waiting, quiet or errored.
    NeedYou,
    /// Working and not quiet.
    Working,
    /// Finished.
    Finished,
}

impl Filter {
    /// The next in the cycle `◆ → ● → ✓ → off`.
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::NeedYou,
            Self::NeedYou => Self::Working,
            Self::Working => Self::Finished,
            Self::Finished => Self::Off,
        }
    }

    /// Whether the filter keeps an agent in this view.
    pub fn matches(self, v: &AgentView) -> bool {
        let s = &v.status;
        match self {
            Self::Off => true,
            Self::NeedYou => {
                s.quiet || matches!(s.state, AgentState::Waiting | AgentState::Errored)
            }
            Self::Working => s.state == AgentState::Working && !s.quiet,
            Self::Finished => s.state == AgentState::Finished,
        }
    }

    /// The glyph the header counts use for this state.
    pub fn glyph(self) -> Option<char> {
        match self {
            Self::Off => None,
            Self::NeedYou => Some('◆'),
            Self::Working => Some('●'),
            Self::Finished => Some('✓'),
        }
    }

    /// The state in words, for `no agent is <state>`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::NeedYou => "needing you",
            Self::Working => "working",
            Self::Finished => "finished",
        }
    }

    /// The header chip: `filtered: ◆ · esc clears`, or `filter ◆` when compact.
    pub fn chip(self, compact: bool) -> Option<String> {
        let glyph = self.glyph()?;
        Some(if compact {
            format!("filter {glyph}")
        } else {
            format!("filtered: {glyph} · esc clears")
        })
    }
}

/// `f`: the next filter. A selection it hides moves to the first match.
pub(crate) fn cycle(m: &mut UiModel) {
    m.filter = m.filter.next();
    keep_selection_visible(m);
}

/// `esc` in the org pane: clears the filter. `false` when there was none.
pub(crate) fn clear(m: &mut UiModel) -> bool {
    let was = m.filter != Filter::Off;
    m.filter = Filter::Off;
    was
}

/// Moves a selection the filter hides to the first agent it keeps, else to Duo.
fn keep_selection_visible(m: &mut UiModel) {
    let rows = tree_rows(m, false, false, m.now);
    if rows.iter().any(|r| r.key.as_ref() == Some(&m.selected)) {
        return;
    }
    m.selected = rows
        .iter()
        .find(|r| r.kind == TreeRowKind::Agent)
        .and_then(|r| r.key.clone())
        .unwrap_or(NodeKey::Duo);
}

#[cfg(test)]
mod tests {
    use duohive_core::status::Status;

    use super::*;
    use crate::fixtures::{status, view};

    fn v(state: AgentState, quiet: bool) -> AgentView {
        let mut s: Status = status(state, std::time::Duration::from_secs(60));
        s.quiet = quiet;
        view(s)
    }

    #[test]
    fn f_cycles_need_you_working_finished_off() {
        let seq: Vec<Filter> = std::iter::successors(Some(Filter::Off), |f| Some(f.next()))
            .take(5)
            .collect();
        assert_eq!(
            seq,
            [
                Filter::Off,
                Filter::NeedYou,
                Filter::Working,
                Filter::Finished,
                Filter::Off
            ]
        );
    }

    #[test]
    fn filter_need_you_includes_quiet_and_errored() {
        let f = Filter::NeedYou;
        assert!(f.matches(&v(AgentState::Waiting, false)));
        assert!(f.matches(&v(AgentState::Errored, false)));
        assert!(f.matches(&v(AgentState::Working, true)));
        assert!(!f.matches(&v(AgentState::Working, false)));
        assert!(!f.matches(&v(AgentState::Idle, false)));
    }

    #[test]
    fn working_excludes_quiet() {
        assert!(Filter::Working.matches(&v(AgentState::Working, false)));
        assert!(!Filter::Working.matches(&v(AgentState::Working, true)));
        assert!(Filter::Finished.matches(&v(AgentState::Finished, false)));
    }

    #[test]
    fn chip_width_rules() {
        assert_eq!(
            Filter::NeedYou.chip(false).as_deref(),
            Some("filtered: ◆ · esc clears")
        );
        assert_eq!(Filter::Working.chip(true).as_deref(), Some("filter ●"));
        assert_eq!(Filter::Off.chip(false), None);
    }
}

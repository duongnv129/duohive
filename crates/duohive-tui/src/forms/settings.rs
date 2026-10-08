//! The Settings screen's state and the pure changes its controls make (UI §4.10).

use duohive_core::settings::{Settings, Theme};

use super::LineEdit;

/// The least Quiet after allows (D19).
pub const QUIET_MINIMUM: u32 = 1;

/// A control on the Settings screen, in the order `↑↓` visits them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsRow {
    /// Your name.
    Name,
    /// Quiet after, in minutes.
    Quiet,
    /// Desktop alert: needs you.
    NeedsYou,
    /// Desktop alert: finished.
    Finished,
    /// Theme.
    Theme,
    /// Last reply under agents.
    LastReply,
    /// Reduce motion.
    ReduceMotion,
}

impl SettingsRow {
    /// Every control, top to bottom.
    pub const ALL: [Self; 7] = [
        Self::Name,
        Self::Quiet,
        Self::NeedsYou,
        Self::Finished,
        Self::Theme,
        Self::LastReply,
        Self::ReduceMotion,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|r| *r == self).unwrap_or(0)
    }

    /// The control below, or this one at the bottom.
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1).min(Self::ALL.len() - 1)]
    }

    /// The control above, or this one at the top.
    pub fn prev(self) -> Self {
        Self::ALL[self.index().saturating_sub(1)]
    }

    /// A text box, opened with `enter`.
    pub fn is_text(self) -> bool {
        matches!(self, Self::Name | Self::Quiet)
    }
}

/// Where the cursor is, and the text box being typed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsState {
    /// The focused control.
    pub row: SettingsRow,
    /// The open text box (Name or Quiet), until `enter` commits or `esc` drops it.
    pub editing: Option<LineEdit>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            row: SettingsRow::Name,
            editing: None,
        }
    }
}

/// `s` with `row` switched on or off, or `theme` stepped; a text box is not changed here.
pub fn toggled(s: &Settings, row: SettingsRow) -> Settings {
    let mut out = s.clone();
    match row {
        SettingsRow::NeedsYou => out.alerts.needs_you = !s.alerts.needs_you,
        SettingsRow::Finished => out.alerts.finished = !s.alerts.finished,
        SettingsRow::LastReply => out.show_last_reply = !s.show_last_reply,
        SettingsRow::ReduceMotion => out.reduce_motion = !s.reduce_motion,
        SettingsRow::Theme => out.theme = cycled(s.theme, true),
        SettingsRow::Name | SettingsRow::Quiet => {}
    }
    out
}

/// `auto → dark → light → auto`, or backwards.
pub fn cycled(t: Theme, forward: bool) -> Theme {
    match (t, forward) {
        (Theme::Auto, true) | (Theme::Light, false) => Theme::Dark,
        (Theme::Dark, true) | (Theme::Auto, false) => Theme::Light,
        (Theme::Light, true) | (Theme::Dark, false) => Theme::Auto,
    }
}

/// `s` with Quiet after one more or one less minute, never below the minimum.
pub fn quiet_stepped(s: &Settings, up: bool) -> Settings {
    let now = s.quiet_after_minutes.max(QUIET_MINIMUM);
    let minutes = if up {
        now.saturating_add(1)
    } else {
        now.saturating_sub(1).max(QUIET_MINIMUM)
    };
    Settings {
        quiet_after_minutes: minutes,
        ..s.clone()
    }
}

/// `s` with Quiet after read from typed text: whole minutes, at least the minimum.
pub fn quiet_typed(s: &Settings, text: &str) -> Settings {
    let minutes = text.trim().parse::<u32>().unwrap_or(0).max(QUIET_MINIMUM);
    Settings {
        quiet_after_minutes: minutes,
        ..s.clone()
    }
}

/// `s` with the name typed; blank is no name.
pub fn name_typed(s: &Settings, text: &str) -> Settings {
    let name = text.trim();
    Settings {
        your_name: (!name.is_empty()).then(|| name.to_owned()),
        ..s.clone()
    }
}

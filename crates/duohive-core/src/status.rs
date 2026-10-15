//! The per-agent status state machine (TD §4.2): pure, no I/O.

use serde::{Deserialize, Serialize};

use crate::time::Millis;

/// Where an agent is in its life (PRD, UI §2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentState {
    Stopped,
    Starting,
    Working,
    Waiting,
    Finished,
    Idle,
    Errored,
}

impl AgentState {
    /// A CLI process is alive in this state.
    pub fn is_running(self) -> bool {
        !matches!(self, Self::Stopped | Self::Errored)
    }

    /// The agent has nothing to do: finished or idle.
    pub fn is_at_rest(self) -> bool {
        matches!(self, Self::Finished | Self::Idle)
    }

    /// Free to take work (PRD "Free"): stopped, finished or idle.
    pub fn is_free(self) -> bool {
        matches!(self, Self::Stopped) || self.is_at_rest()
    }

    /// The word shown on a row.
    pub fn word(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Finished => "finished",
            Self::Idle => "idle",
            Self::Errored => "errored",
        }
    }

    /// The glyph shown beside the word.
    pub fn glyph(self) -> char {
        match self {
            Self::Stopped => '○',
            Self::Starting => '◌',
            Self::Working => '●',
            Self::Waiting => '◆',
            Self::Finished => '✓',
            Self::Idle => '◦',
            Self::Errored => '✕',
        }
    }
}

/// An agent's state with its overlays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub state: AgentState,
    /// Quiet overlay (working and starting only).
    pub quiet: bool,
    /// When the current silence began, while quiet.
    pub quiet_since: Option<Millis>,
    /// The state was read from the screen (`~`), not reported by a hook.
    pub from_screen: bool,
    /// When the agent entered `state`.
    pub since: Millis,
    /// Why the agent is errored.
    pub error: Option<String>,
    /// What a waiting agent waits on, when its screen shows a prompt DuoHive knows
    /// (P2-D23). Read from the screen, never saved: the next read restores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_on: Option<WaitingOn>,
}

/// A prompt a waiting agent's screen shows that DuoHive can say how to answer (P2-D23).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitingOn {
    /// Claude Code's folder-trust dialog, whose default choice exits.
    TrustFolder,
}

/// An event that may move an agent between states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trigger {
    Launch,
    Ready { has_task: bool },
    LaunchFailed,
    NeedsInput,
    Answered,
    TurnEnded { is_duo: bool },
    Viewed,
    NewWork,
    UnexpectedExit,
    Stop,
}

/// The state `t` moves `from` to, or `None` when the table (TD §4.2 and D2) has no such edge.
pub fn transition(from: AgentState, t: &Trigger) -> Option<AgentState> {
    use AgentState::{Errored, Finished, Idle, Starting, Stopped, Waiting, Working};
    use Trigger::{
        Answered, Launch, LaunchFailed, NeedsInput, NewWork, Ready, Stop, TurnEnded,
        UnexpectedExit, Viewed,
    };
    match (from, t) {
        (Stopped | Errored, Launch) => Some(Starting),
        (Starting, Ready { has_task: true }) => Some(Working),
        (Starting, Ready { has_task: false }) => Some(Idle),
        (Starting, LaunchFailed) => Some(Errored),
        (Working | Starting, NeedsInput) => Some(Waiting),
        (Waiting, Answered) => Some(Working),
        (Working | Waiting, TurnEnded { is_duo: false }) => Some(Finished),
        (Working | Waiting, TurnEnded { is_duo: true }) => Some(Idle),
        (Finished, Viewed) => Some(Idle),
        (Finished | Idle, NewWork) => Some(Working),
        (Starting | Working | Waiting | Finished | Idle, UnexpectedExit) => Some(Errored),
        (Starting | Working | Waiting | Finished | Idle | Errored, Stop) => Some(Stopped),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::AgentState::*;
    use super::Trigger::*;
    use super::*;

    const ALL_STATES: [AgentState; 7] =
        [Stopped, Starting, Working, Waiting, Finished, Idle, Errored];

    fn all_triggers() -> Vec<Trigger> {
        vec![
            Launch,
            Ready { has_task: true },
            Ready { has_task: false },
            LaunchFailed,
            NeedsInput,
            Answered,
            TurnEnded { is_duo: false },
            TurnEnded { is_duo: true },
            Viewed,
            NewWork,
            UnexpectedExit,
            Stop,
        ]
    }

    fn table() -> Vec<(AgentState, Trigger, AgentState)> {
        let mut t = vec![
            (Stopped, Launch, Starting),
            (Errored, Launch, Starting),
            (Starting, Ready { has_task: true }, Working),
            (Starting, Ready { has_task: false }, Idle),
            (Starting, LaunchFailed, Errored),
            (Working, NeedsInput, Waiting),
            (Starting, NeedsInput, Waiting),
            (Waiting, Answered, Working),
            (Finished, Viewed, Idle),
            (Finished, NewWork, Working),
            (Idle, NewWork, Working),
        ];
        for from in [Working, Waiting] {
            t.push((from, TurnEnded { is_duo: false }, Finished));
            t.push((from, TurnEnded { is_duo: true }, Idle));
        }
        for from in [Starting, Working, Waiting, Finished, Idle] {
            t.push((from, UnexpectedExit, Errored));
        }
        for from in [Starting, Working, Waiting, Finished, Idle, Errored] {
            t.push((from, Stop, Stopped));
        }
        t
    }

    #[test]
    fn every_row_of_the_table_moves() {
        for (f, t, to) in table() {
            assert_eq!(transition(f, &t), Some(to), "{f:?} {t:?}");
        }
    }

    #[test]
    fn nothing_else_moves() {
        let table = table();
        for f in ALL_STATES {
            for t in all_triggers() {
                if !table.iter().any(|(a, b, _)| *a == f && *b == t) {
                    assert_eq!(transition(f, &t), None, "{f:?} {t:?}");
                }
            }
        }
    }

    #[test]
    fn duo_never_finishes() {
        assert_eq!(transition(Working, &TurnEnded { is_duo: true }), Some(Idle));
        assert_eq!(transition(Waiting, &TurnEnded { is_duo: true }), Some(Idle));
    }

    #[test]
    fn missing_cli_goes_through_starting() {
        assert_eq!(transition(Stopped, &LaunchFailed), None);
        assert_eq!(transition(Stopped, &Launch), Some(Starting));
        assert_eq!(transition(Starting, &LaunchFailed), Some(Errored));
    }

    #[test]
    fn words_and_glyphs_match_ui_2_1() {
        let expected = [
            (Stopped, "stopped", '○'),
            (Starting, "starting", '◌'),
            (Working, "working", '●'),
            (Waiting, "waiting", '◆'),
            (Finished, "finished", '✓'),
            (Idle, "idle", '◦'),
            (Errored, "errored", '✕'),
        ];
        for (s, w, g) in expected {
            assert_eq!(s.word(), w);
            assert_eq!(s.glyph(), g);
        }
    }

    #[test]
    fn free_means_stopped_finished_idle() {
        let free: Vec<_> = ALL_STATES.into_iter().filter(|s| s.is_free()).collect();
        assert_eq!(free, [Stopped, Finished, Idle]);
    }

    #[test]
    fn running_and_at_rest_sets() {
        let running: Vec<_> = ALL_STATES.into_iter().filter(|s| s.is_running()).collect();
        assert_eq!(running, [Starting, Working, Waiting, Finished, Idle]);
        let rest: Vec<_> = ALL_STATES.into_iter().filter(|s| s.is_at_rest()).collect();
        assert_eq!(rest, [Finished, Idle]);
    }
}

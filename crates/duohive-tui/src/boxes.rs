//! The org pane as grouped boxes (UI §4.1.5): the pure list of lines. Drawing, hit-testing,
//! scrolling and the wheel all read this one list (OB-D1), so they cannot disagree about
//! heights. Everything is laid out against fixed widths: the pane is 50 columns whenever the
//! boxes are drawn.

use duohive_core::colour::TeamColour;
use duohive_core::event::AgentView;
use duohive_core::names::NodeName;
use duohive_core::org::{NodeKey, Role, RoleKey, Team};
use duohive_core::status::{AgentState, Status};
use duohive_core::time::Millis;

use crate::format::{
    age_of, format_age, one_line, quoted, status_glyph, status_word, truncate_end, truncate_middle,
    width, wrap_capped,
};
use crate::model::UiModel;
use crate::tree::{filter_keeps_nothing, roll_up, visible};

/// A box at the root of the pane, borders included.
pub const ROOT_BOX: usize = 46;
/// A role box inside a team box, borders included.
pub const TEAM_BOX: usize = 42;
/// The columns Duo's name has on the Manager card, whatever the status.
pub const DUO_NAME: usize = 16;
/// The columns line 2 of the Manager card has (one stays free at the right edge).
pub const DUO_LINE_2: usize = 27;
/// A problem wraps onto at most this many lines (UI §4.12).
const MAX_PROBLEM_LINES: usize = 3;
/// The hint every problem line ends with: `e` edits the agent.
const EDIT_HINT: &str = " (e)";
const EMPTY_ROLE: &str = "no agents yet · n to add one or ask Duo";
const SEP: &str = " · ";

/// What a line of the pane is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxLineKind {
    /// The card's top border: `⬢ Manager`.
    ManagerTop,
    /// Duo's name and its status pill.
    ManagerName,
    /// What needs you, or Duo's short error reason.
    ManagerCounts,
    /// The card's bottom border.
    ManagerBottom,
    /// A team box's top border: its name and roll-up.
    TeamTop,
    /// A role box's top border; a top-level role's carries its roll-up.
    RoleTop,
    /// An agent row.
    Agent,
    /// The second line of an agent: its state, or its problem.
    Detail,
    /// A role box's bottom border.
    RoleBottom,
    /// A team box's bottom border.
    TeamBottom,
    /// The hint inside a role with no agents.
    EmptyRole,
    /// The line when the filter keeps no agent.
    FilterEmpty,
}

/// How a [`BoxLineKind::Detail`] line is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// The state line (and every line that is not a detail).
    Plain,
    /// An error or a configuration problem: `status.error` unless selected.
    Problem,
    /// A notice: `fg.muted` unless selected.
    Notice,
}

/// One line of the pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxLine {
    /// What the line is.
    pub kind: BoxLineKind,
    /// The node it belongs to; a click on the line selects it.
    pub key: Option<NodeKey>,
    /// The box's hue; `None` for the Manager card, which has its own colour.
    pub hue: Option<TeamColour>,
    /// The line belongs to the selected node.
    pub selected: bool,
    /// The box is folded to its title line.
    pub folded: bool,
    /// The line is inside a team box, so its box is [`TEAM_BOX`] wide, not [`ROOT_BOX`].
    pub in_team: bool,
    /// The text: a name, a title, a pill's neighbour, or a detail.
    pub label: String,
    /// The text at the right edge: a roll-up, a state icon or Duo's status pill.
    pub right: String,
    /// Messages waiting for the row's agent; 0 elsewhere.
    pub inbox: u32,
    /// How a detail line is coloured.
    pub tone: Tone,
}

impl BoxLine {
    fn new(kind: BoxLineKind, key: Option<NodeKey>, label: impl Into<String>) -> Self {
        Self {
            kind,
            key,
            hue: None,
            selected: false,
            folded: false,
            in_team: false,
            label: label.into(),
            right: String::new(),
            inbox: 0,
            tone: Tone::Plain,
        }
    }
}

/// The columns an agent's name may take in a box, so its row keeps a marker column, the
/// `✉ n`, a gap, the icon (two columns with a `~`) and a pad before the border.
pub const fn name_room_base(in_team: bool) -> usize {
    box_width(in_team) - 2 - (1 + 1 + 2 + 1)
}

/// [`name_room_base`] less the envelope for `inbox` waiting messages.
pub fn name_room(in_team: bool, inbox: u32) -> usize {
    name_room_base(in_team).saturating_sub(envelope_width(inbox))
}

/// The columns of ` ✉ n` after a name; none with nothing waiting.
pub fn envelope_width(inbox: u32) -> usize {
    if inbox == 0 {
        0
    } else {
        3 + inbox.to_string().len()
    }
}

/// The columns a detail line's text has: the box less its borders and indent.
pub const fn detail_width(in_team: bool) -> usize {
    box_width(in_team) - 4
}

const fn box_width(in_team: bool) -> usize {
    if in_team { TEAM_BOX } else { ROOT_BOX }
}

/// Every line of the pane, top to bottom. `replies` is whether the last reply shows on a
/// selected agent's second line.
pub fn box_rows(m: &UiModel, now: Millis, replies: bool) -> Vec<BoxLine> {
    let mut out = Vec::new();
    manager(m, &mut out);
    let visible = visible(m);
    if filter_keeps_nothing(m, &visible.teams, &visible.roots) {
        let label = format!("no agent is {} · esc clears the filter", m.filter.word());
        out.push(BoxLine::new(BoxLineKind::FilterEmpty, None, label));
    }
    let cx = Cx { m, now, replies };
    for (team, roles) in &visible.teams {
        cx.team(team, roles, &mut out);
    }
    for role in &visible.roots {
        let key = RoleKey {
            team: None,
            role: role.name.clone(),
        };
        cx.role(role, key, &mut out);
    }
    out
}

/// The Manager card: four lines, whatever the name or the state.
fn manager(m: &UiModel, out: &mut Vec<BoxLine>) {
    let duo = &m.org.duo.name;
    let status = &m.view(duo).status;
    let selected = m.selected == NodeKey::Duo;
    let line = |kind, label: String, right: String| BoxLine {
        selected,
        right,
        ..BoxLine::new(kind, Some(NodeKey::Duo), label)
    };
    out.push(line(
        BoxLineKind::ManagerTop,
        "⬢ Manager".to_owned(),
        String::new(),
    ));
    out.push(line(
        BoxLineKind::ManagerName,
        truncate_middle(duo.as_str(), DUO_NAME),
        pill(status),
    ));
    out.push(line(
        BoxLineKind::ManagerCounts,
        counts_line(m, status),
        String::new(),
    ));
    out.push(line(
        BoxLineKind::ManagerBottom,
        String::new(),
        String::new(),
    ));
}

/// Duo's status pill text: its icon and word. Duo never shows `finished`: it goes straight
/// to idle.
fn pill(s: &Status) -> String {
    if s.quiet {
        return format!("{} {}", status_glyph(s, None), status_word(s));
    }
    let state = if s.state == AgentState::Finished {
        AgentState::Idle
    } else {
        s.state
    };
    format!("{} {}", state.glyph(), state.word())
}

/// Line 2 of the card: what needs you, or, when Duo is errored, its short reason.
fn counts_line(m: &UiModel, s: &Status) -> String {
    if let (AgentState::Errored, Some(reason)) = (s.state, s.error.as_deref()) {
        return truncate_end(short_reason(reason), DUO_LINE_2);
    }
    let need = m.needs_you.len();
    if need == 0 {
        return "nothing needs you".to_owned();
    }
    let cards = m.needs_you.iter().filter(|i| i.card.is_some()).count();
    match cards {
        0 => format!("{need} need you"),
        1 => format!("{need} need you · 1 card"),
        n => format!("{need} need you · {n} cards"),
    }
}

/// The reason up to its first ` (` or ` · `.
fn short_reason(reason: &str) -> &str {
    let cut = [" (", SEP]
        .iter()
        .filter_map(|d| reason.find(d))
        .min()
        .unwrap_or(reason.len());
    &reason[..cut]
}

struct Cx<'a> {
    m: &'a UiModel,
    now: Millis,
    replies: bool,
}

impl Cx<'_> {
    fn team(&self, team: &Team, roles: &[&Role], out: &mut Vec<BoxLine>) {
        let key = NodeKey::Team(team.name.clone());
        let hue = self.m.org.colour_of(&key);
        let selected = self.m.selected == key;
        let folded = self.m.folded.contains(&key);
        let agents: Vec<&AgentView> = team
            .roles
            .iter()
            .flat_map(|r| r.agents.iter().map(|a| self.m.view(&a.name)))
            .collect();
        let line = |kind, label: String, right: String| BoxLine {
            hue,
            selected,
            folded,
            right,
            ..BoxLine::new(kind, Some(key.clone()), label)
        };
        out.push(line(
            BoxLineKind::TeamTop,
            group_label("▣ ", &team.name),
            roll_up(&agents).trim_start().to_owned(),
        ));
        if folded {
            return;
        }
        for role in roles {
            let rkey = RoleKey {
                team: Some(team.name.clone()),
                role: role.name.clone(),
            };
            self.role(role, rkey, out);
        }
        out.push(line(BoxLineKind::TeamBottom, String::new(), String::new()));
    }

    fn role(&self, role: &Role, key: RoleKey, out: &mut Vec<BoxLine>) {
        let in_team = key.team.is_some();
        let node = NodeKey::Role(key);
        let hue = self.m.org.colour_of(&node);
        let selected = self.m.selected == node;
        let folded = self.m.folded.contains(&node);
        let roll = if in_team {
            String::new()
        } else {
            let agents: Vec<&AgentView> =
                role.agents.iter().map(|a| self.m.view(&a.name)).collect();
            roll_up(&agents).trim_start().to_owned()
        };
        let line = |kind, label: String, right: String| BoxLine {
            hue,
            selected,
            folded,
            in_team,
            right,
            ..BoxLine::new(kind, Some(node.clone()), label)
        };
        out.push(line(
            BoxLineKind::RoleTop,
            group_label("◇ ", &role.name),
            roll,
        ));
        if folded {
            return;
        }
        if role.agents.is_empty() {
            let hint = BoxLine {
                hue,
                in_team,
                ..BoxLine::new(BoxLineKind::EmptyRole, Some(node.clone()), EMPTY_ROLE)
            };
            out.push(hint);
        }
        for agent in &role.agents {
            if self.m.filter.matches(self.m.view(&agent.name)) {
                self.agent(&agent.name, (hue, in_team), out);
            }
        }
        out.push(line(BoxLineKind::RoleBottom, String::new(), String::new()));
    }

    fn agent(
        &self,
        name: &duohive_core::names::AgentName,
        (hue, in_team): (Option<TeamColour>, bool),
        out: &mut Vec<BoxLine>,
    ) {
        let key = NodeKey::Agent(name.clone());
        let view = self.m.view(name);
        let selected = self.m.selected == key;
        let s = &view.status;
        let icon = format!(
            "{}{}",
            if s.from_screen { "~" } else { "" },
            status_glyph(s, None)
        );
        out.push(BoxLine {
            hue,
            selected,
            in_team,
            right: icon,
            inbox: view.inbox,
            ..BoxLine::new(
                BoxLineKind::Agent,
                Some(key.clone()),
                truncate_middle(name.as_str(), name_room(in_team, view.inbox)),
            )
        });
        for (tone, text) in self.details(view, selected, in_team) {
            out.push(BoxLine {
                hue,
                selected,
                in_team,
                tone,
                ..BoxLine::new(BoxLineKind::Detail, Some(key.clone()), text)
            });
        }
    }

    /// The agent's second line. There is only ever one entry, which may wrap onto up to three
    /// lines: a problem (shown selected or not) replaces the state line; a notice follows the
    /// state line of a selected agent and stands alone under one that is not.
    fn details(&self, view: &AgentView, selected: bool, in_team: bool) -> Vec<(Tone, String)> {
        let room = detail_width(in_team);
        if let Some(problem) = problem_text(&view.status, view) {
            return wrap_capped(&problem, room, MAX_PROBLEM_LINES)
                .into_iter()
                .map(|l| (Tone::Problem, l))
                .collect();
        }
        if !selected {
            return view
                .notice
                .iter()
                .map(|n| (Tone::Notice, truncate_end(n, room)))
                .collect();
        }
        vec![(Tone::Plain, self.state_line(view, room))]
    }

    /// `working 2m · "Running go test ./..."`: the word, how long, the last reply (while the
    /// setting is on and the agent is not stopped) and a notice, cut at the box's edge.
    fn state_line(&self, view: &AgentView, room: usize) -> String {
        let s = &view.status;
        let age = age_of(s, self.now).map(format_age);
        let mut text = match age {
            Some(age) => format!("{} {age}", status_word(s)),
            None => status_word(s).to_owned(),
        };
        let notice = view
            .notice
            .as_deref()
            .map(|n| format!("{SEP}{n}"))
            .unwrap_or_default();
        let reply = view
            .last_reply
            .as_deref()
            .filter(|_| self.replies && s.state != AgentState::Stopped);
        if let Some(reply) = reply {
            let left = room.saturating_sub(width(&text) + width(SEP) + width(&notice));
            if left >= 3 {
                text.push_str(SEP);
                text.push_str(&quoted(&one_line(reply), left));
            }
        }
        text.push_str(&notice);
        truncate_end(&text, room)
    }
}

/// `▣ Payments`, the name uncut: the painter cuts it beside the roll-up and the fold marker.
fn group_label(glyph: &str, name: &NodeName) -> String {
    format!("{glyph}{name}")
}

/// The problem line for an agent: an error with its reason, or a configuration problem, each
/// ending with the `(e)` hint, or `None` when the agent has neither.
fn problem_text(s: &Status, view: &AgentView) -> Option<String> {
    if s.state == AgentState::Errored {
        return Some(match s.error.as_deref() {
            Some(reason) => format!("errored{SEP}{reason}{EDIT_HINT}"),
            None => format!("errored{EDIT_HINT}"),
        });
    }
    view.problem
        .as_deref()
        .map(|p| p.strip_prefix("✕ ").unwrap_or(p).to_owned())
}

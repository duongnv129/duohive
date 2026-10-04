//! The org tree as rows (UI §2.2, §4.1): connectors, labels, state cells and roll-ups.

use duohive_core::event::AgentView;
use duohive_core::org::{NodeKey, Role, RoleKey};
use duohive_core::status::{AgentState, Status};
use duohive_core::time::Millis;

use crate::format::{compact_cell, one_line, state_cell};
use crate::model::UiModel;

const TEE: &str = "├─ ";
const ELBOW: &str = "└─ ";
const PIPE: &str = "│  ";
const GAP: &str = "   ";
const OPEN: &str = "▾ ";
const EMPTY_ROLE: &str = "no agents yet · n to add one";
/// Hint after an error on Duo's row (D17): Duo's CLI is set in Edit org.
const DUO_ERROR_HINT: &str = " · e Edit org";
/// Hint after an error on an agent's row (D16).
const AGENT_ERROR_HINT: &str = " · s stop";
const FIX_HINT: &str = " · e to fix";
const HINT_SEP: &str = " · ";

/// What a tree row shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeRowKind {
    /// Duo, at the root.
    Duo,
    /// A team.
    Team,
    /// A role.
    Role,
    /// An agent.
    Agent,
    /// The line under an agent: its last reply, or a notice.
    Reply,
    /// The line under an agent: its error or configuration problem.
    Problem,
    /// The line under a role with no agents.
    EmptyRole,
    /// An org with no teams or roles (the message itself is in the side pane).
    EmptyOrg,
}

/// One line of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// What the row shows.
    pub kind: TreeRowKind,
    /// The node, on node rows only (detail lines can't be selected).
    pub key: Option<NodeKey>,
    /// Connectors or indent before the label.
    pub prefix: String,
    /// The label, uncut.
    pub label: String,
    /// The state cell or roll-up, aligned in the right column.
    pub right: String,
}

impl TreeRow {
    fn node(kind: TreeRowKind, key: NodeKey, prefix: String, label: String, right: String) -> Self {
        Self {
            kind,
            key: Some(key),
            prefix,
            label,
            right,
        }
    }

    fn detail(kind: TreeRowKind, prefix: String, label: String) -> Self {
        Self {
            kind,
            key: None,
            prefix,
            label,
            right: String::new(),
        }
    }
}

/// The state cell for an agent row: 16 cells wide, or 7 in the compact layout.
pub fn agent_right(s: &Status, now: Millis, compact: bool, spinner: Option<usize>) -> String {
    if compact {
        compact_cell(s, now, spinner)
    } else {
        state_cell(s, now, spinner)
    }
}

struct Ctx<'a> {
    m: &'a UiModel,
    compact: bool,
    show_replies: bool,
    now: Millis,
    rows: Vec<TreeRow>,
}

/// Every row of the tree, top to bottom.
pub fn tree_rows(m: &UiModel, compact: bool, show_replies: bool, now: Millis) -> Vec<TreeRow> {
    let mut c = Ctx {
        m,
        compact,
        show_replies,
        now,
        rows: Vec::new(),
    };
    c.duo();
    let org = &m.org;
    let children = org.teams.len() + org.roles.len();
    for (i, team) in org.teams.iter().enumerate() {
        let last = i + 1 == children;
        let (prefix, cont) = c.branch("", last, "");
        let agents: Vec<&AgentView> = team.roles.iter().flat_map(|r| c.views(r)).collect();
        let label = if compact { OPEN } else { "▣ " };
        c.rows.push(TreeRow::node(
            TreeRowKind::Team,
            NodeKey::Team(team.name.clone()),
            prefix,
            format!("{label}{}", team.name),
            roll_up(&agents),
        ));
        for (j, role) in team.roles.iter().enumerate() {
            let key = RoleKey {
                team: Some(team.name.clone()),
                role: role.name.clone(),
            };
            c.role(role, key, &cont, j + 1 == team.roles.len(), "  ");
        }
    }
    for (i, role) in org.roles.iter().enumerate() {
        let key = RoleKey {
            team: None,
            role: role.name.clone(),
        };
        c.role(role, key, "", org.teams.len() + i + 1 == children, "");
    }
    c.rows
}

impl Ctx<'_> {
    /// The prefix for a node and the continuation for its children: connectors when wide,
    /// `indent` when compact.
    fn branch(&self, parent: &str, last: bool, indent: &str) -> (String, String) {
        if self.compact {
            return (indent.to_owned(), indent.to_owned());
        }
        let (conn, cont) = if last { (ELBOW, GAP) } else { (TEE, PIPE) };
        (format!("{parent}{conn}"), format!("{parent}{cont}"))
    }

    /// The prefix for a node's detail line: one cell past its name when wide.
    fn detail_prefix(&self, parent: &str, last: bool, compact_indent: &str) -> String {
        if self.compact {
            return compact_indent.to_owned();
        }
        let cont = if last { "    " } else { "│   " };
        format!("{parent}{cont}")
    }

    fn views(&self, role: &Role) -> Vec<&AgentView> {
        role.agents
            .iter()
            .filter_map(|a| self.m.agents.get(&a.name))
            .collect()
    }

    fn duo(&mut self) {
        let m = self.m;
        let duo = &m.org.duo;
        let view = m.view(&duo.name);
        let label = if self.compact {
            format!("⬢ {}", duo.name)
        } else {
            format!("⬢ {} · your manager agent", duo.name)
        };
        let right = agent_right(&view.status, self.now, self.compact, None);
        self.rows.push(TreeRow::node(
            TreeRowKind::Duo,
            NodeKey::Duo,
            String::new(),
            label,
            right,
        ));
        self.detail(view, true, "    ".to_owned());
    }

    fn role(&mut self, role: &Role, key: RoleKey, parent: &str, last: bool, indent: &str) {
        let (prefix, cont) = self.branch(parent, last, indent);
        let label = if self.compact { OPEN } else { "◇ " };
        let right = roll_up(&self.views(role));
        self.rows.push(TreeRow::node(
            TreeRowKind::Role,
            NodeKey::Role(key),
            prefix,
            format!("{label}{}", role.name),
            right,
        ));
        if role.agents.is_empty() {
            let prefix = self.detail_prefix(parent, last, "    ");
            let row = TreeRow::detail(TreeRowKind::EmptyRole, prefix, EMPTY_ROLE.to_owned());
            self.rows.push(row);
        }
        for (k, agent) in role.agents.iter().enumerate() {
            let last = k + 1 == role.agents.len();
            let (prefix, _) = self.branch(&cont, last, "    ");
            let m = self.m;
            let view = m.view(&agent.name);
            let right = agent_right(&view.status, self.now, self.compact, None);
            self.rows.push(TreeRow::node(
                TreeRowKind::Agent,
                NodeKey::Agent(agent.name.clone()),
                prefix,
                agent.name.to_string(),
                right,
            ));
            let detail = self.detail_prefix(&cont, last, "      ");
            self.detail(view, false, detail);
        }
    }

    /// The line under a Duo or agent row, if any: error, problem, notice, else last reply.
    fn detail(&mut self, view: &AgentView, is_duo: bool, prefix: String) {
        let s = &view.status;
        let error = s
            .error
            .as_deref()
            .filter(|_| s.state == AgentState::Errored);
        let line = if let Some(e) = error {
            let hint = if is_duo {
                DUO_ERROR_HINT
            } else {
                AGENT_ERROR_HINT
            };
            Some((TreeRowKind::Problem, with_hint(e, hint)))
        } else if let Some(p) = &view.problem {
            let p = if p.starts_with('✕') {
                p.clone()
            } else {
                format!("✕ {p}")
            };
            Some((TreeRowKind::Problem, with_hint(&p, FIX_HINT)))
        } else if let Some(n) = &view.notice {
            Some((TreeRowKind::Reply, n.clone()))
        } else {
            view.last_reply
                .as_deref()
                .filter(|_| self.show_replies && !self.compact)
                .map(|r| (TreeRowKind::Reply, format!("\"{}\"", one_line(r))))
        };
        if let Some((kind, label)) = line {
            self.rows.push(TreeRow::detail(kind, prefix, label));
        }
    }
}

/// `text` with `hint` appended, unless it already carries one.
fn with_hint(text: &str, hint: &str) -> String {
    if text.contains(HINT_SEP) {
        text.to_owned()
    } else {
        format!("{text}{hint}")
    }
}

/// The roll-up (D12): `● n`, `◆ n`, `◷ n`, `✕ n`, non-zero only, after `~` or a space.
pub fn roll_up(agents: &[&AgentView]) -> String {
    let counts = roll_up_counts(agents);
    let parts: Vec<String> = counts
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(g, n)| format!("{g} {n}"))
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    let screen = agents
        .iter()
        .any(|v| v.status.from_screen && rolled_up(&v.status).is_some());
    format!("{}{}", if screen { '~' } else { ' ' }, parts.join(" "))
}

/// Working, waiting, quiet and errored counts, in roll-up order.
pub fn roll_up_counts(agents: &[&AgentView]) -> [(char, usize); 4] {
    let mut counts = [('●', 0), ('◆', 0), ('◷', 0), ('✕', 0)];
    for v in agents {
        if let Some(i) = rolled_up(&v.status) {
            counts[i].1 += 1;
        }
    }
    counts
}

/// Which roll-up count a status adds to, if any.
fn rolled_up(s: &Status) -> Option<usize> {
    match (s.quiet, s.state) {
        (true, _) => Some(2),
        (false, AgentState::Working) => Some(0),
        (false, AgentState::Waiting) => Some(1),
        (false, AgentState::Errored) => Some(3),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use duohive_core::org::NodeKey;
    use duohive_core::testkit::n;

    use super::*;
    use crate::fixtures::{NOW, spec_model};

    #[test]
    fn wide_rows_carry_keys_prefixes_and_roll_ups() {
        let m = spec_model();
        let rows = tree_rows(&m, false, true, NOW);
        let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
        use TreeRowKind::*;
        assert_eq!(
            kinds,
            [
                Duo, Team, Role, Agent, Reply, Agent, Reply, Role, Agent, Role, Agent, Agent, Agent
            ]
        );
        assert_eq!(rows[3].key, Some(NodeKey::Agent(n("auth-service"))));
        assert_eq!(rows[3].prefix, "│  │  ├─ ");
        assert_eq!(rows[4].key, None);
        assert_eq!(rows[4].label, "\"Running go test ./...\"");
        assert_eq!(rows[1].right, " ● 1 ◆ 1");
        assert_eq!(rows[9].right, "~● 1 ◷ 1");
        assert_eq!(rows[7].right, "");
    }

    #[test]
    fn replies_hidden_when_off_or_compact() {
        let m = spec_model();
        for (compact, show) in [(false, false), (true, true)] {
            let rows = tree_rows(&m, compact, show, NOW);
            assert!(rows.iter().all(|r| r.kind != TreeRowKind::Reply));
        }
        let compact = tree_rows(&m, true, true, NOW);
        assert_eq!(compact[3].prefix, "    ");
        assert_eq!(compact[3].right, " ●   2m");
        assert_eq!(compact[1].label, "▾ Payments");
    }
}

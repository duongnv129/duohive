//! Activity by `poll` (spike S0.5): one `list-windows` a second, diffed against the last reply.

use std::collections::HashMap;

use duohive_core::launch::WindowId;
use duohive_core::names::AgentName;
use duohive_core::ports::{LiveWindow, RuntimeEvent};

/// The poll command. Double quotes on purpose: tmux turns `\t` into a tab only there (S0.5).
pub(crate) fn poll_command(session: &str) -> String {
    format!(
        "list-windows -t {} -F \"#{{window_id}}\\t#{{window_activity}}\\t#{{pane_dead}}\\t#{{pane_dead_status}}\"",
        session_target(session)
    )
}

/// The inventory command; same quoting as the poll.
pub(crate) fn inventory_command(session: &str) -> String {
    format!(
        "list-windows -t {} -F \"#{{window_id}}\\t#{{@duohive_agent}}\\t#{{pane_dead}}\\t#{{pane_dead_status}}\\t#{{window_activity}}\"",
        session_target(session)
    )
}

/// `=<session>`, single-quoted (session names are plain identifiers, D31).
fn session_target(session: &str) -> String {
    format!("'={}'", session.replace('\'', r"'\''"))
}

/// One poll reply line: `@id \t activity \t dead \t status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PollRow {
    pub window: String,
    pub activity: u64,
    pub dead: bool,
    pub status: Option<i32>,
}

/// Parses a poll line; `None` for anything malformed.
pub(crate) fn parse_poll_line(line: &str) -> Option<PollRow> {
    let mut f = line.split('\t');
    let window = f.next().filter(|w| w.starts_with('@'))?.to_owned();
    let activity = f.next()?.parse().ok()?;
    let dead = f.next()? == "1";
    let status = f.next()?.parse().ok();
    Some(PollRow {
        window,
        activity,
        dead,
        status,
    })
}

/// Parses an inventory line; `None` for untagged (the keeper) or malformed lines.
pub(crate) fn parse_inventory_line(line: &str) -> Option<LiveWindow> {
    let mut f = line.split('\t');
    let window = f.next().filter(|w| w.starts_with('@'))?;
    let tag = f.next()?;
    if tag.is_empty() {
        return None;
    }
    let Ok(agent) = AgentName::parse(tag) else {
        tracing::warn!(
            window,
            tag,
            "window tagged with an invalid agent name; skipped"
        );
        return None;
    };
    let dead = f.next()? == "1";
    let status = f.next()?.parse().ok();
    let activity_secs = f.next()?.parse().unwrap_or(0);
    Some(LiveWindow {
        agent,
        window: WindowId(window.to_owned()),
        dead: dead.then_some(status),
        activity_secs,
    })
}

#[derive(Debug, Clone)]
struct Entry {
    agent: AgentName,
    /// Order of arrival: an agent's newest window has the highest.
    seq: u64,
    activity: Option<u64>,
    dead: bool,
}

/// The runtime's windows. An entry lives until its window is reported gone.
#[derive(Debug, Default)]
pub(crate) struct Table {
    windows: HashMap<String, Entry>,
    next_seq: u64,
}

impl Table {
    fn push(&mut self, window: &str, entry: impl FnOnce(u64) -> Entry) {
        if !self.windows.contains_key(window) {
            self.next_seq += 1;
            self.windows.insert(window.to_owned(), entry(self.next_seq));
        }
    }

    /// A window this runtime started: alive, activity not yet seen.
    pub fn insert_started(&mut self, window: &WindowId, agent: &AgentName) {
        self.push(&window.0, |seq| Entry {
            agent: agent.clone(),
            seq,
            activity: None,
            dead: false,
        });
    }

    /// A window found by an inventory, already reported in that state.
    pub fn insert_restored(&mut self, live: &LiveWindow) {
        self.push(&live.window.0, |seq| Entry {
            agent: live.agent.clone(),
            seq,
            activity: Some(live.activity_secs),
            dead: live.dead.is_some(),
        });
    }

    /// The agent's newest window.
    pub fn current(&self, agent: &AgentName) -> Option<WindowId> {
        self.windows
            .iter()
            .filter(|(_, e)| e.agent == *agent)
            .max_by_key(|(_, e)| e.seq)
            .map(|(w, _)| WindowId(w.clone()))
    }

    pub fn contains(&self, window: &WindowId) -> bool {
        self.windows.contains_key(&window.0)
    }

    /// The window closed: `WindowGone` once, then forgotten.
    pub fn gone(&mut self, window: &str) -> Option<RuntimeEvent> {
        self.windows
            .remove(window)
            .map(|e| RuntimeEvent::WindowGone {
                agent: e.agent,
                window: WindowId(window.to_owned()),
            })
    }

    /// Diffs a full poll reply against the table.
    pub fn diff(&mut self, rows: &[PollRow]) -> Vec<RuntimeEvent> {
        let by_id: HashMap<&str, &PollRow> = rows.iter().map(|r| (r.window.as_str(), r)).collect();
        let mut ids: Vec<(u64, String)> = self
            .windows
            .iter()
            .map(|(w, e)| (e.seq, w.clone()))
            .collect();
        ids.sort();
        let mut out = Vec::new();
        for (_, id) in ids {
            let Some(row) = by_id.get(id.as_str()) else {
                out.extend(self.gone(&id));
                continue;
            };
            let Some(entry) = self.windows.get_mut(&id) else {
                continue;
            };
            if entry.activity != Some(row.activity) {
                entry.activity = Some(row.activity);
                out.push(RuntimeEvent::Activity {
                    agent: entry.agent.clone(),
                    at_secs: row.activity,
                });
            }
            if row.dead && !entry.dead {
                entry.dead = true;
                out.push(RuntimeEvent::PaneDied {
                    agent: entry.agent.clone(),
                    window: WindowId(id.clone()),
                    status: row.status,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> AgentName {
        AgentName::parse(s).unwrap()
    }

    fn w(s: &str) -> WindowId {
        WindowId(s.into())
    }

    fn row(window: &str, activity: u64, dead: bool, status: Option<i32>) -> PollRow {
        PollRow {
            window: window.into(),
            activity,
            dead,
            status,
        }
    }

    #[test]
    fn commands_use_double_quoted_tabs() {
        assert_eq!(
            poll_command("duohive"),
            r##"list-windows -t '=duohive' -F "#{window_id}\t#{window_activity}\t#{pane_dead}\t#{pane_dead_status}""##
        );
        assert_eq!(
            inventory_command("duohive"),
            r##"list-windows -t '=duohive' -F "#{window_id}\t#{@duohive_agent}\t#{pane_dead}\t#{pane_dead_status}\t#{window_activity}""##
        );
    }

    #[test]
    fn poll_lines_parse_with_an_empty_last_field() {
        assert_eq!(
            parse_poll_line("@1\t100\t0\t"),
            Some(row("@1", 100, false, None))
        );
        assert_eq!(
            parse_poll_line("@2\t5\t1\t3"),
            Some(row("@2", 5, true, Some(3)))
        );
        assert_eq!(
            parse_poll_line("@3\t5\t1\t"),
            Some(row("@3", 5, true, None))
        );
        assert_eq!(parse_poll_line("garbage"), None);
        assert_eq!(parse_poll_line("@1 100 0 "), None);
    }

    #[test]
    fn inventory_lines_skip_untagged_and_invalid() {
        assert_eq!(parse_inventory_line("@0\t\t0\t\t9"), None);
        assert_eq!(parse_inventory_line("@1\tbad name\t0\t\t9"), None);
        assert_eq!(
            parse_inventory_line("@2\tb\t1\t2\t9"),
            Some(LiveWindow {
                agent: n("b"),
                window: w("@2"),
                dead: Some(Some(2)),
                activity_secs: 9
            })
        );
        assert_eq!(
            parse_inventory_line("@3\ta\t0\t\t7").map(|l| l.dead),
            Some(None)
        );
    }

    #[test]
    fn diff_reports_activity_death_and_disappearance_once() {
        let mut t = Table::default();
        t.insert_started(&w("@1"), &n("a"));
        t.insert_started(&w("@2"), &n("b"));
        let ev = t.diff(&[row("@1", 10, false, None), row("@2", 10, false, None)]);
        assert_eq!(ev.len(), 2);
        assert!(
            t.diff(&[row("@1", 10, false, None), row("@2", 10, false, None)])
                .is_empty()
        );
        assert_eq!(
            t.diff(&[row("@1", 10, true, Some(3))]),
            vec![
                RuntimeEvent::PaneDied {
                    agent: n("a"),
                    window: w("@1"),
                    status: Some(3)
                },
                RuntimeEvent::WindowGone {
                    agent: n("b"),
                    window: w("@2")
                },
            ]
        );
        assert!(t.diff(&[row("@1", 10, true, Some(3))]).is_empty());
        assert_eq!(t.gone("@2"), None);
    }

    #[test]
    fn untracked_windows_are_ignored_and_restored_ones_start_as_reported() {
        let mut t = Table::default();
        assert!(t.diff(&[row("@0", 1, false, None)]).is_empty());
        t.insert_restored(&LiveWindow {
            agent: n("a"),
            window: w("@4"),
            dead: Some(Some(1)),
            activity_secs: 50,
        });
        assert!(t.diff(&[row("@4", 50, true, Some(1))]).is_empty());
    }

    #[test]
    fn current_is_the_newest_window_and_old_ones_live_on() {
        let mut t = Table::default();
        t.insert_started(&w("@1"), &n("a"));
        t.insert_started(&w("@2"), &n("a"));
        assert_eq!(t.current(&n("a")), Some(w("@2")));
        assert!(t.contains(&w("@1")));
        assert_eq!(
            t.gone("@2"),
            Some(RuntimeEvent::WindowGone {
                agent: n("a"),
                window: w("@2")
            })
        );
        assert_eq!(t.current(&n("a")), Some(w("@1")));
        assert_eq!(t.current(&n("b")), None);
    }
}

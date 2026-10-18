//! Test fixtures: the UI §4.1 model and helpers that render to text.

use std::time::Duration;

use duohive_core::event::{AgentView, NeedsYouItem};
use duohive_core::org::{NodeKey, Org};
use duohive_core::settings::Settings;
use duohive_core::status::{AgentState, Status};
use duohive_core::testkit::{example_org, n};
use duohive_core::time::Millis;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use crate::model::UiModel;
use duohive_core::view::PaneChunk;

use crate::render::{LiveScreen, RenderCtx, live_size, render, render_live};
use crate::term_view::TermView;
use crate::theme::Palette;

/// "Now" in every test.
pub const NOW: Millis = Millis(1_000 * 86_400_000);

pub fn mins(m: u64) -> Duration {
    Duration::from_secs(m * 60)
}

pub fn ago(d: Duration) -> Millis {
    Millis(NOW.0 - u64::try_from(d.as_millis()).unwrap())
}

pub fn status(state: AgentState, age: Duration) -> Status {
    Status {
        state,
        quiet: false,
        quiet_since: None,
        from_screen: false,
        since: ago(age),
        error: None,
        waiting_on: None,
    }
}

pub fn quiet(age: Duration) -> Status {
    Status {
        quiet: true,
        quiet_since: Some(ago(age)),
        ..status(AgentState::Working, mins(30))
    }
}

pub fn view(status: Status) -> AgentView {
    AgentView {
        status,
        last_reply: None,
        notice: None,
        problem: None,
        worktree: None,
        delivery_pending: false,
        inbox: 0,
    }
}

pub fn replied(status: Status, reply: &str) -> AgentView {
    AgentView {
        last_reply: Some(reply.to_owned()),
        ..view(status)
    }
}

/// A model over `org` with every agent stopped for a day and Duo idle for a minute.
pub fn model_for(org: Org) -> UiModel {
    let mut m = UiModel::new(org);
    m.now = NOW;
    let names: Vec<_> = std::iter::once(m.org.duo.name.clone())
        .chain(m.org.agents().iter().map(|a| a.agent.name.clone()))
        .collect();
    for name in names {
        let s = if m.org.is_duo(&name) {
            status(AgentState::Idle, mins(1))
        } else {
            status(AgentState::Stopped, mins(24 * 60))
        };
        m.agents.insert(name, view(s));
    }
    m
}

/// UI §4.1's main screen: six agents plus Duo, auth-service selected.
pub fn spec_model() -> UiModel {
    let mut m = model_for(example_org());
    let screen = Status {
        from_screen: true,
        ..status(AgentState::Working, mins(1))
    };
    let views = [
        (
            "auth-service",
            replied(
                status(AgentState::Working, mins(2)),
                "Running go test ./...",
            ),
        ),
        (
            "billing-api",
            replied(
                status(AgentState::Waiting, mins(4)),
                "Allow edit to api.go?",
            ),
        ),
        ("squad-1", view(screen)),
        ("squad-2", view(status(AgentState::Finished, mins(3)))),
        ("squad-3", view(quiet(mins(6)))),
    ];
    for (name, v) in views {
        m.agents.insert(n(name), v);
    }
    m.needs_you = vec![
        NeedsYouItem {
            agent: n("squad-3"),
            state: AgentState::Working,
            quiet: true,
            since: ago(mins(6)),
            excerpt: None,
            card: None,
        },
        NeedsYouItem {
            agent: n("billing-api"),
            state: AgentState::Waiting,
            quiet: false,
            since: ago(mins(4)),
            excerpt: Some("Allow edit to api.go?".to_owned()),
            card: None,
        },
    ];
    m.selected = NodeKey::Agent(n("auth-service"));
    m
}

/// Static glyphs (mock-ups draw `●`), dark palette.
pub fn ctx() -> RenderCtx {
    RenderCtx {
        now: NOW,
        spinner: 0,
        palette: Palette::dark(),
        settings: Settings {
            reduce_motion: true,
            ..Settings::default()
        },
        utc_offset_secs: 0,
    }
}

pub fn render_buffer(m: &UiModel, ctx: &RenderCtx, w: u16, h: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| render(f, m, ctx)).unwrap();
    t.backend().buffer().clone()
}

pub fn lines_of(buf: &Buffer) -> Vec<String> {
    let a = buf.area;
    (a.y..a.y + a.height)
        .map(|y| (a.x..a.x + a.width).map(|x| buf[(x, y)].symbol()).collect())
        .collect()
}

pub fn render_lines(m: &UiModel, ctx: &RenderCtx, w: u16, h: u16) -> Vec<String> {
    lines_of(&render_buffer(m, ctx, w, h))
}

/// The first `n` characters of a line.
pub fn cols(line: &str, n: usize) -> String {
    line.chars().take(n).collect()
}

/// A terminal the size of `m`'s live pane on a `w`×`h` screen, showing `screen` with the
/// cursor at `cursor`.
pub fn term_for(m: &UiModel, w: u16, h: u16, screen: &[u8], cursor: (u16, u16)) -> TermView {
    let mut m = m.clone();
    m.screen = ratatui::layout::Rect::new(0, 0, w, h);
    let mut term = TermView::new(live_size(&m).unwrap());
    term.apply(&PaneChunk::Reset {
        bytes: screen.to_vec(),
        cursor,
    });
    term
}

pub fn render_live_lines(
    m: &UiModel,
    ctx: &RenderCtx,
    live: Option<LiveScreen<'_>>,
    w: u16,
    h: u16,
) -> Vec<String> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| render_live(f, m, ctx, live)).unwrap();
    lines_of(t.backend().buffer())
}

/// A worktree row of `agent` for `task`, checked 3 minutes ago against `origin/main`.
pub fn worktree_row(
    agent: &str,
    task: &str,
    state: duohive_core::worktree::WorktreeState,
) -> duohive_core::worktree::WorktreeRow {
    use duohive_core::worktree::{BaseRef, TaskId, WorktreeRow};
    WorktreeRow {
        agent: n(agent),
        task: TaskId(task.to_owned()),
        repo: "/repo".into(),
        path: format!("/wt/{agent}/{task}").into(),
        branch: format!("duohive/{agent}/{task}"),
        created_at: ago(mins(60)),
        created_from: "abc123".to_owned(),
        state,
        checked_at: Some(ago(mins(3))),
        base: Some(BaseRef("origin/main".to_owned())),
    }
}

/// UI §4.9's four rows.
pub fn spec_worktrees() -> Vec<duohive_core::worktree::WorktreeRow> {
    use duohive_core::worktree::WorktreeState as S;
    vec![
        worktree_row("auth-service", "retry-loop", S::InUse),
        worktree_row("pay-qa", "retry-cases", S::Removing),
        worktree_row("squad-2", "flaky-date", S::NotMerged { files_changed: 2 }),
        worktree_row("squad-1", "readme", S::MergedDirty),
    ]
}

/// `h:mm` UTC on day 0, for chat times (`ctx()` has no offset).
pub fn clock(h: u64, min: u64) -> Millis {
    Millis((h * 60 + min) * 60_000)
}

/// A chat entry as the engine publishes it.
pub fn chat_entry(
    id: u64,
    at: Millis,
    kind: duohive_core::chat::ChatKind,
    text: &str,
) -> duohive_core::chat::ChatEntry {
    duohive_core::chat::ChatEntry {
        id: duohive_core::chat::ChatId(id),
        at,
        kind,
        text: text.to_owned(),
    }
}

/// An action line (`→ text   result`).
pub fn action_entry(
    id: u64,
    at: Millis,
    text: &str,
    result: &str,
) -> duohive_core::chat::ChatEntry {
    let kind = duohive_core::chat::ChatKind::Action {
        result: result.to_owned(),
    };
    chat_entry(id, at, kind, text)
}

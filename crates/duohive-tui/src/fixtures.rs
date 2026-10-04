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
use crate::render::{RenderCtx, render};
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
        },
        NeedsYouItem {
            agent: n("billing-api"),
            state: AgentState::Waiting,
            quiet: false,
            since: ago(mins(4)),
            excerpt: Some("Allow edit to api.go?".to_owned()),
        },
    ];
    m.selected = NodeKey::Agent(n("auth-service"));
    m
}

/// Static glyphs (mock-ups draw `●`), dark palette, the default socket.
pub fn ctx() -> RenderCtx {
    RenderCtx {
        now: NOW,
        spinner: 0,
        palette: Palette::dark(),
        settings: Settings {
            reduce_motion: true,
            ..Settings::default()
        },
        tmux_socket: "duohive".to_owned(),
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

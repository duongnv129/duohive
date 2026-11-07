//! Typing that a click or `enter` asked for, started once the agent's terminal is live
//! (click-to-type, CT-D4, CT-D6).

use super::tests::opened;
use super::*;
use duohive_core::org::NodeKey;
use duohive_core::status::AgentState;
use duohive_core::testkit::{FakePaneView, n};

use crate::focus::{AgentMode, Pane};
use crate::model::Dialog;

/// auth-service is live and billing-api (waiting) was just clicked: selected, the agent
/// pane focused, Typing asked for. The sync that closes auth-service's stream has run.
fn clicked_billing() -> (Live, FakePaneView, UiModel, Instant) {
    let (mut live, view, mut m, t) = opened();
    m.selected = NodeKey::Agent(n("billing-api"));
    m.focus.pane = Pane::Agent;
    m.typing_wanted = Some(n("billing-api"));
    live.sync(&mut m, t);
    (live, view, m, t)
}

#[test]
fn wanted_typing_survives_the_switch_and_starts_when_the_new_stream_is_live() {
    let (mut live, view, mut m, t) = clicked_billing();
    assert_eq!(
        m.typing_wanted,
        Some(n("billing-api")),
        "A's stream closing"
    );
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    live.open_due(&mut m, t + SETTLE);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected, "no Reset yet");
    view.push(
        StreamId(2),
        PaneChunk::Reset {
            bytes: b"$ ".to_vec(),
            cursor: (2, 0),
        },
    );
    live.sync(&mut m, t + SETTLE);
    assert_eq!(m.live_agent, Some(n("billing-api")));
    assert!(m.typing());
    assert_eq!(m.typing_wanted, None);
    assert_eq!(m.footer_note, None);
}

#[test]
fn nothing_wanted_and_nothing_live_does_not_start_typing() {
    let mut live = Live::new(Arc::new(duohive_core::ports::NoPaneView), false);
    let mut m = crate::fixtures::spec_model();
    m.screen = ratatui::layout::Rect::new(0, 0, 120, 36);
    m.focus.pane = Pane::Agent;
    assert_eq!(
        (m.typing_wanted.clone(), m.live_agent.clone()),
        (None, None)
    );
    live.sync(&mut m, Instant::now());
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
}

#[test]
fn wanted_typing_for_another_agent_does_not_type_into_this_one() {
    let (mut live, _view, mut m, t) = opened();
    m.focus.pane = Pane::Agent;
    m.typing_wanted = Some(n("billing-api"));
    live.sync(&mut m, t);
    assert_eq!(m.live_agent, Some(n("auth-service")));
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.typing_wanted, None, "the selection is not billing-api");
}

#[test]
fn wanted_typing_is_dropped_when_the_selection_moves() {
    let (mut live, _view, mut m, t) = clicked_billing();
    m.selected = NodeKey::Agent(n("auth-service"));
    live.sync(&mut m, t);
    assert_eq!(m.typing_wanted, None);
}

#[test]
fn wanted_typing_is_dropped_when_focus_leaves_the_agent_pane() {
    let (mut live, _view, mut m, t) = clicked_billing();
    m.focus.pane = Pane::Org;
    live.sync(&mut m, t);
    assert_eq!(m.typing_wanted, None);
}

#[test]
fn wanted_typing_is_dropped_when_the_agent_stops() {
    let (mut live, _view, mut m, t) = clicked_billing();
    m.agents.get_mut(&n("billing-api")).unwrap().status =
        crate::fixtures::status(AgentState::Stopped, crate::fixtures::mins(1));
    live.sync(&mut m, t);
    assert_eq!(m.typing_wanted, None);
}

#[test]
fn wanted_typing_is_dropped_when_a_dialog_opens() {
    let (mut live, _view, mut m, t) = clicked_billing();
    m.dialog = Some(Dialog::Worktrees(crate::model::WorktreeList { cursor: 0 }));
    live.sync(&mut m, t);
    assert_eq!(m.typing_wanted, None);
    assert_eq!(m.footer_note, None);
}

#[test]
fn wanted_typing_and_its_footer_are_cleared_when_the_open_fails() {
    let (mut live, view, mut m, t) = clicked_billing();
    m.footer_note = Some("opening billing-api…".to_owned());
    view.set_open_fails(true);
    live.open_due(&mut m, t + SETTLE);
    assert_eq!(m.typing_wanted, None);
    assert_eq!(m.footer_note, None);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
}

#[test]
fn wanted_typing_is_dropped_when_the_stream_cannot_open() {
    let (mut live, _view, mut m, t) = clicked_billing();
    let status = m.view(&n("billing-api")).status.clone();
    live.blocked = Some((n("billing-api"), status));
    m.footer_note = Some("opening billing-api…".to_owned());
    live.sync(&mut m, t);
    assert_eq!(m.typing_wanted, None);
    assert_eq!(m.footer_note, None);
}

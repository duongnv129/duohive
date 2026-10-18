//! Tests for approval cards: Task 3.6 (TD §6.2; P3-D2, P3-D5, P3-D12, P3-D21, P3-D25).

use std::time::Duration;

use serde_json::json;

use super::super::Input;
use super::super::harness::{Harness, Setup, T0};
use crate::alert::{Alert, AlertKind};
use crate::card::{Card, CardChange, CardId, CardOutcome, CardSource};
use crate::chat::ChatKind;
use crate::command::{AgentDraft, Command, OrgEdit, TeamDraft};
use crate::event::Event;
use crate::hook::HookMessage;
use crate::inbox::{MessageKind, MessageStatus, Sender};
use crate::launch::WindowId;
use crate::names::NodeName;
use crate::org::{Fields, NodeKey, RoleKey};
use crate::ports::RuntimeEvent;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, n};
use crate::time::Millis;
use crate::tool::{CallId, ErrorCode, ToolCall, ToolError, ToolOk, ToolResult};

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn qa() -> RoleKey {
    RoleKey {
        team: Some(nn("Payments")),
        role: nn("QA"),
    }
}

/// A hook from `agent`'s current launch.
fn hook(h: &mut Harness, agent: &str, event: &str) -> Vec<Event> {
    let token = h.rt(agent).token.clone().unwrap();
    h.input(Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: token.0,
        agent: agent.into(),
        event: event.into(),
        payload: json!({}),
    }))
}

/// `agent` launched with no task, in window `@<agent>`, and ready: idle.
fn idle(h: &mut Harness, agent: &str) {
    h.start(agent);
    h.input(Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(format!("@{agent}")),
    }));
    hook(h, agent, "session-start");
    assert_eq!(h.state(agent), AgentState::Idle);
}

/// `agent` launched with a task and ready: working.
fn working(h: &mut Harness, agent: &str) {
    h.give(agent, "first task");
    hook(h, agent, "session-start");
    assert_eq!(h.state(agent), AgentState::Working);
}

/// The texts pasted into `agent`, oldest first.
fn pastes_to(h: &Harness, agent: &str) -> Vec<String> {
    h.calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Paste(a, _, text, _) if a == n(agent) => Some(text),
            _ => None,
        })
        .collect()
}

fn starts_of(h: &Harness, agent: &str) -> usize {
    h.calls()
        .into_iter()
        .filter(|c| matches!(c, RuntimeCall::Start(a, _) if *a == n(agent)))
        .count()
}

/// A tool call with `from`'s token: its result and every other event.
fn call(h: &mut Harness, from: &str, call: ToolCall) -> (ToolResult, Vec<Event>) {
    let token = h.rt(from).token.clone().unwrap();
    let out = h.input(Input::Command(Command::Tool {
        call_id: CallId(7),
        token,
        call,
    }));
    split(out)
}

fn split(out: Vec<Event>) -> (ToolResult, Vec<Event>) {
    let mut result = None;
    let mut rest = Vec::new();
    for e in out {
        match e {
            Event::ToolResult { result: r, .. } => result = Some(r),
            other => rest.push(other),
        }
    }
    (result.expect("no ToolResult"), rest)
}

fn propose(h: &mut Harness, edit: OrgEdit) -> (ToolResult, Vec<Event>) {
    call(
        h,
        "Duo",
        ToolCall::ProposeOrgChange {
            change: Box::new(edit),
        },
    )
}

/// Duo proposes `edit`, which must raise a card.
fn pend(h: &mut Harness, edit: OrgEdit) {
    let (result, _) = propose(h, edit);
    assert!(matches!(result, Ok(ToolOk::Pending { .. })), "{result:?}");
}

fn add_web() -> OrgEdit {
    OrgEdit::AddAgent {
        role: qa(),
        agent: AgentDraft {
            name: n("pay-web"),
            fields: Fields::default(),
        },
    }
}

fn add_web_in(dir: &str) -> OrgEdit {
    OrgEdit::AddAgent {
        role: qa(),
        agent: AgentDraft {
            name: n("pay-web"),
            fields: Fields {
                directory: Some(dir.into()),
                ..Fields::default()
            },
        },
    }
}

fn approve(h: &mut Harness, card: u64) -> Vec<Event> {
    h.send(Command::ApproveCard {
        card: CardId(card),
        edited: None,
    })
}

fn reject(h: &mut Harness, card: u64) -> Vec<Event> {
    h.send(Command::RejectCard { card: CardId(card) })
}

fn rejected(ev: &[Event]) -> Vec<String> {
    ev.iter()
        .filter_map(|e| match e {
            Event::CommandRejected { reason } => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

fn answered(ev: &[Event]) -> Vec<(CardId, CardOutcome)> {
    ev.iter()
        .filter_map(|e| match e {
            Event::CardAnswered { card, outcome } => Some((*card, outcome.clone())),
            _ => None,
        })
        .collect()
}

fn raised(ev: &[Event]) -> Vec<Card> {
    ev.iter()
        .filter_map(|e| match e {
            Event::CardRaised(card) => Some((**card).clone()),
            _ => None,
        })
        .collect()
}

fn alerts(ev: &[Event]) -> Vec<Alert> {
    ev.iter()
        .filter_map(|e| match e {
            Event::Alert(a) => Some(a.clone()),
            _ => None,
        })
        .collect()
}

fn has_agent(h: &Harness, agent: &str) -> bool {
    h.engine.org.find_agent(&n(agent)).is_some()
}

fn stored_card(h: &Harness, id: u64) -> Card {
    h.store.state.lock().unwrap().cards[&CardId(id)].clone()
}

/// Raises a stop card for `agent` straight from the engine, as `stop_agent` will (3.5).
fn raise_stop(h: &mut Harness, agent: &str) -> (CardId, Vec<Event>) {
    let mut out = Vec::new();
    let id = h.engine.raise_card(
        CardSource::Duo,
        CardChange::Stop { agent: n(agent) },
        &mut out,
    );
    (id, out)
}

#[test]
fn tool_returns_pending_with_card_id() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    let (result, events) = propose(&mut h, add_web());
    assert_eq!(result, Ok(ToolOk::Pending { card: CardId(1) }));
    let cards = raised(&events);
    assert_eq!(cards.len(), 1, "{events:?}");
    assert_eq!(
        (
            cards[0].id,
            cards[0].source,
            &cards[0].change,
            cards[0].answer.as_ref()
        ),
        (
            CardId(1),
            CardSource::Duo,
            &CardChange::Org(add_web()),
            None
        )
    );
    assert_eq!(stored_card(&h, 1), cards[0]);
    // Nothing applied, and Duo keeps working: no state change, nothing pasted.
    assert!(!has_agent(&h, "pay-web"));
    assert_eq!(h.state("Duo"), AgentState::Idle);
    assert!(pastes_to(&h, "Duo").is_empty());
    let team = OrgEdit::AddTeam(TeamDraft {
        name: nn("Growth"),
        defaults: Fields::default(),
    });
    let (result, _) = propose(&mut h, team);
    assert_eq!(result, Ok(ToolOk::Pending { card: CardId(2) }));
    let pending: Vec<CardId> = h.engine.snapshot().cards.iter().map(|c| c.id).collect();
    assert_eq!(pending, [CardId(1), CardId(2)]);
}

#[test]
fn propose_invalid_change_refused() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    let dup = OrgEdit::AddAgent {
        role: qa(),
        agent: AgentDraft {
            name: n("auth-service"),
            fields: Fields::default(),
        },
    };
    let (result, events) = propose(&mut h, dup);
    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidChange);
    assert!(
        err.message
            .contains("an agent named auth-service already exists"),
        "{}",
        err.message
    );
    assert!(raised(&events).is_empty());
    assert!(h.engine.snapshot().cards.is_empty());
    assert!(h.store.state.lock().unwrap().cards.is_empty());
}

#[test]
fn remove_running_agent_rejected() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    working(&mut h, "squad-1");
    let refused = ToolError::new(
        ErrorCode::Refused,
        "squad-1 is working; only a stopped agent can be removed",
    );
    let (result, events) = propose(&mut h, OrgEdit::Delete(NodeKey::Agent(n("squad-1"))));
    assert_eq!(result, Err(refused.clone()));
    assert!(raised(&events).is_empty());
    // A role holding one.
    let generalist = NodeKey::Role(RoleKey {
        team: None,
        role: nn("Generalist"),
    });
    let (result, _) = propose(&mut h, OrgEdit::Delete(generalist));
    assert_eq!(result, Err(refused));
    // A team holding one.
    working(&mut h, "pay-qa");
    let (result, _) = propose(&mut h, OrgEdit::Delete(NodeKey::Team(nn("Payments"))));
    assert_eq!(
        result,
        Err(ToolError::new(
            ErrorCode::Refused,
            "pay-qa is working; only a stopped agent can be removed",
        ))
    );
    assert!(h.engine.snapshot().cards.is_empty());
    // A stopped agent can be proposed for removal.
    let (result, _) = propose(&mut h, OrgEdit::Delete(NodeKey::Agent(n("squad-2"))));
    assert_eq!(result, Ok(ToolOk::Pending { card: CardId(1) }));
}

#[test]
fn approve_applies_edit() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    pend(&mut h, add_web());
    let events = approve(&mut h, 1);
    let org = events
        .iter()
        .find_map(|e| match e {
            Event::OrgChanged { org, .. } => Some(org.clone()),
            _ => None,
        })
        .expect("no OrgChanged");
    assert!(org.find_agent(&n("pay-web")).is_some());
    assert!(has_agent(&h, "pay-web"));
    assert_eq!(answered(&events), [(CardId(1), CardOutcome::Approved)]);
    let card = stored_card(&h, 1);
    assert_eq!(card.answer.map(|a| a.0), Some(CardOutcome::Approved));
    assert!(h.engine.snapshot().cards.is_empty());
    // The chat collapses the card into one result line.
    let line = events
        .iter()
        .find_map(|e| match e {
            Event::ChatLine(entry) => Some(entry.clone()),
            _ => None,
        })
        .expect("no chat line");
    assert_eq!(
        line.kind,
        ChatKind::CardResult {
            card: CardId(1),
            outcome: CardOutcome::Approved
        }
    );
    assert_eq!(line.text, "approved: add pay-web");
}

#[test]
fn edited_card_applies_edited_change() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    pend(&mut h, add_web());
    // An edit must keep the card's kind.
    let events = h.send(Command::ApproveCard {
        card: CardId(1),
        edited: Some(Box::new(CardChange::Stop {
            agent: n("squad-1"),
        })),
    });
    assert_eq!(rejected(&events), ["an edit must keep the card's kind"]);
    assert_eq!(h.engine.snapshot().cards.len(), 1);
    let events = h.send(Command::ApproveCard {
        card: CardId(1),
        edited: Some(Box::new(CardChange::Org(add_web_in("~/code/web")))),
    });
    assert_eq!(
        answered(&events),
        [(CardId(1), CardOutcome::EditedApproved)]
    );
    let loc = h.engine.org.find_agent(&n("pay-web")).expect("no pay-web");
    assert_eq!(loc.agent.fields.directory.as_deref(), Some("~/code/web"));
    assert_eq!(
        pastes_to(&h, "Duo"),
        ["card 1 edited and approved: add pay-web"]
    );
}

#[test]
fn reject_applies_nothing_and_tells_duo() {
    let mut h = Harness::new();
    working(&mut h, "Duo");
    pend(&mut h, add_web());
    let events = reject(&mut h, 1);
    assert!(
        !events.iter().any(|e| matches!(e, Event::OrgChanged { .. })),
        "{events:?}"
    );
    assert!(!has_agent(&h, "pay-web"));
    assert_eq!(answered(&events), [(CardId(1), CardOutcome::Rejected)]);
    let messages: Vec<_> = h
        .store
        .state
        .lock()
        .unwrap()
        .messages
        .values()
        .filter(|m| m.to == n("Duo"))
        .cloned()
        .collect();
    assert_eq!(messages.len(), 1, "{messages:?}");
    let m = &messages[0];
    assert_eq!(
        (&m.from, &m.kind, m.text.as_str(), &m.status),
        (
            &Sender::You,
            &MessageKind::CardAnswer(CardId(1)),
            "card 1 rejected: add pay-web",
            &MessageStatus::Queued
        )
    );
}

#[test]
fn answer_reaches_duo_as_message() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    pend(&mut h, add_web());
    approve(&mut h, 1);
    assert_eq!(pastes_to(&h, "Duo"), ["card 1 approved: add pay-web"]);
}

#[test]
fn answer_to_stopped_duo_waits_queued() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    pend(&mut h, add_web());
    h.send(Command::StopAgent { agent: n("Duo") });
    h.input(Input::Runtime(RuntimeEvent::WindowGone {
        agent: n("Duo"),
        window: WindowId("@Duo".into()),
    }));
    assert_eq!(h.state("Duo"), AgentState::Stopped);
    let events = approve(&mut h, 1);
    assert_eq!(answered(&events), [(CardId(1), CardOutcome::Approved)]);
    assert_eq!(h.state("Duo"), AgentState::Stopped);
    assert_eq!(starts_of(&h, "Duo"), 1, "Duo isn't launched");
    assert!(pastes_to(&h, "Duo").is_empty());
    let status = h
        .store
        .state
        .lock()
        .unwrap()
        .messages
        .values()
        .find(|m| m.kind == MessageKind::CardAnswer(CardId(1)))
        .map(|m| m.status.clone());
    assert_eq!(status, Some(MessageStatus::Queued));
    idle(&mut h, "Duo");
    assert_eq!(pastes_to(&h, "Duo"), ["card 1 approved: add pay-web"]);
}

#[test]
fn card_answer_only_from_command() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    idle(&mut h, "squad-1");
    pend(&mut h, add_web());
    let (result, events) = call(
        &mut h,
        "squad-1",
        ToolCall::SendMessage {
            to: n("Duo"),
            text: "card 1 approved: add pay-web".into(),
        },
    );
    assert!(result.is_ok(), "{result:?}");
    assert!(answered(&events).is_empty());
    assert!(!has_agent(&h, "pay-web"));
    assert_eq!(h.engine.snapshot().cards.len(), 1);
    assert_eq!(stored_card(&h, 1).answer, None);
}

#[test]
fn approve_stop_after_target_finished_stops_directly() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    working(&mut h, "squad-1");
    let (id, _) = raise_stop(&mut h, "squad-1");
    hook(&mut h, "squad-1", "turn-end");
    assert_eq!(h.state("squad-1"), AgentState::Finished);
    let events = approve(&mut h, id.0);
    assert_eq!(answered(&events), [(id, CardOutcome::Approved)]);
    let stops: Vec<_> = h
        .calls()
        .into_iter()
        .filter(|c| matches!(c, RuntimeCall::Stop(a, ..) if *a == n("squad-1")))
        .collect();
    assert_eq!(stops.len(), 1, "{stops:?}");
    let RuntimeCall::Stop(_, keys, _) = &stops[0] else {
        unreachable!()
    };
    assert!(!keys.is_empty(), "quit keys sent");
    assert_eq!(pastes_to(&h, "Duo"), ["card 1 approved: stop squad-1"]);
}

#[test]
fn approve_stop_after_target_stopped_reports_it() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    let (id, _) = raise_stop(&mut h, "squad-1");
    let events = approve(&mut h, id.0);
    assert_eq!(
        answered(&events),
        [(
            id,
            CardOutcome::NotApplied("squad-1 is already stopped".into())
        )]
    );
    assert_eq!(
        pastes_to(&h, "Duo"),
        ["card 1 approved, not applied (squad-1 is already stopped): stop squad-1"]
    );
}

#[test]
fn approve_remove_after_agent_started_not_applied() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    let (result, _) = propose(&mut h, OrgEdit::Delete(NodeKey::Agent(n("pay-qa"))));
    assert_eq!(result, Ok(ToolOk::Pending { card: CardId(1) }));
    h.start("pay-qa");
    let events = approve(&mut h, 1);
    let outcome = answered(&events);
    assert!(
        matches!(
            outcome.as_slice(),
            [(CardId(1), CardOutcome::NotApplied(_))]
        ),
        "{outcome:?}"
    );
    assert!(has_agent(&h, "pay-qa"));
    assert!(
        !events.iter().any(|e| matches!(e, Event::OrgChanged { .. })),
        "{events:?}"
    );
}

#[test]
fn approve_add_after_name_taken_not_applied() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    pend(&mut h, add_web());
    h.send(Command::EditOrg(add_web()));
    assert!(has_agent(&h, "pay-web"));
    let events = approve(&mut h, 1);
    let outcome = answered(&events);
    let [(CardId(1), CardOutcome::NotApplied(reason))] = outcome.as_slice() else {
        panic!("{outcome:?}");
    };
    assert!(reason.contains("pay-web"), "{reason}");
}

#[test]
fn answered_card_refuses_second_answer() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    pend(&mut h, add_web());
    approve(&mut h, 1);
    let again = approve(&mut h, 1);
    assert_eq!(rejected(&again), ["card 1 was already answered"]);
    assert!(answered(&again).is_empty());
    let again = reject(&mut h, 1);
    assert_eq!(rejected(&again), ["card 1 was already answered"]);
    assert_eq!(rejected(&reject(&mut h, 9)), ["no card 9"]);
    assert_eq!(pastes_to(&h, "Duo").len(), 1);
}

#[test]
fn cards_in_needs_you_oldest_first() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    h.force("squad-1", AgentState::Waiting);
    h.rt_mut("squad-1").status.since = T0 + Duration::from_secs(5);
    h.force("squad-2", AgentState::Waiting);
    h.rt_mut("squad-2").status.since = T0 + Duration::from_secs(20);
    h.at(Duration::from_secs(10));
    let (_, events) = propose(&mut h, add_web());
    let items = h.engine.needs_you();
    let order: Vec<(String, Option<CardId>)> = items
        .iter()
        .map(|i| (i.agent.to_string(), i.card))
        .collect();
    assert_eq!(
        order,
        [
            ("squad-1".to_owned(), None),
            ("Duo".to_owned(), Some(CardId(1))),
            ("squad-2".to_owned(), None)
        ]
    );
    let row = &items[1];
    assert_eq!(
        (row.state, row.quiet, row.since, row.excerpt.as_deref()),
        (
            AgentState::Waiting,
            false,
            T0 + Duration::from_secs(10),
            Some("add pay-web")
        )
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::NeedsYouChanged(list) if list.len() == 3)),
        "{events:?}"
    );
    let events = approve(&mut h, 1);
    assert!(h.engine.needs_you().iter().all(|i| i.card.is_none()));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::NeedsYouChanged(list) if list.len() == 2)),
        "{events:?}"
    );
}

#[test]
fn card_raises_alert() {
    let mut h = Harness::new();
    working(&mut h, "squad-1");
    let (_, events) = raise_stop(&mut h, "squad-1");
    let alert = Alert {
        agent: n("Duo"),
        kind: AlertKind::NeedsYou,
        title: "Duo needs you".into(),
        body: "stop squad-1".into(),
    };
    assert_eq!(alerts(&events), std::slice::from_ref(&alert));
    assert_eq!(h.notified(), [alert]);
}

#[test]
fn card_alert_respects_needs_you_toggle() {
    let mut setup = Setup::default();
    setup.settings.alerts.needs_you = false;
    let mut h = setup.build();
    working(&mut h, "squad-1");
    let (_, events) = raise_stop(&mut h, "squad-1");
    assert_eq!(raised(&events).len(), 1);
    assert!(alerts(&events).is_empty());
    assert!(h.notified().is_empty());
}

#[test]
fn no_card_alerts_during_replay() {
    let mut first = Harness::new();
    idle(&mut first, "Duo");
    let mut h = Setup {
        store: first.store.clone(),
        ..Setup::default()
    }
    .build();
    h.engine.start_up();
    // Before the inventory: start-up is still replaying.
    let (result, events) = propose(&mut h, add_web());
    assert_eq!(result, Ok(ToolOk::Pending { card: CardId(1) }));
    assert_eq!(raised(&events).len(), 1);
    assert!(alerts(&events).is_empty());
    assert!(h.notified().is_empty());
}

#[test]
fn pending_cards_survive_restart() {
    let mut first = Harness::new();
    idle(&mut first, "Duo");
    pend(&mut first, add_web());
    let team = OrgEdit::AddTeam(TeamDraft {
        name: nn("Growth"),
        defaults: Fields::default(),
    });
    pend(&mut first, team);
    // The newest card is answered: ids still continue after it.
    approve(&mut first, 2);
    let mut h = Setup {
        store: first.store.clone(),
        start: T0 + Duration::from_secs(60),
        ..Setup::default()
    }
    .build();
    let events = h.engine.start_up();
    let Some(Event::Snapshot(snap)) = events.first() else {
        panic!("no snapshot: {events:?}");
    };
    let pending: Vec<CardId> = snap.cards.iter().map(|c| c.id).collect();
    assert_eq!(pending, [CardId(1)]);
    assert_eq!(snap.cards[0].change, CardChange::Org(add_web()));
    let rows: Vec<Option<CardId>> = snap.needs_you.iter().map(|i| i.card).collect();
    assert_eq!(rows, [Some(CardId(1))]);
    assert_eq!(snap.needs_you[0].since, Millis(T0.0));
    assert!(alerts(&events).is_empty());
    assert!(h.notified().is_empty());
    h.input(Input::Runtime(RuntimeEvent::Inventory(Vec::new())));
    assert!(h.notified().is_empty());
    let (id, _) = raise_stop(&mut h, "squad-1");
    assert_eq!(id, CardId(3));
}

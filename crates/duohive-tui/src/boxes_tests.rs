//! The pure row list for the org boxes (UI §4.1.5).

use duohive_core::colour::TeamColour;
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{AgentDef, Fields, NodeKey, Role, RoleKey};
use duohive_core::status::{AgentState, Status};
use duohive_core::testkit::n;

use crate::boxes::{BoxLine, BoxLineKind as K, Tone, box_rows};
use crate::filter::Filter;
use crate::fixtures::{NOW, five_agent_org, mins, status, view};
use crate::format::width;
use crate::model::UiModel;

fn rows(m: &UiModel) -> Vec<BoxLine> {
    box_rows(m, NOW, true)
}

fn kinds(rows: &[BoxLine]) -> Vec<K> {
    rows.iter().map(|r| r.kind).collect()
}

fn duo(m: &UiModel) -> AgentName {
    m.org.duo.name.clone()
}

fn with_duo(state: AgentState, error: Option<&str>) -> UiModel {
    let mut m = five_agent_org();
    let d = duo(&m);
    let s = Status {
        error: error.map(str::to_owned),
        ..status(state, mins(1))
    };
    m.agents.insert(d, view(s));
    m
}

fn agent_line<'a>(rows: &'a [BoxLine], name: &str) -> &'a BoxLine {
    rows.iter()
        .find(|r| r.kind == K::Agent && r.key == Some(NodeKey::Agent(n(name))))
        .unwrap_or_else(|| panic!("no row for {name}"))
}

fn details<'a>(rows: &'a [BoxLine], name: &str) -> Vec<&'a BoxLine> {
    rows.iter()
        .filter(|r| r.kind == K::Detail && r.key == Some(NodeKey::Agent(n(name))))
        .collect()
}

fn select(m: &mut UiModel, name: &str) {
    m.selected = NodeKey::Agent(n(name));
}

fn generalist(agent: &str) -> Role {
    Role {
        name: NodeName::parse("Generalist").unwrap(),
        defaults: Fields::default(),
        agents: vec![AgentDef {
            name: n(agent),
            fields: Fields::default(),
        }],
        color: None,
    }
}

#[test]
fn duo_is_a_four_line_card_whatever_the_state() {
    for (state, pill) in [
        (AgentState::Idle, "◦ idle"),
        (AgentState::Working, "● working"),
        (AgentState::Waiting, "◆ waiting"),
        (AgentState::Errored, "✕ errored"),
        (AgentState::Stopped, "○ stopped"),
        (AgentState::Starting, "◌ starting"),
        // Duo goes straight to idle, so `finished` shows as idle.
        (AgentState::Finished, "◦ idle"),
    ] {
        let m = with_duo(state, Some("boom"));
        let r = rows(&m);
        assert_eq!(
            kinds(&r[..4]),
            [
                K::ManagerTop,
                K::ManagerName,
                K::ManagerCounts,
                K::ManagerBottom
            ],
            "{state:?}"
        );
        assert_eq!(r[1].right, pill, "{state:?}");
        assert_eq!(r[0].label, "⬢ Manager");
        assert!(
            r[..4]
                .iter()
                .all(|l| l.key == Some(NodeKey::Duo) && l.hue.is_none())
        );
    }
}

#[test]
fn a_long_duo_name_is_cut_at_16_columns_in_the_middle() {
    let mut m = five_agent_org();
    m.org.duo.name = AgentName::parse("platform-manager-and-coordinator-agent").unwrap();
    assert_eq!(rows(&m)[1].label, "platform…r-agent");
}

#[test]
fn renaming_duo_keeps_the_card_four_rows_and_the_name_in_16_columns() {
    let long = "x".repeat(40);
    for name in ["a", "a-", "a_", "7-eleven", long.as_str()] {
        let mut m = five_agent_org();
        m.org.duo.name = AgentName::parse(name).unwrap();
        let r = rows(&m);
        assert_eq!(r[3].kind, K::ManagerBottom, "{name}");
        assert!(width(&r[1].label) <= 16, "{name}: {}", r[1].label);
        assert_eq!(r[4].kind, K::TeamTop, "{name}");
    }
}

#[test]
fn the_counts_line_matches_the_header() {
    let mut m = five_agent_org();
    // The fixture: billing-api and one card.
    assert_eq!(rows(&m)[2].label, "2 need you · 1 card");
    let mut item = m.needs_you[0].clone();
    m.needs_you.truncate(1);
    assert_eq!(rows(&m)[2].label, "1 need you");
    item.agent = n("squad-9");
    m.needs_you.push(item.clone());
    assert_eq!(rows(&m)[2].label, "2 need you");
    for id in [1, 2] {
        let mut card = item.clone();
        card.card = Some(duohive_core::card::CardId(id));
        m.needs_you.push(card);
    }
    assert_eq!(rows(&m)[2].label, "4 need you · 2 cards");
    m.needs_you.clear();
    assert_eq!(rows(&m)[2].label, "nothing needs you");
}

#[test]
fn a_duo_error_replaces_the_counts_with_its_short_reason() {
    let m = with_duo(AgentState::Errored, Some("opencode not found (code 127)"));
    assert_eq!(rows(&m)[2].label, "opencode not found");
}

#[test]
fn the_short_reason_stops_at_the_first_space_paren_or_middle_dot() {
    for (reason, want) in [
        (
            "opencode v2.0 not found (code 127)",
            "opencode v2.0 not found",
        ),
        ("claude not found · install it", "claude not found"),
        ("a.b.c failed", "a.b.c failed"),
        ("first (a) · second", "first"),
    ] {
        let m = with_duo(AgentState::Errored, Some(reason));
        assert_eq!(rows(&m)[2].label, want, "{reason}");
    }
}

#[test]
fn a_long_duo_error_reason_is_cut_with_an_ellipsis_on_line_2() {
    let m = with_duo(
        AgentState::Errored,
        Some("the launcher could not start the manager process at all"),
    );
    let label = &rows(&m)[2].label;
    assert_eq!(width(label), 27);
    assert!(label.ends_with('…'), "{label}");
}

#[test]
fn the_starting_pill_is_the_widest_and_still_fits_beside_a_16_column_name() {
    let mut m = five_agent_org();
    m.org.duo.name = AgentName::parse("platform-manager-and-coordinator-agent").unwrap();
    let d = duo(&m);
    m.agents
        .insert(d, view(status(AgentState::Starting, mins(1))));
    let r = rows(&m);
    assert_eq!(r[1].right, "◌ starting");
    assert!(width(&r[1].label) + 1 + width(&r[1].right) <= 28);
}

#[test]
fn the_manager_card_stays_under_a_filter() {
    let mut m = five_agent_org();
    m.filter = Filter::Working;
    let r = rows(&m);
    assert_eq!(
        kinds(&r[..4]),
        [
            K::ManagerTop,
            K::ManagerName,
            K::ManagerCounts,
            K::ManagerBottom
        ]
    );
}

#[test]
fn boxes_stack_without_a_gap() {
    let r = rows(&five_agent_org());
    let at = r.iter().position(|l| l.kind == K::TeamBottom).unwrap();
    assert_eq!(r[at + 1].kind, K::TeamTop);
    assert_eq!(r[3].kind, K::ManagerBottom);
    assert_eq!(r[4].kind, K::TeamTop);
    // Every line is a box line: nothing blank sits between boxes.
    assert!(
        r.iter()
            .all(|l| !(l.kind == K::Detail && l.label.is_empty()))
    );
}

#[test]
fn a_role_under_duo_is_its_own_box_with_agent_rows_inside() {
    let mut m = five_agent_org();
    m.org.roles = vec![generalist("squad-1")];
    m.agents
        .insert(n("squad-1"), view(status(AgentState::Idle, mins(1))));
    let r = rows(&m);
    let tail: Vec<_> = r.iter().rev().take(3).map(|l| l.kind).collect();
    assert_eq!(tail, [K::RoleBottom, K::Agent, K::RoleTop]);
    let top = &r[r.len() - 3];
    assert_eq!(top.label, "◇ Generalist");
    assert!(!top.in_team);
    // The third sibling: Payments teal, Platform violet, Generalist slate.
    assert_eq!(top.hue, Some(TeamColour::Slate));
    // A role inside a team is a nested box in its team's hue.
    let backend = r
        .iter()
        .find(|l| l.kind == K::RoleTop && l.label == "◇ Backend")
        .unwrap();
    assert!(backend.in_team);
    assert_eq!(backend.hue, Some(TeamColour::Teal));
}

#[test]
fn roll_ups_are_on_team_and_top_level_role_boxes_only() {
    let mut m = five_agent_org();
    let r = rows(&m);
    let top = |label: &str| {
        r.iter()
            .find(|l| l.kind == K::TeamTop && l.label == label)
            .unwrap()
    };
    assert_eq!(top("▣ Payments").right, "● 1 ◆ 1");
    assert_eq!(top("▣ Platform").right, "● 1");
    assert!(
        r.iter()
            .filter(|l| l.kind == K::RoleTop && l.in_team)
            .all(|l| l.right.is_empty())
    );
    // A top-level role carries its own roll-up, with `~` when a counted state is read from
    // the screen.
    m.org.roles = vec![generalist("squad-1")];
    let s = Status {
        from_screen: true,
        ..status(AgentState::Working, mins(1))
    };
    m.agents.insert(n("squad-1"), view(s));
    let r = rows(&m);
    let role = r
        .iter()
        .find(|l| l.kind == K::RoleTop && !l.in_team)
        .unwrap();
    assert_eq!(role.right, "~● 1");
}

#[test]
fn a_selected_agent_gets_one_detail_line() {
    let m = five_agent_org();
    let r = rows(&m);
    let d = details(&r, "auth-service");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].label, "working 2m · \"Running go test ./...\"");
    assert_eq!(d[0].tone, Tone::Plain);
    assert!(agent_line(&r, "auth-service").selected);
    assert!(details(&r, "billing-api").is_empty());
    assert_eq!(r.iter().filter(|l| l.kind == K::Detail).count(), 1);
}

#[test]
fn replies_off_leaves_the_reply_out_of_the_detail() {
    let m = five_agent_org();
    let r = box_rows(&m, NOW, false);
    assert_eq!(details(&r, "auth-service")[0].label, "working 2m");
}

#[test]
fn a_stopped_agent_detail_has_no_reply() {
    let mut m = five_agent_org();
    select(&mut m, "pay-qa");
    m.agents.insert(
        n("pay-qa"),
        crate::fixtures::replied(status(AgentState::Stopped, mins(24 * 60)), "old reply"),
    );
    let r = rows(&m);
    assert_eq!(details(&r, "pay-qa")[0].label, "stopped 1d");
}

#[test]
fn an_errored_agent_detail_names_the_reason() {
    let mut m = five_agent_org();
    select(&mut m, "pay-qa");
    let s = Status {
        error: Some("opencode not found".to_owned()),
        ..status(AgentState::Errored, mins(1))
    };
    m.agents.insert(n("pay-qa"), view(s));
    let r = rows(&m);
    let d = details(&r, "pay-qa");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].label, "errored · opencode not found (e)");
    assert_eq!(d[0].tone, Tone::Problem);
}

#[test]
fn a_problem_shows_its_detail_when_not_selected() {
    let mut m = five_agent_org();
    let mut v = view(status(AgentState::Stopped, mins(24 * 60)));
    v.problem = Some("✕ ~/code/pay not found · e to fix".to_owned());
    m.agents.insert(n("pay-qa"), v);
    let mut w = view(status(AgentState::Stopped, mins(24 * 60)));
    w.notice = Some("untested CLI version 9.9".to_owned());
    m.agents.insert(n("infra-1"), w);
    let r = rows(&m);
    let d = details(&r, "pay-qa");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].label, "~/code/pay not found · e to fix");
    assert_eq!(d[0].tone, Tone::Problem);
    assert!(!d[0].selected);
    let notice = details(&r, "infra-1");
    assert_eq!(notice.len(), 1);
    assert_eq!(notice[0].tone, Tone::Notice);
    assert_eq!(notice[0].label, "untested CLI version 9.9");
}

#[test]
fn a_selected_agent_with_a_problem_shows_the_problem_in_place_of_the_state_line() {
    let mut m = five_agent_org();
    let s = Status {
        error: Some("opencode not found".to_owned()),
        ..status(AgentState::Errored, mins(1))
    };
    m.agents.insert(n("auth-service"), view(s));
    let r = rows(&m);
    let d = details(&r, "auth-service");
    assert_eq!(d.len(), 1, "one second-line entry only");
    assert!(d[0].label.starts_with("errored · opencode not found"));
    assert!(d[0].selected);
}

#[test]
fn a_notice_follows_the_state_line() {
    let mut m = five_agent_org();
    let mut v = crate::fixtures::replied(status(AgentState::Working, mins(2)), "Run");
    v.notice = Some("untested 9.9".to_owned());
    m.agents.insert(n("auth-service"), v);
    let r = rows(&m);
    let d = details(&r, "auth-service");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].label, "working 2m · \"Run\" · untested 9.9");
}

#[test]
fn an_agent_name_is_cut_in_the_middle_and_keeps_its_inbox_marker() {
    let mut m = five_agent_org();
    let long = "billing-payments-reconciliation-worker";
    m.org.teams[0].roles[0].agents[1].name = n(long);
    m.agents
        .insert(n(long), view(status(AgentState::Idle, mins(1))));
    m.agents.get_mut(&n(long)).unwrap().inbox = 12;
    let r = rows(&m);
    let line = r
        .iter()
        .find(|l| l.kind == K::Agent && l.inbox == 12)
        .expect("the long agent's row");
    assert!(line.label.contains('…'), "{}", line.label);
    assert!(width(&line.label) <= crate::boxes::name_room(true, 12));
    assert!(line.label.starts_with("billing"));
    assert!(line.label.ends_with("worker"));
}

#[test]
fn a_tilde_leads_the_icon_when_the_state_is_read_from_the_screen() {
    let mut m = five_agent_org();
    let s = Status {
        from_screen: true,
        ..status(AgentState::Working, mins(1))
    };
    m.agents.insert(n("infra-2"), view(s));
    let r = rows(&m);
    assert_eq!(agent_line(&r, "infra-2").right, "~●");
    assert_eq!(agent_line(&r, "infra-1").right, "✓");
    assert_eq!(agent_line(&r, "pay-qa").right, "○");
}

#[test]
fn a_long_last_reply_is_cut_at_the_box_edge() {
    let mut m = five_agent_org();
    let long = "x".repeat(200);
    m.agents.insert(
        n("auth-service"),
        crate::fixtures::replied(status(AgentState::Working, mins(2)), &long),
    );
    let r = rows(&m);
    let d = details(&r, "auth-service");
    let text = &d[0].label;
    assert!(width(text) <= crate::boxes::detail_width(true), "{text}");
    assert!(text.ends_with("…\""), "{text}");
}

#[test]
fn a_folded_team_or_role_is_its_title_line_only() {
    let mut m = five_agent_org();
    m.folded
        .insert(NodeKey::Team(NodeName::parse("Payments").unwrap()));
    let r = rows(&m);
    let top = r
        .iter()
        .find(|l| l.kind == K::TeamTop && l.label == "▣ Payments")
        .unwrap();
    assert!(top.folded);
    assert_eq!(top.right, "● 1 ◆ 1", "the roll-up stays");
    let at = r.iter().position(|l| l.label == "▣ Payments").unwrap();
    assert_eq!(r[at + 1].kind, K::TeamTop, "no body and no bottom line");
    // A role folds on its own, inside its team.
    let mut m = five_agent_org();
    m.folded.insert(NodeKey::Role(RoleKey {
        team: Some(NodeName::parse("Platform").unwrap()),
        role: NodeName::parse("Infra").unwrap(),
    }));
    let r = rows(&m);
    let at = r.iter().position(|l| l.label == "◇ Infra").unwrap();
    assert!(r[at].folded);
    assert_eq!(r[at + 1].kind, K::TeamBottom);
}

#[test]
fn a_filter_keeps_matching_agents_and_their_boxes() {
    let mut m = five_agent_org();
    m.filter = Filter::Working;
    let r = rows(&m);
    let agents: Vec<_> = r
        .iter()
        .filter(|l| l.kind == K::Agent)
        .map(|l| l.label.as_str())
        .collect();
    assert_eq!(agents, ["auth-service", "infra-2"]);
    assert!(r.iter().all(|l| l.label != "◇ QA"));
}

#[test]
fn a_filter_with_no_match_shows_its_line() {
    let mut m = five_agent_org();
    m.filter = Filter::Finished;
    m.agents
        .insert(n("infra-1"), view(status(AgentState::Idle, mins(1))));
    let r = rows(&m);
    let line = r
        .iter()
        .find(|l| l.kind == K::FilterEmpty)
        .expect("the filter line");
    assert_eq!(line.label, "no agent is finished · esc clears the filter");
}

#[test]
fn an_empty_org_is_the_manager_card_alone() {
    let mut m = five_agent_org();
    m.org.teams.clear();
    let r = rows(&m);
    assert_eq!(
        kinds(&r),
        [
            K::ManagerTop,
            K::ManagerName,
            K::ManagerCounts,
            K::ManagerBottom
        ]
    );
}

#[test]
fn an_empty_role_shows_the_hint_line() {
    let mut m = five_agent_org();
    m.org.teams[1].roles[0].agents.clear();
    let r = rows(&m);
    let line = r
        .iter()
        .find(|l| l.kind == K::EmptyRole)
        .expect("the hint line");
    assert_eq!(line.label, "no agents yet · n to add one or ask Duo");
}

#[test]
fn selection_marks_the_box_or_row_it_names() {
    let mut m = five_agent_org();
    m.selected = NodeKey::Team(NodeName::parse("Payments").unwrap());
    let r = rows(&m);
    let marked: Vec<_> = r.iter().filter(|l| l.selected).map(|l| l.kind).collect();
    assert_eq!(marked, [K::TeamTop, K::TeamBottom]);
    m.selected = NodeKey::Duo;
    let marked: Vec<_> = rows(&m)
        .iter()
        .filter(|l| l.selected)
        .map(|l| l.kind)
        .collect();
    assert_eq!(
        marked,
        [
            K::ManagerTop,
            K::ManagerName,
            K::ManagerCounts,
            K::ManagerBottom
        ]
    );
}

#[test]
fn box_rows_for_100_agents_is_linear() {
    let mut m = five_agent_org();
    let names: Vec<AgentName> = (0..100).map(|i| n(&format!("squad-{i}"))).collect();
    m.org.teams.clear();
    m.org.roles = vec![Role {
        name: NodeName::parse("Squad").unwrap(),
        defaults: Fields::default(),
        agents: names
            .iter()
            .map(|a| AgentDef {
                name: a.clone(),
                fields: Fields::default(),
            })
            .collect(),
        color: None,
    }];
    for a in &names {
        m.agents
            .insert(a.clone(), view(status(AgentState::Idle, mins(1))));
    }
    m.selected = NodeKey::Agent(names[50].clone());
    let r = rows(&m);
    // 100 agent rows, the card (4), the role box (2) and the selected agent's one detail line.
    assert_eq!(r.len(), 100 + 4 + 2 + 1);
}

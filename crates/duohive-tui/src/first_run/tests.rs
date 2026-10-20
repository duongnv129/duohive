//! The first-run wizard (UI §4.3): snapshots of its three steps and the keys
//! that drive them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::adapter::CliPresence;
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;
use duohive_core::event::Event;
use duohive_core::templates::{STARTER_PROMPTS, SquadInput, TemplateInput};
use duohive_core::testkit::{FakeDirs, example_org};
use duohive_core::validate::DirStatus;

use super::{FirstRun, Step, TeamRow, TemplateChoice};
use crate::fixtures::{ctx, model_for, render_lines};
use crate::forms::LineEdit;
use crate::input::{UiEffect, map_key, update};
use crate::model::{SharedDirs, UiModel};

const HOME: &str = "/home/me";

fn found(version: &str) -> CliPresence {
    CliPresence::Found {
        version: version.to_owned(),
    }
}

fn both_found() -> Vec<(CliKind, CliPresence)> {
    vec![
        (CliKind::Claude, found("2.1.0")),
        (CliKind::Opencode, found("0.4.2")),
    ]
}

fn claude_only() -> Vec<(CliKind, CliPresence)> {
    vec![
        (CliKind::Claude, found("2.1.0")),
        (CliKind::Opencode, CliPresence::Missing),
    ]
}

fn no_cli() -> Vec<(CliKind, CliPresence)> {
    vec![
        (CliKind::Claude, CliPresence::Missing),
        (CliKind::Opencode, CliPresence::Missing),
    ]
}

fn dirs() -> FakeDirs {
    FakeDirs::new(HashMap::from([
        (PathBuf::from("/home/me/code/app"), DirStatus::GitRepo),
        (PathBuf::from("/home/me/code/plain"), DirStatus::Found),
        (PathBuf::from("/home/me/gone"), DirStatus::Missing),
    ]))
}

fn model(w: FirstRun) -> UiModel {
    let mut m = model_for(example_org());
    m.dirs = SharedDirs(Arc::new(dirs()));
    m.home = PathBuf::from(HOME);
    m.first_run = Some(w.inventory.clone());
    m.wizard = Some(w);
    m
}

fn wizard() -> FirstRun {
    FirstRun::new(claude_only(), Some("Duong".to_owned()))
}

fn at(template: TemplateChoice) -> FirstRun {
    let mut w = wizard();
    w.step = Step::Form;
    w.cursor = TemplateChoice::ALL
        .iter()
        .position(|c| *c == template)
        .unwrap_or(0);
    // Blank has no fields, so its form starts on Create org.
    w.focus = usize::from(w.controls() == 0);
    w
}

fn press(m: &mut UiModel, code: KeyCode) -> Vec<UiEffect> {
    let k = KeyEvent::new(code, KeyModifiers::NONE);
    match map_key(m, k) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

fn type_text(m: &mut UiModel, s: &str) {
    for c in s.chars() {
        press(m, KeyCode::Char(c));
    }
}

fn text(m: &UiModel) -> String {
    render_lines(m, &ctx(), 120, 40).join("\n")
}

fn shot(name: &str, w: FirstRun) {
    let m = model(w);
    for (size, width, height) in [("80x24", 80, 24), ("160x48", 160, 48)] {
        insta::assert_snapshot!(
            format!("{name}_{size}"),
            render_lines(&m, &ctx(), width, height).join("\n")
        );
    }
}

#[test]
fn step1_you_and_duo() {
    shot("step1_you_and_duo", wizard());
}

#[test]
fn step1_missing_cli_radio_disabled() {
    let mut m = model(wizard());
    if let Some(w) = m.wizard.as_mut() {
        w.focus = 1;
    }
    // OpenCode isn't installed, so its radio is disabled and arrows don't reach it.
    let before = m.wizard.as_ref().map(|w| w.duo_cli);
    press(&mut m, KeyCode::Right);
    assert_eq!(m.wizard.as_ref().map(|w| w.duo_cli), before);
    let text = text(&m);
    assert!(text.contains("(-) OpenCode"), "{text}");
    shot("step1_missing_cli_radio_disabled", wizard());
}

#[test]
fn step2_templates_with_preview() {
    let mut w = wizard();
    w.step = Step::Template;
    let text = render_lines(&model(w.clone()), &ctx(), 120, 40).join("\n");
    for want in [
        "Agent Squad",
        "Product Team",
        "Engineering Org",
        "Blank",
        "a Generalist role; any free agent takes a task",
        "Preview",
        "squad-1",
        "squad-3",
    ] {
        assert!(text.contains(want), "missing {want:?} in\n{text}");
    }
    shot("step2_templates_with_preview", w);
}

#[test]
fn step3_agent_squad() {
    let mut w = at(TemplateChoice::AgentSquad);
    w.squad.directory = LineEdit::new("~/code/app");
    shot("step3_agent_squad", w);
}

#[test]
fn step3_product_team() {
    let mut w = at(TemplateChoice::ProductTeam);
    w.product.directories[0] = LineEdit::new("~/code/app");
    shot("step3_product_team", w);
}

#[test]
fn step3_engineering_org() {
    let mut w = at(TemplateChoice::EngineeringOrg);
    w.engineering.rows = vec![
        TeamRow {
            name: LineEdit::new("Payments"),
            repo: LineEdit::new("~/code/app"),
        },
        TeamRow {
            name: LineEdit::new("Web"),
            repo: LineEdit::new("~/code/plain"),
        },
    ];
    shot("step3_engineering_org", w);
}

#[test]
fn step3_blank_summary() {
    shot("step3_blank_summary", at(TemplateChoice::Blank));
}

#[test]
fn blank_starts_on_create_org() {
    let mut m = model(wizard());
    let w = m.wizard.as_mut().unwrap();
    w.step = Step::Template;
    w.cursor = TemplateChoice::ALL.len() - 1;
    press(&mut m, KeyCode::Enter);
    let w = m.wizard.as_ref().unwrap();
    assert_eq!((w.step, w.focus), (Step::Form, w.controls() + 1));
    let effects = press(&mut m, KeyCode::Enter);
    assert!(matches!(
        effects.as_slice(),
        [UiEffect::Send(Command::CreateOrg { .. })]
    ));
}

#[test]
fn squad_directory_missing() {
    let mut w = at(TemplateChoice::AgentSquad);
    w.squad.directory = LineEdit::new("~/gone");
    let text = text(&model(w));
    assert!(text.contains("✕ not found"), "{text}");
}

#[test]
fn squad_not_git_with_worktrees_on() {
    let mut w = at(TemplateChoice::AgentSquad);
    w.squad.directory = LineEdit::new("~/code/plain");
    assert!(w.squad.worktrees);
    let text = text(&model(w));
    assert!(text.contains("✕ not a git repo"), "{text}");
}

#[test]
fn squad_size_out_of_range() {
    let mut w = at(TemplateChoice::AgentSquad);
    w.squad.directory = LineEdit::new("~/code/app");
    w.squad.size = LineEdit::new("101");
    let text = text(&model(w));
    assert!(text.contains("agents must be 1–100"), "{text}");
}

#[test]
fn product_team_directory_errors() {
    let mut w = at(TemplateChoice::ProductTeam);
    w.product.directories[0] = LineEdit::new("/home/me/gone");
    let text = text(&model(w));
    assert!(text.contains("✕ not found"), "{text}");
    assert!(text.contains("✕ required"), "{text}");
}

#[test]
fn engineering_no_rows() {
    let mut w = at(TemplateChoice::EngineeringOrg);
    w.engineering.rows.clear();
    let text = text(&model(w));
    assert!(text.contains("add at least one team"), "{text}");
}

#[test]
fn duplicate_team_name() {
    let mut w = at(TemplateChoice::EngineeringOrg);
    w.engineering.rows = vec![
        TeamRow {
            name: LineEdit::new("Payments"),
            repo: LineEdit::new("~/code/app"),
        },
        TeamRow {
            name: LineEdit::new("Payments"),
            repo: LineEdit::new("~/code/app"),
        },
    ];
    let text = text(&model(w));
    assert!(text.contains("duplicate team name: Payments"), "{text}");
}

#[test]
fn write_failed_keeps_dialog() {
    let mut m = model(at(TemplateChoice::Blank));
    assert!(m.apply(Event::CommandRejected {
        reason: "couldn't write org.toml: disk full".to_owned(),
    }));
    assert!(m.wizard.is_some(), "the wizard stays up after a refusal");
    assert!(text(&m).contains("couldn't write org.toml: disk full"));
}

#[test]
fn no_cli_blocks() {
    let mut m = model(FirstRun::new(no_cli(), None));
    let text = text(&m);
    assert!(
        text.contains("Neither Claude Code nor OpenCode was found"),
        "{text}"
    );
    assert!(text.contains("q Quit"), "{text}");
    assert_eq!(press(&mut m, KeyCode::Char('q')), vec![UiEffect::Quit]);
    shot("blocked_no_cli", FirstRun::new(no_cli(), None));
}

#[test]
fn esc_on_step1_asks_before_quit() {
    let mut m = model(wizard());
    assert!(press(&mut m, KeyCode::Esc).is_empty());
    let w = m.wizard.as_ref().unwrap();
    assert!(w.ask_quit && w.step == Step::You);
    assert!(text(&m).contains("Quit DuoHive?"));
    // `n` stays.
    assert!(press(&mut m, KeyCode::Char('n')).is_empty());
    assert!(!m.wizard.as_ref().unwrap().ask_quit);
    // `esc` then `y` quits.
    press(&mut m, KeyCode::Esc);
    assert_eq!(press(&mut m, KeyCode::Char('y')), vec![UiEffect::Quit]);
}

#[test]
fn esc_on_later_steps_goes_back() {
    let mut m = model(wizard());
    if let Some(w) = m.wizard.as_mut() {
        w.step = Step::Template;
    }
    press(&mut m, KeyCode::Esc);
    assert_eq!(m.wizard.as_ref().map(|w| w.step), Some(Step::You));
    if let Some(w) = m.wizard.as_mut() {
        w.step = Step::Form;
    }
    press(&mut m, KeyCode::Esc);
    assert_eq!(m.wizard.as_ref().map(|w| w.step), Some(Step::Template));
}

#[test]
fn first_run_creates_org_command() {
    let mut m = model(at(TemplateChoice::AgentSquad));
    {
        let w = m.wizard.as_mut().unwrap();
        w.squad.directory = LineEdit::new("~/code/app");
        w.focus = w.controls() + 1;
    }
    let effects = press(&mut m, KeyCode::Enter);
    assert_eq!(
        effects,
        vec![UiEffect::Send(Command::CreateOrg {
            template: TemplateInput::AgentSquad(SquadInput {
                size: 3,
                directory: "~/code/app".to_owned(),
                cli: CliKind::Claude,
                worktrees: true,
                prompt: Some(STARTER_PROMPTS[0].1.to_owned()),
            }),
            your_name: Some("Duong".to_owned()),
            duo_cli: CliKind::Claude,
        })]
    );
}

#[test]
fn first_run_applies_name_and_duo_cli() {
    let mut m = model(FirstRun::new(both_found(), None));
    type_text(&mut m, "Duong");
    press(&mut m, KeyCode::Down); // the CLI row
    press(&mut m, KeyCode::Right); // OpenCode, the next installed CLI
    let w = m.wizard.as_mut().unwrap();
    assert_eq!(w.duo_cli, CliKind::Opencode);
    w.step = Step::Form;
    w.cursor = TemplateChoice::ALL.len() - 1; // Blank
    w.focus = w.controls() + 1;
    let effects = press(&mut m, KeyCode::Enter);
    assert_eq!(
        effects,
        vec![UiEffect::Send(Command::CreateOrg {
            template: TemplateInput::Blank,
            your_name: Some("Duong".to_owned()),
            duo_cli: CliKind::Opencode,
        })]
    );
}

#[test]
fn squad_directory_kept_as_typed_and_trimmed() {
    let mut m = model(at(TemplateChoice::AgentSquad));
    {
        let w = m.wizard.as_mut().unwrap();
        w.squad.directory = LineEdit::new("~/code/app ");
        w.focus = w.controls() + 1;
    }
    let effects = press(&mut m, KeyCode::Enter);
    let UiEffect::Send(Command::CreateOrg { template, .. }) = &effects[0] else {
        panic!("expected Create org, got {effects:?}");
    };
    let TemplateInput::AgentSquad(input) = template else {
        panic!("expected Agent Squad, got {template:?}");
    };
    assert_eq!(input.directory, "~/code/app");
}

#[test]
fn keys_type_q_in_a_field_but_buttons_still_work() {
    let mut m = model(FirstRun::new(claude_only(), None));
    type_text(&mut m, "q");
    assert_eq!(
        m.wizard.as_ref().map(|w| w.you.text().to_owned()),
        Some("q".to_owned())
    );
    // The Quit button acts when focused.
    let w = m.wizard.as_mut().unwrap();
    w.focus = w.controls() + w.buttons().len() - 1;
    assert_eq!(press(&mut m, KeyCode::Enter), vec![UiEffect::Quit]);
}

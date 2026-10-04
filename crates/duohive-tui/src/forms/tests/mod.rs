//! Forms and the chooser (UI §4.4.1–4.4.2, D11, D14): driven through `Form::handle` and
//! `update`, drawn with `TestBackend`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::cli_kind::CliKind;
use duohive_core::command::{AgentDraft, Command, OrgEdit, RoleDraft, TeamDraft};
use duohive_core::event::{AgentView, Event};
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{DuoConfig, Fields, NodeKey, Org, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::{FakeDirs, example_org, n};
use duohive_core::validate::{DirProbe, DirStatus};
use ratatui::buffer::Buffer;

mod agent;
mod groups;
mod keys;

use super::{Chooser, FieldId, FieldKind, Form, FormOutcome, LineEdit};
use crate::fixtures::{ctx, mins, model_for, render_buffer, status, view};
use crate::input::{Action, UiEffect, map_key, paste, update};
use crate::model::{Dialog, SharedDirs, UiModel};

const HOME: &str = "/home/me";
const ORG_PROMPT: &str = "Ask before you delete anything.";
const BACKEND_PROMPT: &str = "API URLs use hyphens. Run make test before done.";
const OWN_PROMPT: &str = "Owns the billing service; invoices and retries.";

fn home() -> PathBuf {
    PathBuf::from(HOME)
}

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn dirs() -> FakeDirs {
    FakeDirs::new(HashMap::from([
        (PathBuf::from("/home/me/code/billing"), DirStatus::GitRepo),
        (PathBuf::from("/home/me/code/web"), DirStatus::GitRepo),
        (PathBuf::from("/home/me/notes"), DirStatus::Found),
        (PathBuf::from("/home/me/gone"), DirStatus::Missing),
    ]))
}

/// The example org dressed as UI §4.4.1 draws it.
fn org() -> Org {
    let mut o = example_org();
    o.defaults.model = Some("sonnet".into());
    o.defaults.effort = Some("high".into());
    o.defaults.prompt = Some(ORG_PROMPT.into());
    let backend = &mut o.teams[0].roles[0];
    backend.defaults.prompt = Some(BACKEND_PROMPT.into());
    let billing = &mut backend.agents[1];
    billing.fields.directory = Some("~/code/billing".into());
    billing.fields.prompt = Some(OWN_PROMPT.into());
    o
}

fn backend() -> RoleKey {
    RoleKey {
        team: Some(nn("Payments")),
        role: nn("Backend"),
    }
}

fn running() -> AgentView {
    view(status(AgentState::Working, mins(2)))
}

fn stopped() -> AgentView {
    view(status(AgentState::Stopped, mins(60)))
}

fn press(f: &mut Form, org: &Org, a: Action) -> FormOutcome {
    let out = f.handle(a, org);
    f.probe_dir(org, &dirs(), Path::new(HOME));
    out
}

fn typed(f: &mut Form, org: &Org, s: &str) {
    for c in s.chars() {
        assert_eq!(press(f, org, Action::Char(c)), FormOutcome::Continue);
    }
}

/// Empties the focused field.
fn clear(f: &mut Form, org: &Org) {
    press(f, org, Action::End);
    while !f.fields[f.focus].edit.text().is_empty() {
        press(f, org, Action::Backspace);
    }
}

fn text(f: &Form, id: FieldId) -> String {
    f.field(id).unwrap().edit.text().to_owned()
}

fn edit_billing(org: &Org, v: &AgentView) -> Form {
    Form::edit_agent(org, &n("billing-api"), v, &dirs(), Path::new(HOME))
}

/// A model over [`org`] with the fake directories, billing-api running and selected.
fn model() -> UiModel {
    let mut m = model_for(org());
    m.dirs = SharedDirs(Arc::new(dirs()));
    m.home = home();
    m.agents.insert(n("billing-api"), running());
    m.selected = NodeKey::Agent(n("billing-api"));
    m
}

fn form(m: &UiModel) -> &Form {
    match &m.dialog {
        Some(Dialog::Form(f)) => f,
        other => panic!("no form open: {other:?}"),
    }
}

fn form_mut(m: &mut UiModel) -> &mut Form {
    match &mut m.dialog {
        Some(Dialog::Form(f)) => f,
        other => panic!("no form open: {other:?}"),
    }
}

/// Cells `x0..x1` of row `y`, one string per row.
fn rows(buf: &Buffer, x0: u16, x1: u16) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| (x0..x1).map(|x| buf[(x, y)].symbol()).collect())
        .collect()
}

/// The open dialog's box, drawn on a 100×30 screen.
fn box_rows(m: &UiModel, w: u16) -> Vec<String> {
    let buf = render_buffer(m, &ctx(), 100, 30);
    let x0 = (100 - w) / 2;
    rows(&buf, x0, x0 + w)
        .into_iter()
        .skip_while(|r| !r.starts_with('┌'))
        .take_while(|r| !r.starts_with('└'))
        .collect()
}

fn screen_text(m: &UiModel) -> String {
    let buf = render_buffer(m, &ctx(), 100, 30);
    rows(&buf, 0, 100).join("\n")
}

/// The mock-up starting `┌─ <title>` in UI §4.4.1, without its bottom border.
fn mock(title: &str) -> Vec<String> {
    include_str!("../../../../../docs/ux/ui-spec.md")
        .lines()
        .skip_while(|l| !l.starts_with(&format!("┌─ {title}")))
        .take_while(|l| !l.starts_with('└'))
        .map(str::to_owned)
        .collect()
}

/// Every mock-up row not named in `except` appears, in order, among the drawn rows.
fn follows_mock(got: &[String], mock: &[String], except: &[&str]) {
    assert!(mock.len() > 5, "mock-up not found");
    let got: Vec<String> = got.iter().map(|g| g.replace('▌', " ")).collect();
    let mut at = 0;
    for want in mock
        .iter()
        .filter(|w| !except.iter().any(|e| w.contains(e)))
    {
        let found = got[at..].iter().position(|g| g == want);
        assert!(found.is_some(), "missing {want:?} in {got:#?}");
        at += found.unwrap_or(0) + 1;
    }
}

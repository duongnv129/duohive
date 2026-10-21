//! Mouse parity (UI §4.4.2, §5): the right-click menu, dragging nodes and the chat edge,
//! header counts, and a click on each screen's buttons and rows.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use duohive_core::command::{Command, MoveTarget, OrgEdit};
use duohive_core::org::{NodeKey, RoleKey};
use duohive_core::testkit::n;
use ratatui::layout::Rect;

use super::mouse::on_mouse;
use super::mouse_tests::{areas, cell_of, model, mouse, press};
use super::move_tests::spec_org;
use super::{Action, UiEffect, map_key, update};
use crate::filter::Filter;
use crate::fixtures::{ctx, model_for, render_lines, spec_worktrees};
use crate::focus::Pane;
use crate::keymap::{ActionId, KeysState};
use crate::model::{Dialog, UiModel};

const W: u16 = 120;
const H: u16 = 36;

fn right(x: u16, y: u16) -> crossterm::event::MouseEvent {
    mouse(MouseEventKind::Down(MouseButton::Right), x, y)
}

fn drag_to(x: u16, y: u16) -> crossterm::event::MouseEvent {
    mouse(MouseEventKind::Drag(MouseButton::Left), x, y)
}

fn release(x: u16, y: u16) -> crossterm::event::MouseEvent {
    mouse(MouseEventKind::Up(MouseButton::Left), x, y)
}

fn screen() -> Rect {
    Rect::new(0, 0, W, H)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn spec_model_on_screen() -> UiModel {
    let mut m = model_for(spec_org());
    m.screen = screen();
    m
}

fn open_menu(m: &mut UiModel, text: &str) {
    let (x, y) = cell_of(m, areas(m).org, text);
    on_mouse(m, right(x, y));
    assert!(m.menu.is_some(), "no menu on {text}");
}

fn labels(m: &UiModel) -> Vec<String> {
    m.menu
        .as_ref()
        .map(|menu| menu.rows.iter().map(|r| r.label.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn right_click_opens_menu_and_drag_moves_node() {
    let mut m = spec_model_on_screen();
    open_menu(&mut m, "pay-qa");
    assert_eq!(m.selected, NodeKey::Agent(n("pay-qa")));
    m.menu = None;
    // Press on pay-qa, drag onto the Generalist role, release: Move to… asks to confirm.
    let (x, y) = cell_of(&m, areas(&m).org, "pay-qa");
    on_mouse(&mut m, press(x, y));
    let (gx, gy) = cell_of(&m, areas(&m).org, "Generalist");
    on_mouse(&mut m, drag_to(gx, gy));
    assert!(
        m.drag
            .as_ref()
            .and_then(|d| d.over.as_ref())
            .is_some_and(|t| t.valid)
    );
    assert_eq!(on_mouse(&mut m, release(gx, gy)), []);
    assert!(
        matches!(m.dialog, Some(Dialog::MoveTo(_))),
        "inherited values change"
    );
}

#[test]
fn right_click_menu_lists_every_action_with_its_key() {
    let mut m = model();
    open_menu(&mut m, "billing-api");
    let menu = m.menu.as_ref().unwrap();
    let keys: Vec<(&str, &str)> = menu
        .rows
        .iter()
        .map(|r| (r.label.as_str(), r.key.as_str()))
        .collect();
    assert_eq!(
        keys,
        [
            ("Give work", "space"),
            ("Stop", "s"),
            ("Edit", "e"),
            ("New here…", "n"),
            ("Move to…", "m"),
            ("Delete", "d"),
            ("Zoom", "z"),
            ("Worktrees", "w"),
            ("Hide replies", "v"),
        ]
    );
}

#[test]
fn menu_disabled_row_gives_the_reason() {
    let mut m = model();
    open_menu(&mut m, "billing-api");
    let delete = m
        .menu
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|r| r.action == ActionId::Delete)
        .unwrap();
    assert_eq!(
        m.menu.as_ref().unwrap().rows[delete].disabled.as_deref(),
        Some("stop it first")
    );
    assert_eq!(super::menu::choose(&mut m, delete), []);
    assert_eq!(m.footer_note.as_deref(), Some("Delete · stop it first"));
    assert!(m.menu.is_none() && m.dialog.is_none());
}

#[test]
fn menu_stays_on_screen_at_the_edges() {
    let mut m = model();
    open_menu(&mut m, "billing-api");
    let mut menu = m.menu.clone().unwrap();
    for at in [(0, 0), (W - 1, H - 1), (W - 1, 0), (0, H - 1)] {
        menu.at = ratatui::layout::Position::new(at.0, at.1);
        let r = crate::render::menu::rect(screen(), &menu);
        assert!(r.x + r.width <= W && r.y + r.height <= H, "{r:?} at {at:?}");
    }
}

#[test]
fn menu_closes_on_esc_click_elsewhere_and_action() {
    let mut m = model();
    open_menu(&mut m, "billing-api");
    let esc = map_key(&m, key(KeyCode::Esc)).unwrap();
    update(&mut m, esc);
    assert!(m.menu.is_none());
    open_menu(&mut m, "billing-api");
    on_mouse(&mut m, press(W - 2, 1));
    assert!(m.menu.is_none());
    open_menu(&mut m, "billing-api");
    // A row runs its action on the node: Edit opens the agent's form.
    let at = labels(&m).iter().position(|l| l == "Edit").unwrap();
    let r = crate::render::menu::rect(screen(), m.menu.as_ref().unwrap());
    on_mouse(&mut m, press(r.x + 2, r.y + 1 + u16::try_from(at).unwrap()));
    assert!(m.menu.is_none());
    assert!(matches!(m.dialog, Some(Dialog::Form(_))));
}

#[test]
fn team_role_duo_menus_differ() {
    let mut m = model();
    open_menu(&mut m, "Payments");
    let team = labels(&m);
    assert!(team.contains(&"Move to…".to_owned()) && !team.contains(&"Give work".to_owned()));
    m.menu = None;
    open_menu(&mut m, "Backend");
    assert!(labels(&m).contains(&"Give work".to_owned()));
    m.menu = None;
    open_menu(&mut m, "Duo");
    assert!(labels(&m).contains(&"Edit org".to_owned()));
    assert!(!labels(&m).contains(&"Delete".to_owned()));
}

#[test]
fn drag_highlights_valid_targets() {
    let mut m = spec_model_on_screen();
    let (x, y) = cell_of(&m, areas(&m).org, "pay-qa");
    on_mouse(&mut m, press(x, y));
    let (bx, by) = cell_of(&m, areas(&m).org, "Backend");
    on_mouse(&mut m, drag_to(bx, by));
    let text = render_lines(&m, &ctx(), W, H).join("\n");
    assert!(
        text.contains('▹'),
        "valid targets carry a marker, not only a colour"
    );
}

#[test]
fn invalid_drop_refused_with_reason() {
    let mut m = spec_model_on_screen();
    let (x, y) = cell_of(&m, areas(&m).org, "pay-qa");
    on_mouse(&mut m, press(x, y));
    let (tx, ty) = cell_of(&m, areas(&m).org, "Payments");
    on_mouse(&mut m, drag_to(tx, ty));
    assert_eq!(on_mouse(&mut m, release(tx, ty)), []);
    assert_eq!(
        m.footer_note.as_deref(),
        Some("an agent moves only into a role")
    );
}

#[test]
fn drop_without_inherited_change_moves_at_once() {
    let mut m = model();
    let (x, y) = cell_of(&m, areas(&m).org, "squad-1");
    on_mouse(&mut m, press(x, y));
    let (bx, by) = cell_of(&m, areas(&m).org, "Backend");
    on_mouse(&mut m, drag_to(bx, by));
    let out = on_mouse(&mut m, release(bx, by));
    assert_eq!(
        out,
        [UiEffect::Send(Command::EditOrg(OrgEdit::Move {
            node: NodeKey::Agent(n("squad-1")),
            to: MoveTarget::Into(NodeKey::Role(RoleKey {
                team: Some(duohive_core::names::NodeName::parse("Payments").unwrap()),
                role: duohive_core::names::NodeName::parse("Backend").unwrap(),
            })),
        }))]
    );
}

#[test]
fn drag_chat_edge_resizes() {
    let mut m = model();
    let chat = areas(&m).chat;
    on_mouse(&mut m, press(chat.x + 5, chat.y));
    on_mouse(&mut m, drag_to(chat.x + 5, chat.y - 2));
    assert_eq!(m.chat_rows_extra, 2);
    on_mouse(&mut m, drag_to(chat.x + 5, chat.y + 5));
    assert_eq!(m.chat_rows_extra, 0);
    on_mouse(&mut m, release(chat.x + 5, chat.y));
    assert!(m.edge_drag.is_none());
}

#[test]
fn header_count_click_sets_filter_for_two_states() {
    let mut m = model();
    let find = |m: &UiModel, text: &str| cell_of(m, Rect::new(0, 0, W, 1), text);
    let (x, y) = find(&m, "working");
    on_mouse(&mut m, press(x, y));
    assert_eq!(m.filter, Filter::Working);
    on_mouse(&mut m, press(x, y));
    assert_eq!(m.filter, Filter::Off, "a second click clears it");
    let (x, y) = find(&m, "need you");
    on_mouse(&mut m, press(x, y));
    assert_eq!(m.filter, Filter::NeedYou);
}

#[test]
fn typing_mode_right_click_and_drag_ignored() {
    let mut m = model();
    let (x, y) = cell_of(&m, areas(&m).org, "billing-api");
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = crate::focus::AgentMode::Typing;
    let before = m.clone();
    assert_eq!(on_mouse(&mut m, right(x, y)), []);
    assert_eq!(on_mouse(&mut m, drag_to(x, y)), []);
    assert!(m == before);
}

fn stop_duo(m: &UiModel) -> Command {
    Command::StopAgent {
        agent: m.org.duo.name.clone(),
    }
}

fn click_text(m: &mut UiModel, text: &str) -> Vec<UiEffect> {
    let (x, y) = cell_of(m, screen(), text);
    on_mouse(m, press(x, y))
}

#[test]
fn click_dialog_buttons() {
    let mut m = model();
    m.dialog = Some(super::confirm(
        "Stop Duo?",
        Vec::new(),
        "Stop",
        stop_duo(&m),
    ));
    let want = stop_duo(&m);
    assert_eq!(click_text(&mut m, "[ n Cancel ]"), []);
    assert!(m.dialog.is_none());
    m.dialog = Some(super::confirm(
        "Stop Duo?",
        Vec::new(),
        "Stop",
        want.clone(),
    ));
    assert_eq!(click_text(&mut m, "[ y Stop ]"), [UiEffect::Send(want)]);
}

#[test]
fn click_form_buttons() {
    let mut m = model();
    update(&mut m, Action::Edit);
    assert!(matches!(m.dialog, Some(Dialog::Form(_))));
    assert_eq!(click_text(&mut m, "[ Cancel ]"), []);
    assert!(m.dialog.is_none());
    update(&mut m, Action::Edit);
    let out = click_text(&mut m, "[ Save ]");
    assert!(
        matches!(out.as_slice(), [UiEffect::Send(Command::EditOrg(_))]),
        "{out:?}"
    );
}

#[test]
fn click_worktree_rows() {
    let mut m = model();
    m.worktrees = spec_worktrees();
    update(&mut m, Action::Worktrees);
    click_text(&mut m, "squad-2");
    assert!(m.dialog.is_none());
    assert_eq!(m.selected, NodeKey::Agent(n("squad-2")));
}

#[test]
fn click_settings_rows() {
    let mut m = model();
    update(&mut m, Action::Settings);
    let out = click_text(&mut m, "needs you");
    assert!(
        matches!(out.as_slice(), [UiEffect::Send(Command::UpdateSettings(s))] if !s.alerts.needs_you),
        "{out:?}"
    );
}

#[test]
fn click_move_to_rows() {
    let mut m = spec_model_on_screen();
    m.selected = NodeKey::Agent(n("pay-qa"));
    update(&mut m, Action::Move);
    // The picker's own row, not the tree's behind it.
    let (x, y) = cell_of(&m, Rect::new(30, 0, W - 30, H), "Generalist");
    let out = on_mouse(&mut m, press(x, y));
    assert!(
        matches!(
            out.as_slice(),
            [UiEffect::Send(Command::EditOrg(OrgEdit::Move { .. }))]
        ),
        "{out:?}"
    );
}

#[test]
fn click_picker_rows() {
    let mut m = model();
    m.org.defaults.model = Some("sonnet".to_owned());
    m.models.insert(
        duohive_core::cli_kind::CliKind::Claude,
        Ok(duohive_core::adapter::ModelList {
            models: vec!["sonnet".into(), "opus".into()],
            ..Default::default()
        }),
    );
    update(&mut m, Action::Edit);
    let (x, y) = cell_of(&m, screen(), "inherit: sonnet");
    on_mouse(&mut m, press(x, y));
    let Some(Dialog::Form(f)) = &m.dialog else {
        panic!("form closed");
    };
    assert!(f.field(crate::forms::FieldId::Model).unwrap().drop.open);
    click_text(&mut m, "opus");
    let Some(Dialog::Form(f)) = &m.dialog else {
        panic!("form closed");
    };
    assert_eq!(
        f.field(crate::forms::FieldId::Model)
            .unwrap()
            .value()
            .as_deref(),
        Some("opus")
    );
}

#[test]
fn click_key_binding_rows() {
    let mut m = model();
    m.dialog = Some(Dialog::Keys(KeysState::default()));
    click_text(&mut m, "Zoom");
    let Some(Dialog::Keys(k)) = &m.dialog else {
        panic!("editor closed");
    };
    assert!(k.capturing);
    assert_eq!(ActionId::ALL[k.cursor], ActionId::Zoom);
}

fn found() -> Vec<(
    duohive_core::cli_kind::CliKind,
    duohive_core::adapter::CliPresence,
)> {
    duohive_core::cli_kind::CliKind::ALL
        .into_iter()
        .map(|k| {
            (
                k,
                duohive_core::adapter::CliPresence::Found {
                    version: "1.0.0".into(),
                },
            )
        })
        .collect()
}

#[test]
fn click_first_run_steps() {
    let mut m = model();
    m.wizard = Some(crate::first_run::FirstRun::new(found(), None));
    click_text(&mut m, "[ Next → ]");
    assert_eq!(
        m.wizard.as_ref().unwrap().step,
        crate::first_run::Step::Template
    );
    click_text(&mut m, "Product Team");
    assert_eq!(m.wizard.as_ref().unwrap().cursor, 1);
    click_text(&mut m, "[ Next → ]");
    assert_eq!(
        m.wizard.as_ref().unwrap().step,
        crate::first_run::Step::Form
    );
}

#[test]
fn click_replace_dialog_buttons() {
    let mut m = model();
    m.wizard = Some(crate::first_run::FirstRun::replace(Vec::new(), None));
    click_text(&mut m, "Blank");
    click_text(&mut m, "[ Next → ]");
    assert_eq!(
        m.wizard.as_ref().unwrap().step,
        crate::first_run::Step::Form
    );
    click_text(&mut m, "[ Replace org ]");
    assert!(matches!(m.dialog, Some(Dialog::Confirm(_))));
    assert_eq!(click_text(&mut m, "[ n Cancel ]"), []);
    assert!(m.dialog.is_none() && m.wizard.is_some());
    click_text(&mut m, "[ Replace org ]");
    let out = click_text(&mut m, "[ y Replace ]");
    assert!(
        matches!(
            out.as_slice(),
            [UiEffect::Send(Command::NewOrgFromTemplate { .. })]
        ),
        "{out:?}"
    );
}

#[test]
fn every_action_has_a_click_path() {
    // The menu lists these for an agent; the rest have a screen control or a header count.
    let mut m = model();
    open_menu(&mut m, "billing-api");
    let in_menu: Vec<ActionId> = m
        .menu
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .map(|r| r.action)
        .collect();
    let elsewhere = [
        ActionId::OpenOrg,
        ActionId::Settings,
        ActionId::Filter,
        ActionId::KeyMap,
        ActionId::CardApprove,
        ActionId::CardEdit,
        ActionId::CardReject,
    ];
    for a in ActionId::ALL {
        assert!(
            in_menu.contains(&a) || elsewhere.contains(&a),
            "{a:?} has no click path"
        );
    }
}

#[test]
fn no_color_menu_and_drag_readable() {
    let mut m = spec_model_on_screen();
    open_menu(&mut m, "pay-qa");
    let plain = crate::render::RenderCtx {
        palette: crate::theme::Palette::mono(),
        ..ctx()
    };
    let text = render_lines(&m, &plain, W, H).join("\n");
    assert!(text.contains("Move to…") && text.contains("Delete"));
    m.menu = None;
    let (px, py) = cell_of(&m, areas(&m).org, "pay-qa");
    on_mouse(&mut m, press(px, py));
    let (gx, gy) = cell_of(&m, areas(&m).org, "Generalist");
    on_mouse(&mut m, drag_to(gx, gy));
    let text = render_lines(&m, &plain, W, H).join("\n");
    assert!(text.contains('▹'));
}

#[test]
fn reduce_motion_menu_and_drag() {
    let mut m = spec_model_on_screen();
    open_menu(&mut m, "pay-qa");
    let mut still = ctx();
    still.spinner = 1;
    let a = render_lines(&m, &still, W, H);
    still.spinner = 9;
    assert_eq!(a, render_lines(&m, &still, W, H));
}

//! Keys on a focused approval card (UI §4.6, §4.11; P3-D7, P3-D22): `y` approves, `e`
//! edits where it can, `n` rejects, `esc` returns to the input. Saving a card's form
//! approves the edited change.

use duohive_core::card::{CardChange, CardId};
use duohive_core::command::{Command, OrgEdit};

use super::{Action, UiEffect};
use crate::focus::Pane;
use crate::forms::form_for_card;
use crate::keymap::ActionId;
use crate::model::UiModel;

/// The footer when `e` has nothing to edit (P3-D7).
const NOTHING_TO_EDIT: &str = "nothing to edit · y or n";

/// An action while a card has focus; `None` when it is the input's as usual. A key that
/// types returns focus to the input first.
pub(super) fn card_action(m: &mut UiModel, a: Action) -> Option<Vec<UiEffect>> {
    let id = m.focus.card?;
    Some(match a {
        Action::CardApprove => answer(id, true),
        Action::CardReject => answer(id, false),
        Action::CardEdit => edit(m, id),
        Action::Esc => {
            m.focus.card = None;
            Vec::new()
        }
        Action::Up | Action::Down | Action::Enter => Vec::new(),
        Action::ChatGrow | Action::ChatShrink | Action::PageUp | Action::PageDown => return None,
        _ => {
            m.focus.card = None;
            return None;
        }
    })
}

/// A press on one of a card's buttons: its action.
pub(super) fn press_button(m: &mut UiModel, id: ActionId) -> Vec<UiEffect> {
    let action = match id {
        ActionId::CardApprove => Action::CardApprove,
        ActionId::CardEdit => Action::CardEdit,
        ActionId::CardReject => Action::CardReject,
        _ => return Vec::new(),
    };
    card_action(m, action).unwrap_or_default()
}

fn answer(card: CardId, yes: bool) -> Vec<UiEffect> {
    let command = if yes {
        Command::ApproveCard { card, edited: None }
    } else {
        Command::RejectCard { card }
    };
    vec![UiEffect::Send(command)]
}

/// `e`: the card's form, pre-filled; on a card with nothing to edit, the footer says so.
fn edit(m: &mut UiModel, id: CardId) -> Vec<UiEffect> {
    let Some(card) = m.cards.get(&id) else {
        return Vec::new();
    };
    match form_for_card(card, m) {
        Some(f) => m.open_form(f),
        None => m.footer_note = Some(NOTHING_TO_EDIT.to_owned()),
    }
    Vec::new()
}

/// The card's change with `edit` in it: a new-agent card keeps its task, run and source
/// (P3-D7); any other card holds the edit.
pub(super) fn edited(m: &UiModel, id: CardId, edit: OrgEdit) -> CardChange {
    match (m.cards.get(&id).map(|c| &c.change), edit) {
        (
            Some(CardChange::NewAgent {
                role,
                task,
                run,
                clone_of,
                ..
            }),
            OrgEdit::AddAgent { agent, .. },
        ) => CardChange::NewAgent {
            role: role.clone(),
            agent,
            task: task.clone(),
            run: *run,
            clone_of: clone_of.clone(),
        },
        (_, edit) => CardChange::Org(edit),
    }
}

/// Saving a card's form approves the edited change and returns focus to the chat.
pub(super) fn approve_edited(m: &mut UiModel, id: CardId, edit: OrgEdit) -> Vec<UiEffect> {
    let change = edited(m, id, edit);
    m.focus.pane = Pane::Chat;
    m.focus.card = None;
    vec![UiEffect::Send(Command::ApproveCard {
        card: id,
        edited: Some(Box::new(change)),
    })]
}

/// What a card form's confirmation sends: the edit as the card's approval.
pub(super) fn approve_command(m: &UiModel, id: CardId, on_yes: Command) -> Command {
    match on_yes {
        Command::EditOrg(edit) => Command::ApproveCard {
            card: id,
            edited: Some(Box::new(edited(m, id, edit))),
        },
        other => other,
    }
}

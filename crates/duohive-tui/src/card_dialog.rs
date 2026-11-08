//! The approval card dialog (UI §4.6; CDL-D1 to CDL-D6): a pending card is answered in a
//! centred dialog like a form. The queue is every pending card, oldest first.

use crossterm::event::{KeyCode, KeyEvent};
use duohive_core::card::{CardChange, CardId};
use duohive_core::command::{Command, OrgEdit};

use crate::forms::{card_editable, form_for_card};
use crate::input::{Action, UiEffect};
use crate::keymap::ActionId;
use crate::model::{Dialog, UiModel};

/// The footer when `e` has nothing to edit (P3-D7).
const NOTHING_TO_EDIT: &str = "nothing to edit · y or n";

/// The open card dialog: which card, which button has focus, and whether its answer is
/// on the way (keys then do nothing but `esc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardDialog {
    /// The card shown.
    pub card: CardId,
    /// The focused button, as an index into [`focusable`].
    pub focus: usize,
    /// An answer was sent and the engine has not confirmed it.
    pub sent: bool,
}

/// Every pending card, oldest first by `(raised_at, id)`.
pub(crate) fn pending(m: &UiModel) -> Vec<CardId> {
    let mut cards: Vec<_> = m.cards.values().collect();
    cards.sort_by_key(|c| (c.raised_at, c.id));
    cards.into_iter().map(|c| c.id).collect()
}

/// `(position, total)` of `id` among the pending cards, position from 1.
pub(crate) fn place(m: &UiModel, id: CardId) -> (usize, usize) {
    let all = pending(m);
    let at = all.iter().position(|c| *c == id).map_or(1, |i| i + 1);
    (at, all.len())
}

/// The buttons the arrows and `tab` can reach: `Edit` only where the card has a form.
pub(crate) fn focusable(m: &UiModel, id: CardId) -> Vec<ActionId> {
    let editable = m.cards.get(&id).is_some_and(card_editable);
    [
        ActionId::CardApprove,
        ActionId::CardEdit,
        ActionId::CardReject,
    ]
    .into_iter()
    .filter(|a| editable || *a != ActionId::CardEdit)
    .collect()
}

/// Opens the dialog on `id`, focus on `Reject` (dialogs open on the safe button, CDL-D3);
/// `false` when no such card is pending.
pub(crate) fn open(m: &mut UiModel, id: CardId) -> bool {
    if !m.cards.contains_key(&id) {
        return false;
    }
    let focus = focusable(m, id).len().saturating_sub(1);
    m.dialog = Some(Dialog::Card(CardDialog {
        card: id,
        focus,
        sent: false,
    }));
    true
}

/// The oldest pending card takes the dialog's place, or the dialog closes (CDL-D2, CDL-D5).
fn next(m: &mut UiModel) {
    m.dialog = None;
    if let Some(id) = pending(m).first().copied() {
        open(m, id);
    }
}

/// `answered` left the pending cards: if the dialog showed it, it moves on.
pub(crate) fn advance(m: &mut UiModel, answered: CardId) {
    if matches!(&m.dialog, Some(Dialog::Card(d)) if d.card == answered) {
        next(m);
    }
}

/// A card the dialog shows that is no longer pending (a reconnect's snapshot) moves on.
pub(crate) fn keep_valid(m: &mut UiModel) {
    if let Some(Dialog::Card(d)) = &m.dialog
        && !m.cards.contains_key(&d.card)
    {
        next(m);
    }
}

/// The key `k` asks for in the dialog.
pub(crate) fn map_key(m: &UiModel, k: KeyEvent) -> Option<Action> {
    if let Some(id) = m.keymap.card_action(&k) {
        return Some(match id {
            ActionId::CardApprove => Action::CardApprove,
            ActionId::CardEdit => Action::CardEdit,
            _ => Action::CardReject,
        });
    }
    match k.code {
        KeyCode::Up => Some(Action::Up),
        KeyCode::Down => Some(Action::Down),
        KeyCode::Left => Some(Action::Left),
        KeyCode::Right => Some(Action::Right),
        KeyCode::Enter => Some(Action::DialogEnter),
        _ => None,
    }
}

/// A key in the dialog.
pub(crate) fn action(m: &mut UiModel, mut d: CardDialog, a: Action) -> Vec<UiEffect> {
    m.dialog = Some(Dialog::Card(d));
    if a == Action::Esc {
        m.dialog = None;
        return Vec::new();
    }
    if d.sent || !m.cards.contains_key(&d.card) {
        return Vec::new();
    }
    let buttons = focusable(m, d.card);
    let n = buttons.len().max(1);
    match a {
        Action::Left | Action::Up | Action::BackTab => d.focus = (d.focus + n - 1) % n,
        Action::Right | Action::Down | Action::Tab => d.focus = (d.focus + 1) % n,
        Action::DialogEnter => {
            let id = buttons
                .get(d.focus)
                .copied()
                .unwrap_or(ActionId::CardReject);
            return press(m, d, id);
        }
        Action::CardApprove => return press(m, d, ActionId::CardApprove),
        Action::CardEdit => return press(m, d, ActionId::CardEdit),
        Action::CardReject => return press(m, d, ActionId::CardReject),
        _ => {}
    }
    m.dialog = Some(Dialog::Card(d));
    Vec::new()
}

/// Presses `button`: approve and reject send the answer and wait for it; edit opens the form.
fn press(m: &mut UiModel, mut d: CardDialog, button: ActionId) -> Vec<UiEffect> {
    match button {
        ActionId::CardEdit => {
            match m.cards.get(&d.card).and_then(|c| form_for_card(c, m)) {
                Some(f) => m.open_form(f),
                None => m.footer_note = Some(NOTHING_TO_EDIT.to_owned()),
            }
            Vec::new()
        }
        ActionId::CardApprove | ActionId::CardReject => {
            d.sent = true;
            m.dialog = Some(Dialog::Card(d));
            let card = d.card;
            vec![UiEffect::Send(if button == ActionId::CardApprove {
                Command::ApproveCard { card, edited: None }
            } else {
                Command::RejectCard { card }
            })]
        }
        _ => Vec::new(),
    }
}

/// The card's change with `edit` in it: a new-agent card keeps its task, run and source
/// (P3-D7); any other card holds the edit.
pub(crate) fn edited(m: &UiModel, id: CardId, edit: OrgEdit) -> CardChange {
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

/// Saving a card's form approves the edited change; the dialog waits for the answer and
/// then shows the next card.
pub(crate) fn approve_edited(m: &mut UiModel, id: CardId, edit: OrgEdit) -> Vec<UiEffect> {
    let change = edited(m, id, edit);
    m.dialog = Some(Dialog::Card(CardDialog {
        card: id,
        focus: 0,
        sent: true,
    }));
    vec![UiEffect::Send(Command::ApproveCard {
        card: id,
        edited: Some(Box::new(change)),
    })]
}

/// What a card form's confirmation sends: the edit as the card's approval.
pub(crate) fn approve_command(m: &UiModel, id: CardId, on_yes: Command) -> Command {
    match on_yes {
        Command::EditOrg(edit) => Command::ApproveCard {
            card: id,
            edited: Some(Box::new(edited(m, id, edit))),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests;

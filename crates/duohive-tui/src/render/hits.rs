//! What a click on a dialog does (UI §5, mouse parity): the renderers record the cells of
//! their buttons and rows with the keys that do the same thing, and a click replays them.
//! Recording happens only in [`keys_at`]'s off-screen draw, so a real frame records nothing.

use std::cell::{Cell, RefCell};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Position, Rect};

use super::{RenderCtx, render};
use crate::input::Action;
use crate::model::UiModel;
use crate::theme::Palette;

thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
    static HITS: RefCell<Vec<(Rect, Vec<Action>)>> = const { RefCell::new(Vec::new()) };
}

/// Notes that a click inside `area` presses `keys`.
pub(super) fn record(area: Rect, keys: Vec<Action>) {
    if ON.with(Cell::get) {
        HITS.with(|h| h.borrow_mut().push((area, keys)));
    }
}

/// `n` presses of `step`, then `enter`: a click on a list row, `delta` rows from the cursor.
pub(super) fn row_keys(delta: isize) -> Vec<Action> {
    let step = if delta < 0 { Action::Up } else { Action::Down };
    let mut keys = vec![step; delta.unsigned_abs()];
    keys.push(Action::DialogEnter);
    keys
}

/// The keys a click at `at` stands for, as the screen draws now; the last thing drawn there
/// wins, so a dialog over the tree takes the click. `None` over nothing clickable.
pub(crate) fn keys_at(m: &UiModel, at: Position) -> Option<Vec<Action>> {
    let mut term = Terminal::new(TestBackend::new(m.screen.width, m.screen.height)).ok()?;
    let ctx = RenderCtx {
        now: m.now,
        spinner: 0,
        palette: Palette::mono(),
        settings: m.settings.clone(),
        utc_offset_secs: 0,
    };
    HITS.with(|h| h.borrow_mut().clear());
    ON.with(|o| o.set(true));
    let drawn = term.draw(|f| render(f, m, &ctx));
    ON.with(|o| o.set(false));
    drawn.ok()?;
    HITS.with(|h| {
        h.borrow_mut()
            .drain(..)
            .rev()
            .find(|(area, _)| area.contains(at))
            .map(|(_, keys)| keys)
    })
}

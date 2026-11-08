# Card dialog — Implementation Plan

**Status:** Plan, 2026-11-08. Supersedes [the docked cards plan](card-dock-plan.md) (CD-D1 to CD-D11) for where a card is shown; its queue, height-cut and "not drawn, no keys" ideas are re-decided below.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox syntax.

**Goal:** A pending approval card is answered in a centred dialog drawn like a form or confirmation, and `↑↓` / `←→` move between items in every form, confirmation and card.

**Architecture:** `Dialog::Card(CardDialog)` joins the dialog enum; it takes every key as a form does. It is drawn with the Confirm dialog's frame and button style, and records its buttons with `hits::record`, so clicks need no special case. The queue is derived from `UiModel::cards` (oldest first). The per-agent dock, `focus.card` and the agent-pane card hit-test are deleted. `duohive-core` is untouched.

**Specs:** [UI §4.6 and §4.4.1 *Moving*](../ux/ui-spec.md) · [PRD *Approval cards*](../product/prd.md) · [TD *Card dialog*](../design/technical-design.md). Approved by the author 2026-11-08 (option A).

## Global Constraints

[AGENTS.md](../../AGENTS.md) and [Phase 0's](phase-0-plan.md#global-constraints): `make check` before every commit; conventional commits, no `Co-Authored-By`, never push; no `unwrap`/`expect` outside tests; test first; files under 800 lines, functions under 50; only `duohive-tui` changes.

## Decisions

| # | Decision |
|---|---|
| CDL-D1 | A card never opens by itself; it opens from its Needs you row (`enter` or click). Selection and agent pane do not change. |
| CDL-D2 | Queue = all pending cards by `(raised_at, id)`; the title shows `· k of N` when N > 1. Answering shows the next oldest, or closes. |
| CDL-D3 | Focus starts on `Reject` (dialogs open on Cancel). `Edit` is absent on a card with nothing to edit, so arrows skip it. |
| CDL-D4 | `esc` closes the dialog; all cards stay pending. `esc` in a card's edit form returns to the card. |
| CDL-D5 | If the open card is answered elsewhere (an `Event::CardAnswered`), the dialog shows the next card or closes. |
| CDL-D6 | `↑↓` and `tab` move focus in a form (not inside an open dropdown, where `↑↓` choose); `←→` move between `Save` and `Cancel` when one has focus. Confirmations and the card use `↑↓` `←→` `tab` the same way. |

## Review Focus

1. The card the dialog shows is answered from elsewhere, or its queue empties: the dialog moves on or closes, never shows a stale card (CDL.2).
2. A stop card (no Edit): `e` does nothing but the footer note, arrows skip the dimmed button (CDL.2).
3. A dropdown popup is open: `↑↓` choose in it and do not leave the field (CDL.1).
4. A text field has focus: `←→` still move the cursor (CDL.1).
5. Reconnecting or an empty org with a pending card: the Needs you row still opens it (CDL.2).

---

### Task CDL.1: `↑↓` and `←→` between items

**Files:** modify `input.rs` (`form_key`, `confirm_action`), `forms/mod.rs` (`Form::handle`); tests in `forms/tests/` and `input/tests.rs`.

- [ ] Tests, red: `form_up_down_move_between_fields`, `form_left_right_on_save_cancel_row_moves_between_buttons`, `form_left_right_in_text_field_moves_cursor_only`, `open_dropdown_up_down_choose_not_leave`, `confirm_arrows_toggle_focus`.
- [ ] `form_key` maps `Up` / `Down` to `Action::Up` / `Action::Down`. `Form::handle`: `Up`/`Down` call `step(false/true)`; `Left`/`Right` call `step` only when `on_save()` or `on_cancel()` (before the field's own `apply`).
- [ ] `confirm_action`: `Up`, `Down`, `Left`, `Right` toggle `focus_yes`.
- [ ] Update the form and confirm footers' hints (`render/form.rs` `HINT`) to name `↑↓`. Green; commit `feat: move between form items with the arrow keys`.

### Task CDL.2: The card dialog

**Files:** create `card_dialog.rs` (model + queue: `CardDialog { card: CardId, focus: usize }`, `pending(m) -> Vec<CardId>`, `open(m, id)`, `action(m, d, a)`), `render/card_dialog.rs` (draw), tests beside each; modify `model.rs` (`Dialog::Card`, `CardAnswered` advances), `input.rs` (route, `form_action` Cancel returns to the card), `input/panes.rs` (`needs_you_enter` opens), `render/dialog.rs` (dispatch), `render/header.rs` (footer).

**Interfaces:** `render/card.rs` gains `pub(crate) fn card_body(card, m, now, cells, max_rows) -> Vec<Row>` — the existing body-line wrap and cut, without the outline or buttons; `card_rows` is deleted in CDL.3. `buttons(card)` and `card_editable` stay.

- [ ] Tests, red: `enter_on_card_row_opens_dialog_on_reject`, `title_shows_position_of_all_pending`, `arrows_move_between_buttons_and_skip_missing_edit`, `y_approves_and_shows_next`, `last_answer_closes_dialog`, `esc_closes_and_keeps_card_pending`, `e_on_stop_card_notes_nothing_to_edit`, `edit_form_esc_returns_to_card`, `edit_form_save_approves_edited_change`, `answered_elsewhere_advances` (Review Focus 1), `opens_while_reconnecting_and_with_empty_org` (5), `click_on_button_presses_it` (via `hits::keys_at`), and a snapshot of an 8-field card at 80×24 with the buttons visible.
- [ ] Implement as above. Draw with `dialog.rs`'s box helpers (`frame_box` in `render/form.rs`, width 66), body from `card_body`, buttons `[ y Approve ]  [ e Edit ]  [ n Reject ]` with `▌` as Confirm does, each recorded with `Action::CardApprove` / `CardEdit` / `CardReject`.
- [ ] Green; commit `feat: answer approval cards in a centred dialog`.

### Task CDL.3: Remove the dock

**Files:** delete `dock.rs`, `dock/tests.rs`, `render/dock.rs`, `render/dock_tests.rs`, `input/cards.rs` (keep `edited` / `approve_command` in `card_dialog.rs`), `Look`/`card_rows`/`button_at`; modify `focus.rs` (drop `card`), `model.rs`, `input.rs`, `input/mouse.rs`, `render/agent_pane.rs`, `render/mod.rs`, `render/header.rs`, `keymap.rs` (`Context::DockedCard` → `CardDialog`, help text), snapshots.

- [ ] `cargo build` fails on each dangling use; remove them until it builds; drop tests that pinned the dock; keep and port those that pinned card content.
- [ ] `make check`; commit `refactor: remove the docked approval card`.

### Task CDL.4: Docs and hand check

- [ ] `docs/plans/status.md` and `docs/README.md` link this plan; mark the dock plan superseded; changelog entry.
- [ ] Hand check (the author): raise a card with the TUI open, open it from Needs you, answer a queue of 3 with `←→` `enter`, edit one, `esc` out, click each button, and resize to 80×24.
- [ ] Commit `docs: card dialog plan and status`.

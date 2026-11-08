# Docked approval cards — Implementation Plan

**Status:** Plan, drafted 2026-11-08 · tasks CD.1 to CD.6 built on the branch `feat/card-dock-impl` (2026-11-08), not merged · CD.7, the hand check, is open (for the author). Built as: removing cards from the chat landed in CD.4 with the focus switch (so no commit has two live card UIs), CD.3 only added the hit test, and CD.5 is mouse only; a button acts on press, so a drag from a button is not a text selection.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A pending approval card is answered on the agent it concerns: it docks over the top of that agent's pane, one card at a time, instead of sitting in the 3-row chat strip where its buttons are cut off.

**Architecture:** `duohive-core` is untouched. The TUI derives each card's dock agent from `Card.change`, keeps a per-agent queue (oldest first), and draws the first card over the top rows of the agent pane (main screen and zoomed, which share `agent_pane::draw`). `focus.card` now means "the docked card has focus" with `focus.pane == Pane::Agent`. The chat stops drawing pending cards.

**Tech Stack:** Rust 1.99.0, Ratatui (pinned), `duohive-tui` only.

**Specs (read first; they already carry the behaviour):** [UI §4.6](../ux/ui-spec.md) (rewritten), §4.1.2, §4.1.3, §4.2, key tables and key map · [PRD *Approval cards*](../product/prd.md) · [TD §6.5 *Docked cards*](../design/technical-design.md). The author approved the design on 2026-11-08 (option A of three: dock over the agent pane; a screen-wide popup and a growing chat were rejected).

## Global Constraints

From [AGENTS.md](../../AGENTS.md) and Phase 0's [Global Constraints](phase-0-plan.md#global-constraints):

- Toolchain 1.99.0, edition 2024. `make check` passes before every commit. Conventional commits; no `Co-Authored-By`; never push.
- No `unwrap`/`expect` outside tests; no `unsafe`. `duohive-core` does no I/O and is not touched here.
- No `std::env::set_var` in tests; tmux tests use a throwaway socket.
- Test first.
- A behaviour change edits the PRD, UI spec and TD in the same commit. They are already updated; keep them true where a task changes the wording.
- Files under 800 lines, functions under 50 lines (user rules). `input.rs` and `render/card.rs` are already large: put new code in new files.

## Decisions

| # | Decision |
|---|---|
| CD-D1 | **Dock agent** is the agent the change names: a stop's `agent`; an org edit, move or delete of one agent; else Duo. A named agent that is not in the org docks on Duo. Approved 2026-11-08. |
| CD-D2 | **Queue is oldest first** by `(raised_at, id)`, shown as `· 1 of N` when N > 1. Approved 2026-11-08 (author: "show one by one"). |
| CD-D3 | **Nothing switches the pane when a card is raised.** Cards are reached from Needs you or by clicking the docked card. Approved 2026-11-08. |
| CD-D4 | **Focus passes to the next card** when the focused card is answered and the agent has another, so a queue is answered without clicking again. *Author's call to confirm:* the risk is a repeated `y` approving a card unseen; the next card is drawn in the same frame the answer is applied, so it is on screen before the next key. |
| CD-D5 | **Zoom:** the card is drawn zoomed too, and keeps the focus it had; a click does nothing while zoomed (§4.1.2), so a card with no focus can't be answered until `z` restores. In UI §4.6 *Zoomed*. |
| CD-D6 | **`esc` on a focused card** clears `focus.card` and moves focus to the org pane; the card stays docked (UI §4.6). |
| CD-D7 | **Height cap:** at most `max(inner_rows / 2, 5)` rows; below 7 inner rows no card is drawn (the layout's smallest agent pane at 80×24 has more; CD.2 pins it). Rows past the cap are replaced by one `… N more rows` row; a `flags` row is never cut. |
| CD-D8 | **A click on a docked card** focuses it, leaves Typing (as any click outside the typed pane's text does) and, on `[y]` `[e]` `[n]`, acts as the key. It never starts click-to-type. |

## Review Focus

Most likely to go wrong first; each has a test in the owning task.

1. A card whose agent was removed, or that names no agent, is unreachable (CD.1: docks on Duo).
2. A card is docked while you type to the agent: `y` / `e` / `n` reach the agent, never the card (CD.4).
3. The focused card is answered and the next one is approved by the same key event or never shown (CD.4).
4. An 8-field card at 80×24 hides its buttons, or clicks on them miss (CD.2, CD.3).
5. A team or role is selected, or DuoHive is reconnecting: a card is drawn, takes focus or eats keys with no agent pane (CD.2, CD.4).

---

### Task CD.1: Dock agent and queue

**Files:**
- Create: `crates/duohive-tui/src/dock.rs` (and `dock/tests.rs`, declared `#[cfg(test)] mod tests;` at its end)
- Modify: `crates/duohive-tui/src/lib.rs` (add `mod dock;`)

**Interfaces:**
- Produces:
  - `pub(crate) fn dock_agent(card: &Card, org: &Org) -> AgentName`
  - `pub(crate) struct Dock { pub card: CardId, pub position: usize, pub total: usize }` (`position` is 1-based)
  - `pub(crate) fn dock_for(m: &UiModel, agent: &AgentName) -> Option<Dock>`: the oldest pending card whose `dock_agent` is `agent`.
  - `pub(crate) fn shown_dock(m: &UiModel) -> Option<Dock>`: `dock_for` for the agent pane's agent, `None` while `m.reconnecting > 0`, with an empty org, or with a team or role selected (the rule `agent_pane::shown_agent` uses; make that function `pub(crate)` and call it).

- [ ] **Step 1: Write the failing tests** in `dock/tests.rs`, using the fixtures in `fixtures.rs` and `Card` builders as `cards/tests.rs` in core does:
  - `stop_card_docks_on_its_agent`, `edit_agent_card_docks_on_that_agent`, `delete_agent_card_docks_on_that_agent`
  - `new_agent_and_team_and_role_and_defaults_cards_dock_on_duo`
  - `card_for_an_agent_not_in_the_org_docks_on_duo`
  - `queue_is_oldest_first_with_position_and_total` (three stop cards on one agent, raised out of id order: position 1 is the earliest `raised_at`, ties by id)
  - `shown_dock_is_none_for_team_role_reconnecting_and_empty_org`
- [ ] **Step 2: Run** `cargo test -p duohive-tui dock` — expect FAIL (module not defined).
- [ ] **Step 3: Implement** the four items above. `dock_agent` matches `CardChange::Stop { agent }` and the `OrgEdit` variants that name one agent (`EditAgent { key, .. }`, `Delete(NodeKey::Agent(a))`, `Move` of a `NodeKey::Agent`); everything else is Duo's name (`org.duo.name`).
- [ ] **Step 4: Run** the same command — expect PASS.
- [ ] **Step 5: Commit** `feat: dock agent and queue for approval cards`

### Task CD.2: Draw the docked card

**Files:**
- Create: `crates/duohive-tui/src/render/dock.rs` (+ `dock_tests.rs`)
- Modify: `crates/duohive-tui/src/render/card.rs` (counter in the title, height cap), `render/agent_pane.rs` (call the draw after the terminal or text is drawn), `render/mod.rs` (`mod dock;`)

**Interfaces:**
- Consumes: `dock::shown_dock`, `card::card_rows`, `agent_pane::live_area`.
- Produces:
  - `card::card_rows(...)` gains `queue: Option<(usize, usize)>` (position, total) for the title (`◆ Stop billing-api · 1 of 3`; none when total is 1) and `max_rows: usize` (CD-D7: cut the body, keep the title, the `flags` rows and the button row, and insert `… N more rows`). Update its one caller in `chat_pane.rs` only until CD.3 removes it.
  - `pub(super) fn dock_rect(pane: Rect) -> Rect`: the card's area: the `live_area` top-left, width `min(CARD_WIDTH, live width)`.
  - `pub(super) fn draw(buf: &mut Buffer, pane: Rect, m: &UiModel, ctx: &RenderCtx)`: draws the first row of `shown_dock(m)` with `focused = m.focus.card == Some(id)`; nothing when the pane has fewer than 7 inner rows.

- [ ] **Step 1: Write the failing tests** (`dock_tests.rs`, in the style of `card_tests.rs` and the snapshot tests in `screen_tests.rs`):
  - `dock_covers_top_rows_and_leaves_the_prompt_row`: a stop card on the selected agent at 120×36; the card's rows are the pane's first rows, the pane's last inner row still shows the terminal's.
  - `title_shows_one_of_n_only_when_queued`
  - `eight_field_card_at_80x24_shows_title_buttons_and_more_rows_line`: buttons row present, `… 3 more rows` (assert the exact N), and the pane's inner rows ≥ 7 at the 80×24 layout.
  - `flags_row_is_never_cut`
  - `unfocused_is_light_and_focused_is_heavy`
  - `nothing_drawn_for_a_team_selection_or_while_reconnecting`
  - `zoomed_pane_draws_the_card_too`
  - one insta snapshot: Duo's pane with a new-agent card (matches the UI §4.6 mock).
- [ ] **Step 2: Run** `cargo test -p duohive-tui render::dock` — expect FAIL.
- [ ] **Step 3: Implement** as the Interfaces block says. Draw after the live terminal render and after the `Paragraph` fallback so the card is on top; the terminal is not resized.
- [ ] **Step 4: Run** the same command and `cargo test -p duohive-tui` — expect PASS (accept new snapshots with `cargo insta review`; existing chat snapshots are still unchanged at this point).
- [ ] **Step 5: Commit** `feat: draw the docked approval card over the agent pane`

### Task CD.3: Hit-test the dock; the chat stops drawing cards

**Files:**
- Modify: `render/dock.rs`, `render/mod.rs` (replace `chat_card_at` and `chat_card_span`), `render/chat_pane.rs` (remove `Item::Card`, `items`' card merge, `card_rows`, `card_indent`, `card_at`, `card_spans`; keep the entries), `render/card.rs` (drop the chat-only parameters if unused)
- Test: `render/dock_tests.rs`, `render/chat_tests.rs`, `render/card_tests.rs`

**Interfaces:**
- Produces: `pub(crate) fn agent_card_at(m: &UiModel, at: Position) -> Option<(CardId, Option<ActionId>)>` in `render/mod.rs`: the docked card under `at` in the agent pane (zoomed pane too) and the button under it (`card::button_at`, with the button row being the card's last row but one).

- [ ] **Step 1: Write the failing tests:**
  - `click_on_each_button_hits_it` (`[y]`, `[e]`, `[n]`; no `[e]` on a stop card), `click_on_the_card_body_hits_no_button`, `click_outside_the_card_hits_nothing`, `button_hit_survives_the_height_cap` (the 8-field card at 80×24)
  - `chat_shows_no_pending_card_but_keeps_result_lines` (a pending card plus a `CardResult` entry: only the `→ approved: …` line is in the chat rows)
- [ ] **Step 2: Run** `cargo test -p duohive-tui` — expect the new tests FAIL and the old chat-card tests (`chat_tests.rs`, `card_tests.rs`, snapshots) still pass.
- [ ] **Step 3: Implement** `agent_card_at`; delete the chat-card code and its tests and snapshots that only existed for chat cards; fix `UiModel`'s `chat_history` uses that counted cards.
- [ ] **Step 4: Run** `cargo test -p duohive-tui` — expect PASS.
- [ ] **Step 5: Commit** `refactor: pending cards leave the chat; hit-test them in the agent pane`

### Task CD.4: Focus and keys

**Files:**
- Modify: `dock.rs` (focus), `chat.rs` (remove `focus_card` and the `↑` newest-card path at chat.rs:135), `input.rs` (`map_key` routing at input.rs:200 and the post-apply `focus.card` reset at input.rs:403, `apply` at input.rs:485), `input/cards.rs`, `input/panes.rs` (`needs_you_enter`), `focus.rs` (doc of `Focus.card`), `model.rs` (`CardAnswered` and snapshot handling at model.rs:386-393 and 732), `keymap.rs` (`Context`: add `DockedCard`; card actions at keymap.rs:490; conflict rules), `render/help.rs` and `render/settings.rs` where they name the Chat context
- Test: `input/cards/tests.rs` (or the existing card tests in `input`), `keymap` tests, `model` tests

**Interfaces:**
- Consumes: `dock::dock_for`, `UiModel::reveal_agent`.
- Produces: `pub(crate) fn focus_card(m: &mut UiModel, id: CardId) -> Option<AgentName>` in `dock.rs`: selects the card's dock agent (`reveal_agent`), sets `focus.pane = Pane::Agent`, `agent_mode = Selected`, `focus.card = Some(<oldest card of that agent>)`, and returns the agent for the `ViewAgent` effect. `panes::needs_you_enter` returns that agent for a card row (it returns `None` today).

- [ ] **Step 1: Write the failing tests:**
  - `enter_on_a_card_row_selects_its_agent_and_focuses_the_oldest_card` (a later card's row focuses the agent's oldest card; effect is `ViewAgent`)
  - `y_e_n_act_only_while_the_card_has_focus_and_the_agent_pane_is_selected_mode`
  - `keys_go_to_the_agent_while_typing_with_a_docked_card` (Review Focus 2: `y` becomes a key to the agent; no `ApproveCard`)
  - `esc_clears_card_focus_moves_to_org_and_keeps_the_card_docked`
  - `answering_the_focused_card_focuses_the_next_and_one_key_answers_one_card` (Review Focus 3)
  - `card_focus_is_dropped_when_the_card_is_answered_elsewhere_or_the_agent_has_no_more`
  - `no_card_focus_with_a_team_selected_or_while_reconnecting` (Review Focus 5)
  - `up_in_an_empty_chat_input_no_longer_focuses_a_card`
  - `edit_form_save_returns_to_the_agent_pane_and_cancel_returns_card_focus`
  - `card_keys_do_not_clash_with_org_pane_e` (keymap: context `DockedCard`, conflict rules from UI §4.10)
  - `zoom_keeps_card_focus_and_card_keys_work_zoomed` (CD-D5)
- [ ] **Step 2: Run** `cargo test -p duohive-tui` — expect the new tests FAIL.
- [ ] **Step 3: Implement.** Card keys apply when `focus.pane == Pane::Agent && agent_mode == Selected && focus.card.is_some()` (replace the `Pane::Chat` conditions). `Focus.card` is cleared whenever the pane is not `Pane::Agent`, or the mode is Typing. Rename the key-map context from Chat to DockedCard for the three card actions and keep the Settings key editor and the `?` map sections consistent with UI §4.10 and §4.11.
- [ ] **Step 4: Run** `cargo test -p duohive-tui` — expect PASS.
- [ ] **Step 5: Commit** `feat: focus and answer the docked approval card from Needs you`

### Task CD.5: Mouse

**Files:**
- Modify: `input/mouse.rs` (`on_click` at mouse.rs:249 and the agent-pane press path; `on_chat` loses its card code; `click_to_type`)
- Test: `input/mouse_tests.rs`

**Interfaces:**
- Consumes: `render::agent_card_at`, `dock::focus_card`, `cards::press_button`.

- [ ] **Step 1: Write the failing tests:**
  - `click_on_the_docked_card_focuses_it_and_does_not_start_typing` (CD-D8)
  - `click_on_approve_sends_approve_card` (and edit, reject)
  - `click_on_the_card_while_typing_leaves_typing_then_acts`
  - `click_in_the_agent_pane_below_the_card_still_starts_typing` (click-to-type unchanged)
  - `a_drag_that_starts_on_the_card_selects_text_and_does_not_press_a_button`
  - `click_in_the_chat_never_focuses_a_card`
- [ ] **Step 2: Run** `cargo test -p duohive-tui input::mouse` — expect FAIL.
- [ ] **Step 3: Implement:** in the agent-pane press, read `agent_card_at` before focus moves; a hit calls `dock::focus_card` (leaving Typing) and `press_button` when a button was hit, and records no click-to-type intent.
- [ ] **Step 4: Run** `cargo test -p duohive-tui` — expect PASS.
- [ ] **Step 5: Commit** `feat: click the docked approval card and its buttons`

### Task CD.6: Words, footers, help, docs and the gate

**Files:**
- Modify: `model.rs` (`NO_FREE_CARD` at model.rs:824 becomes `No free agent · a card on Duo proposes one`), the footers for a focused card and the `nothing to edit · y or n` note (`input/cards.rs:15`) where they say "chat", `render/help.rs`, `docs/plans/status.md`, this plan's status line
- Test: `render/help_tests.rs`, `render/screen_tests.rs`, snapshots

- [ ] **Step 1: Write the failing tests:** the focused-card footer reads `y approve · e edit · n reject · esc back`; the key map shows `y e n` under *Docked card*; the no-free-agent note says Duo; the Needs you footer hint is unchanged.
- [ ] **Step 2: Run** `cargo test -p duohive-tui` — expect FAIL.
- [ ] **Step 3: Implement** the text changes; update `status.md`.
- [ ] **Step 4: Run** `make check` — expect PASS (format, clippy, tests, `cargo deny`, `typos`).
- [ ] **Step 5: Commit** `docs: docked approval cards (status, help, footers)`

### Task CD.7: Hand check (the author's)

- [ ] **Step 1:** In a real session: raise a stop card on a working agent, and an add-agent card for Duo; confirm the alert, then reach each from Needs you with `enter` and by clicking the docked card; answer with `y`, `e`, `n` and the mouse; queue three on one agent and answer them in order; type to an agent with a card docked and confirm the keys reach the agent; zoom with a card focused; check 80×24 and 120×36.
- [ ] **Step 2:** Note the result in `status.md`.

## Self-review

- **Spec coverage:** dock agent (CD.1) · queue and counter (CD.1, CD.2) · docking and no auto-switch (CD.2, CD.4) · getting there by Needs you and click (CD.4, CD.5) · keys, `esc`, Typing (CD.4) · height (CD.2) · zoom (CD.2, CD.4) · chat holds only results (CD.3) · mouse (CD.5) · footers and help (CD.6).
- **Open for the author:** CD-D4 (focus passes to the next card); a card with no focus can't be answered while zoomed (CD-D5).

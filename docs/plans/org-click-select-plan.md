# A click on an org row selects; it no longer starts Typing — Implementation Plan

**Status:** Plan, 2026-11-15. Approved by the author 2026-11-15: option A (click only selects), applied to Needs you rows too, with an `enter type` hint in the org footer. Not built.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox syntax.

**Goal:** Clicking an agent's row in the org pane (or Needs you) selects it and leaves focus on that pane, so `↑` `↓` keep moving the selection; Typing starts with `enter` or a click inside the agent pane, and the org footer says so.

**Architecture:** One predicate changes: a left press starts a click-to-type intent only when it lands in the agent pane (`press_target` in `input/mouse.rs`). Everything else (selection, focus, drags, `typing_wanted`, the wait for the live terminal) is untouched. The org footer gains `enter type` while the selected row is an agent or Duo that takes input.

**Specs (the plan argues from these; update them first, Task OCS.1):** [PRD *Agent pane*, *Moving and sizing*](../product/prd.md) · [UI §4.1.2 *Agent pane* and *Clicks*, §4.1.3, the key tables and *Mouse parity*](../ux/ui-spec.md) · [TD *Typing from a click or `enter`*](../design/technical-design.md).

## Global Constraints

[AGENTS.md](../../AGENTS.md) and [Phase 0's](phase-0-plan.md#global-constraints): `make check` before every commit; conventional commits, no `Co-Authored-By`, never push; no `unwrap`/`expect` outside tests; test first; `duohive-tui` only (`duohive-core` untouched); no `std::env::set_var` in tests; files under 800 lines. Work on the branch `feat/org-click-selects`, never on `main`.

## Decisions

| # | Decision |
|---|---|
| OCS-D1 | **Supersedes CT-D1 and CT-D2 for clicks on rows.** A click on an agent's row in the org pane or in Needs you selects it and focuses that pane (as it already does for a stopped or errored agent, CT-D7 a); it never requests Typing. Reason (author, 2026-11-15): a click on a row is looking around or picking an agent to edit or give work to, and Typing swallowed `↑` `↓`, `e`, `s` and `space`. |
| OCS-D2 | **Typing still starts** with `enter` on the row (CT-D1, unchanged), with `enter` in the agent pane, and with a click inside the agent pane while it shows a running agent (CT-D2, CT-D3, CT-D4 and CT-D6 stand for those two paths). |
| OCS-D3 | **Needs you rows follow the org pane** (the author's call, same reason): a click selects the agent, unfolds its team and role, focuses Needs you and marks a finished agent seen; `enter` there still goes to the agent and types. A click on a card row still opens its dialog. |
| OCS-D4 | **The hint** is `enter type`, placed straight after `↑↓ move` in the org footer (full and compact), so a narrow terminal truncates the tail, not the hint. It shows only when `enter` on the selected row would start Typing: an agent or Duo that `AgentState::is_running` (CT-D3). It is absent on a team, a role, and a stopped or errored agent. The Needs you footer already reads `enter go to agent` and is unchanged. |
| OCS-D5 | **A click while Typing** is unchanged: inside the agent pane it keeps Typing; on a row it leaves Typing (as `ctrl+q`) and selects, so `↑` `↓` then move the selection and nothing reaches the agent. |

## Review Focus

1. A click on a running agent before its stream is live: no `typing_wanted`, no `opening <agent>…` footer, `↑` `↓` work at once (OCS.2).
2. A click on a finished agent: marked seen (`ViewAgent` sent), selected, no Typing (OCS.2).
3. Typing in agent A, click agent B's row, press `↓`: focus is the org pane, the selection moves, no key reaches an agent (OCS.2).
4. A press on a row that becomes a drag still moves the node, and a press in the agent pane that becomes a text selection still selects; neither starts Typing (existing tests, keep green) (OCS.2).
5. The hint: absent on a team, a role, a stopped agent and an errored agent; present on Duo when running; the 80×24 compact footer keeps `enter type` and is not cut mid-word (OCS.3).

---

### Task OCS.1: Docs first

**Files:**
- Modify: `docs/ux/ui-spec.md`, `docs/product/prd.md`, `docs/design/technical-design.md`, `CHANGELOG.md`, `docs/plans/click-to-type-plan.md`, `docs/plans/status.md`

**Interfaces:**
- Produces: the wording the tests and code in OCS.2 and OCS.3 implement. Behaviour is specified only here, in the three documents (AGENTS.md).

- [ ] **Step 1: UI spec.** §4.1.2: in the mode table, Typing's *Enter / leave* becomes `enter` in the pane, `enter` on a running agent's row, or one click inside the pane on a running agent; Selected's *Enter / leave* adds "a click on any agent's row (org pane or Needs you), where the pane that was clicked keeps focus". Rewrite the **Clicks** paragraph: a click on a row selects it and keeps that pane's focus (finished agent marked seen, team and role not folded); a click inside the agent pane on a running agent starts Typing, with its release, drag, `opening <agent>…` and cancel rules kept; a click on a row while Typing leaves Typing and selects. §4.1.3: replace "`enter` or a click on an agent row selects the agent and starts Typing" with `enter` types and a click selects (card rows unchanged). Key table (Org pane `enter` row, Needs you row), the *Mouse parity* row and the Mouse row in the key summary: say the same. §4.1's footer mock (`tab pane · ↑↓ move · …`) and its compact mock gain `enter type` after `↑↓ move`; add one sentence under the footer description: the org footer shows `enter type` while `enter` on the selected row would start Typing.
- [ ] **Step 2: PRD.** *Agent pane* (line 264): one click **inside the pane**, or `enter` on a running agent's row, starts typing. *Moving and sizing* (line 272): a click on a row selects it and keeps the pane's focus; `tab`, the arrow keys and a click on a row never start typing.
- [ ] **Step 3: TD.** In *Typing from a click or `enter`* say the intent exists only for a press inside the agent pane (`press_target`); a press on an org or Needs you row selects and focuses, and sets no `ClickIntent`.
- [ ] **Step 4: CHANGELOG** under `[Unreleased]` → `### Changed`: a click on an agent's row in the org pane or Needs you now only selects it, so `↑` `↓` keep working; press `enter` or click the agent pane to type; the org footer shows `enter type`.
- [ ] **Step 5: Supersede notes.** In `click-to-type-plan.md` append to CT-D1 and CT-D2 "**Superseded 2026-11-15 for clicks on a row (OCS-D1):** a row click selects; `enter` and a click in the agent pane still type." In `status.md`'s click-to-type line add one sentence pointing to this plan.
- [ ] **Step 6: Gate and commit.** Run `make check` (expect pass; `typos` covers the docs), then `git add docs CHANGELOG.md && git commit -m "docs: a click on an org row selects; enter or the agent pane types"`.

### Task OCS.2: A click on a row selects and never types

**Files:**
- Modify: `crates/duohive-tui/src/input/mouse.rs` (`press_target` ~l.181, the doc comment of `click_to_type` ~l.194), `crates/duohive-tui/src/model.rs` (`ClickIntent` doc ~l.125)
- Test: `crates/duohive-tui/src/input/mouse_tests.rs`

**Interfaces:**
- Consumes: `press_target(m: &UiModel, areas: &PaneAreas, at: Position) -> Option<AgentName>`, `ClickIntent`, `click_to_type` (unchanged signatures).
- Produces: `press_target` returns `Some` only for a press inside `areas.agent`.

- [ ] **Step 1: Rewrite the tests that encode the old behaviour (red).** In `mouse_tests.rs`:
  - `click_org_row_of_a_running_agent_moves_focus_and_types` → `click_org_row_of_a_running_agent_selects_and_keeps_org_focus`: after `click`, `selected == Agent(billing-api)`, `focus.pane == Pane::Org`, `!m.typing()`, `typing_wanted == None`.
  - `click_org_row_before_the_stream_opens_types_once_live` → `click_org_row_before_the_stream_opens_requests_nothing`: `live_agent = None`; after `click`, `focus.pane == Pane::Org`, `typing_wanted == None`, `footer_note == None`.
  - `click_finished_agent_marks_it_seen_and_starts_typing` → `click_finished_agent_marks_it_seen_and_does_not_type`: the press still returns `[UiEffect::Send(Command::ViewAgent { agent: squad-2 })]`; after the release `!m.typing()` and `focus.pane == Pane::Org`.
  - `click_needs_you_agent_row_starts_typing` → `click_needs_you_agent_row_selects_and_keeps_needs_you_focus`: `focus.pane == Pane::NeedsYou`, `!m.typing()`, selection moved.
  - `clicking_another_row_while_typing_moves_typing_to_that_agent` → `clicking_another_row_while_typing_leaves_typing_and_selects`: after the click `!m.typing()`, `focus.pane == Pane::Org`, selected is the clicked agent; then `Action::Down` through `crate::input::update` moves `selected` and emits no `Command` to an agent.
  - `an_agent_stopped_between_press_and_release_does_not_get_typing` and `a_selection_moved_by_a_key_between_press_and_release_does_not_get_typing`: press inside the agent pane (`areas(&m).live`) instead of an org row so they still exercise the intent; assertions unchanged.
  - Also check `click_other_agent_shows_it_live`, `click_pane_focuses` and `a_click_that_starts_typing_keeps_the_scrolled_snapshot` (l.~823): any that clicked a row to reach Typing now click the agent pane instead.
  Keep `click_agent_pane_starts_typing_on_release`, `click_inside_while_typing_keeps_typing` and the three drag tests unchanged.
- [ ] **Step 2: Run** `cargo test -p duohive-tui input::mouse` — expect the rewritten tests to FAIL on `focus.pane`/`typing` assertions.
- [ ] **Step 3: Change `fn press_target`** so it returns `agent_of(m, &m.selected)` when `areas.agent.contains(at)` and `None` otherwise; delete the org and Needs you branches. Update the doc comments of `press_target`, `click_to_type` and `ClickIntent` to "a press inside the agent pane" (OCS-D1).
- [ ] **Step 4: Run** `cargo test -p duohive-tui` — expect PASS. Fix any other test that clicked a row to reach Typing the same way as in Step 1.
- [ ] **Step 5: Commit** after `make check`: `git add crates/duohive-tui && git commit -m "feat: a click on an org or Needs you row selects; it no longer starts Typing"`.

### Task OCS.3: The `enter type` hint

**Files:**
- Modify: `crates/duohive-tui/src/render/header.rs` (`org_keys` l.~215, `org_keys_compact`, their call in `draw_footer`)
- Test: `crates/duohive-tui/src/render/pane_tests.rs`; regenerated snapshots under `render/snapshots/`

**Interfaces:**
- Consumes: `UiModel::selected`, `UiModel::view(&AgentName).status.state.is_running()`.
- Produces: `fn org_keys(km: &Keymap, can_type: bool) -> String` and `fn org_keys_compact(km: &Keymap, can_type: bool) -> String`; a private `fn enter_types(m: &UiModel) -> bool` true when `m.selected` is `NodeKey::Agent(a)` with `m.view(a)` running, or `NodeKey::Duo` with Duo running (the same test `panes::enter` applies through `request_typing`).

- [ ] **Step 1: Write failing tests** in `pane_tests.rs` (footer text of the bottom row): `org_footer_shows_enter_type_for_a_running_agent` (select `billing-api`: contains ` ↑↓ move · enter type · `), `org_footer_has_no_enter_type_for_team_role_stopped_and_errored` (select `Payments`, `Backend`, `pay-qa`, and an errored agent: not contains `enter type`), `compact_org_footer_keeps_enter_type` (80×24, running agent: contains `enter type`, no truncated `ent`/`en` tail).
- [ ] **Step 2: Run** `cargo test -p duohive-tui render::pane_tests::org_footer` — expect FAIL.
- [ ] **Step 3: Implement** `enter_types`, add `can_type` to both key builders, inserting `enter type · ` right after `↑↓ move · `, and pass `enter_types(m)` from `draw_footer`.
- [ ] **Step 4: Run** the tests — expect PASS. Then `cargo insta test -p duohive-tui --review` (or `INSTA_UPDATE=always` then `git diff`): every changed snapshot must differ only in the footer line, and only where a running agent or Duo is selected. Reject anything else.
- [ ] **Step 5: Docs check.** Re-read the §4.1 footer mocks and the `?` key map in the UI spec against the snapshots; fix the spec where they differ.
- [ ] **Step 6: Commit** after `make check`: `git add crates/duohive-tui docs && git commit -m "feat: the org footer says enter type when enter would type"`.

### Task OCS.4: Hand check (for the author)

- [ ] **Step 1:** Run DuoHive with a few running agents. Click an agent row: the terminal shows, the org pane keeps its heavy border, `↑` `↓` move, the footer shows `enter type`. Press `enter`: Typing. `ctrl+q`, click another row, `↓`: moves on. Click inside the agent pane: Typing. Click a Needs you agent row: selected, focus stays in Needs you.
- [ ] **Step 2:** Tick it in `status.md` and mark this plan's status "built" with the branch name.

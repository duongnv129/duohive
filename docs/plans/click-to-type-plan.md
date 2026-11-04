# Click to type — Implementation Plan

**Status:** Plan, drafted 2026-11-03 · not started · branch `feat/click-to-type`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** One click on a running agent, or `enter` on its row, starts Typing in the agent pane (today: click or `enter` focuses the pane in Selected mode, and a second `enter` types).

**Specs (the plan argues from these; they already carry the behaviour, read them first):** [PRD *Agent pane* and *Moving and sizing*](../product/prd.md) · [UI §4.1.2 *Clicks*, §4.6 Needs you, key tables and key map](../ux/ui-spec.md) · [TD `PaneView`, *Typing from a click or `enter`*](../design/technical-design.md). This plan holds no behaviour of its own beyond the decisions below.

## Global Constraints

From [AGENTS.md](../../AGENTS.md) and Phase 0's [Global Constraints](phase-0-plan.md#global-constraints):

- Toolchain 1.99.0, edition 2024. `make check` passes before every commit. Conventional commits; never a `Co-Authored-By` line; never push.
- No `unwrap`/`expect` outside tests; no `unsafe`. `duohive-core` is untouched: this is `duohive-tui` only.
- No `std::env::set_var` in tests; tmux tests use a throwaway socket.
- Test first.
- A change to behaviour edits the PRD, UI spec and TD in the same commit (AGENTS.md, *Where features are specified*). The three are already updated for this feature; keep them true if a task changes the wording.

## Decisions

| # | Decision |
|---|---|
| CT-D1 | **Reverses P2-D25's "a click never starts typing".** Decided by the author, 2026-11-03 (option A of three: click and `enter` straight to Typing; `enter` only, and a setting, were rejected). The rest of P2-D25 stands: the wheel never moves focus and a click is never sent to the agent. |
| CT-D2 | **Typing starts on button release**, only if the press did not become a drag (moving a node, resizing the chat) or a text selection. A press still selects and focuses as today (an org or Needs you press leaves focus on that pane); the release then moves focus to the agent pane and requests Typing, because `UiModel::typing()` needs `focus.pane == Agent` and Typing without it would leave the keys on the tree. |
| CT-D3 | **Only a running agent** (`AgentState::is_running`) goes to Typing, Duo included. This is the predicate the pane's own `enter` already uses, so it covers starting, working, waiting, quiet, idle and finished agents (a finished agent's session is alive and takes input; focusing it marks it seen, UI §2.1). Only stopped and errored agents land in Selected, as today. |
| CT-D4 | **Typing waits for the live terminal.** `start_typing` needs `live_agent == agent`, and a freshly selected agent's stream arrives later. A click or `enter` on a running agent that isn't live yet sets `UiModel.typing_wanted`; `LiveView` turns it into Typing when that agent becomes `live_agent`. Selection, focus or mode changes drop it; the focus check is against the agent pane, which a click or `enter` has already set. |
| CT-D5 | `tab`, `shift+tab`, arrows and `esc` never start Typing. Teams and roles keep fold/select. The Worktrees list's `enter` ("open in agent") also stays in Selected mode: it is a dialog action, and the row may belong to a stopped agent. |

## Review Focus

Most likely to go wrong first; each has a test in the owning task.

1. A press-drag on an org row, or a press-drag in the agent pane, ends in Typing (CT.3).
2. Click then quickly click another row: Typing starts on the first agent, or the wanted flag outlives the move (CT.4).
3. Click on a running agent whose stream is still opening: Typing never starts, or starts on the wrong agent (CT.4).
4. A stray release with no matching press (press on a dialog, release outside) starts Typing (CT.3).
5. Typing starts for an agent that stopped between press and release (CT.3).
6. Key map, footers and help snapshots still say "focus the agent pane" (CT.5).
7. Typing starts while focus is still on the org or Needs you pane, so `typing()` is false and keys keep going to the tree (CT.3).
8. An arrow key pressed while the button is held moves the selection, and the release starts Typing on a different agent than the one clicked (CT.3).

## File Structure

```
crates/duohive-tui/src/
  model.rs            typing_wanted, click intent                       (CT.1, CT.3)
  input/agent.rs      request_typing, shared by enter and click          (CT.1)
  input/panes.rs      org `enter`; Needs you `enter`                     (CT.2)
  input/mouse.rs      press records intent, release starts Typing        (CT.3)
  live.rs             typing_wanted becomes Typing when the stream is live (CT.4)
  render/header.rs, render/help.rs, snapshots   wording                  (CT.5)
```

---

### Task CT.1: `request_typing` and `typing_wanted`

**Files:** `model.rs`, `input/agent.rs`, tests in `input/agent_tests.rs` (create if absent, matching the sibling test modules).

- [ ] **Step 1: Tests first.**
  - `request_typing_starts_typing_when_live` — selected agent is `live_agent` and running: mode is Typing, `scrolled` is false, `typing_wanted` is None.
  - `request_typing_waits_when_not_live` — running but not live: mode unchanged, `typing_wanted == Some(agent)`.
  - `request_typing_ignores_stopped_agent` — mode unchanged, `typing_wanted` None, no footer note.
  - `request_typing_on_team_does_nothing`.
- [ ] **Step 2:** Add `typing_wanted: Option<AgentName>` to `UiModel`. Add `pub(super) fn request_typing(m)` in `input/agent.rs` that applies CT-D3 and CT-D4, and make `start_typing` (the pane's `enter`) call it. Keep the "isn't ready yet" footer note only for the pane's own `enter`.
- [ ] **Step 3:** Run, then commit `feat: request typing for a selected running agent`.

### Task CT.2: `enter` on a row starts Typing

**Files:** `input/panes.rs`, `input/mod.rs` (the Needs you path), `input/panes_tests.rs`.

- [ ] **Step 1: Tests first.** `enter_on_running_agent_row_starts_typing`, `enter_on_duo_row_starts_typing`, `enter_on_stopped_agent_row_focuses_pane_in_selected`, `enter_on_errored_agent_row_focuses_pane_in_selected`, `enter_on_finished_agent_row_starts_typing_and_marks_it_seen`, `enter_on_team_still_folds`, `enter_in_needs_you_on_running_agent_starts_typing`, `enter_in_needs_you_on_card_focuses_card`, `tab_into_agent_pane_stays_selected`, `arrow_never_starts_typing`.
- [ ] **Step 2:** In `panes::enter` for `Agent | Duo`: focus the agent pane in Selected, then `request_typing`. Do the same for `needs_you_enter`'s agent case. Leave `tab` and the arrows alone (CT-D5).
- [ ] **Step 3:** Run, then commit `feat: enter on an agent row starts typing`.

### Task CT.3: a click starts Typing on release

**Files:** `model.rs`, `input/mouse.rs`, `input/mouse_tests.rs`.

- [ ] **Step 1: Tests first.** Invert `click_agent_pane_does_not_start_typing` (`mouse_tests.rs:429`) into `click_agent_pane_starts_typing_on_release`; keep `click_never_reaches_agent` and `click_inside_while_typing_keeps_typing` unchanged. Add:
  - `press_alone_does_not_start_typing` (before release, mode is Selected);
  - `click_org_row_of_running_agent_starts_typing_on_release`;
  - `click_org_row_of_stopped_agent_stays_selected`, `click_org_row_of_errored_agent_stays_selected`;
  - `click_finished_agent_marks_it_seen_and_starts_typing`;
  - `drag_from_org_row_does_not_start_typing` (press, drag over another row, release: no Typing);
  - `drag_in_agent_pane_selects_and_does_not_start_typing`;
  - `chat_edge_resize_does_not_start_typing`;
  - `release_without_press_does_not_start_typing`;
  - `agent_stopped_between_press_and_release_does_not_start_typing`;
  - `click_needs_you_agent_row_starts_typing`;
  - `org_row_click_moves_focus_to_agent_pane_and_keys_reach_agent` (agent already live: after release `m.typing()` is true and `a` becomes `UiEffect::Pane` input, not a tree action);
  - `org_row_click_before_stream_opens_types_once_live` (not live at release: pane is Agent, `typing_wanted` set, Typing starts when the stream goes live);
  - `team_and_role_clicks_keep_focus_on_org`;
  - `click_while_zoomed_does_nothing_new`;
  - `click_other_row_while_typing_moves_typing_to_that_agent`;
  - `selection_moved_by_key_between_press_and_release_does_not_start_typing`;
  - `key_between_press_and_release_clears_the_click`;
  - `right_click_never_starts_typing`.
- [ ] **Step 2:** On a left press that targets an agent row (org or Needs you) or the agent pane showing a running agent, store `m.click = Some(ClickIntent { agent, moved: false })`. `on_drag` and a selection that grows set `moved = true`. On `Up(Left)`: take the intent; if `!moved`, no selection range and the agent is still running, set `m.focus.pane = Pane::Agent` (and `focus.card = None`), then call `request_typing`. The release also requires the selection to still be the pressed agent (`agent_of(m, &m.selected) == Some(intent.agent)`), and any key event, other press, menu, dialog or wheel event clears the intent, so a key pressed while the button is held cannot send Typing to a different agent.
- [ ] **Step 3:** Run, then commit `feat: a click on a running agent starts typing`.

### Task CT.4: Typing once the live terminal shows

**Files:** `live.rs`, `live_tests.rs` (or the existing tests module in `live.rs`).

- [ ] **Step 1: Tests first.** `wanted_typing_starts_when_stream_goes_live`; `wanted_typing_dropped_when_selection_moves`; `wanted_typing_dropped_when_focus_leaves_agent_pane`; `wanted_typing_dropped_when_agent_stops`; `wanted_typing_for_other_agent_does_not_start_typing_on_this_one`; `wanted_typing_survives_a_resize_reopen_once`; `switching_from_live_agent_a_to_b_keeps_wanted_typing_for_b` (A live, click B: `end(A)` runs, B's request stays, Typing starts when B goes live); `switching_from_typing_in_a_to_b_types_in_b`.
- [ ] **Step 2:** Where `live.rs` sets `m.live_agent` (after `sync`, after the first `Reset`, and on `end`), if `typing_wanted == live_agent`, set the mode to Typing, clear `scrolled` and the wanted flag. In `end`, clear the flag only when it names the agent whose stream is closing: switching from agent A to agent B closes A's stream before B's opens, and B's request must survive that. Also clear it when `m.selected` or `m.focus.pane` no longer match it.
- [ ] **Step 3:** Run, then commit `feat: start typing when a clicked agent's terminal is live`.

### Task CT.5: wording, key map and snapshots

**Files:** `render/header.rs`, `render/help.rs`, `render/zoom.rs`, snapshots under `render/snapshots/`, the Keys screen text.

- [ ] **Step 1: Tests first.** Update `help_tests` expectations: Org pane `enter` reads `type to agent`; Mouse `click` reads `select + type`; Needs you `enter` text per UI §4.11. Check every footer that says `enter` after a click-to-focus (org pane on an agent row; `grep -rn "enter" render/header.rs`) matches the UI spec's footers.
- [ ] **Step 2:** Edit the strings; regenerate snapshots with `cargo insta review` and read each diff.
- [ ] **Step 3:** Run, then commit `docs: key map and footers for click to type` (together with any doc wording fixed along the way).

### Task CT.6: acceptance

- [ ] **Step 1:** `make check` passes.
- [ ] **Step 2: Hand check (the author).** iTerm2, Claude Code and OpenCode, 80×24 and 160×48: click a running agent, type a line, `ctrl+q`, click another agent, press `enter` on a row, drag a node to another team (no Typing), select text in the agent pane (no Typing), click a stopped agent (Selected). Record the result in [status.md](status.md).
- [ ] **Step 3:** Add the feature to [status.md](status.md) and, if the author wants it listed, to the [v1.1 roadmap](v1.1-roadmap.md).

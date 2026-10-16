# Phase 2 Friction Log

**Status:** Open · started 2026-10-15, the day after Tasks 2.1–2.18 were merged (`b497418`, 2026-10-14). Phase 2 [Task 2.19](phase-2-plan.md#task-219-phase-2-acceptance--a-day-without-tmux-attach-and-the-friction-log) fills it in, and [Task 2.20](phase-2-plan.md#task-220-uiux-improvement-pass-then-mark-the-phase-done) works from it.

## How to log

- Run your daily org on DuoHive for at least three working days without `tmux attach`, on macOS.
- Log every moment that was slow, confusing or missed.
- Severity:
  - **1:** you had to use `tmux attach`, or lost work;
  - **2:** the task was harder than it should be;
  - **3:** cosmetic.
- Task 2.20 fixes every severity-1 entry, then severity-2 entries by count.

## Days without `tmux attach`

| Day | Date | Needed `tmux attach`? | Notes |
|---|---|---|---|
| 1 | | | |
| 2 | | | |
| 3 | | | |

## Entries

| # | Date | Screen | What was slow, confusing or missed | Severity | Fixed in |
|---|---|---|---|---|---|
| 1 | 2026-10-15 | Stop (Claude) | With Claude's vim editor mode, stop's `/exit` became vim commands and Enter re-ran the last task until the 5 s grace kill | 1 | `fix: vim-safe quit keys for Claude Code` |
| 2 | 2026-10-15 | Give work (OpenCode) | OpenCode's "Update Available" dialog, `Confirm` highlighted, swallowed a pasted task and Enter would start an upgrade | 1 | `fix: close OpenCode dialogs before delivering a task` |
| 3 | 2026-10-15 | Agent pane (Claude, new worktree) | The trust dialog's default is `No, exit`, so a reflex `enter` kills the agent | 2 | `feat: hint how to answer Claude's trust dialog` (P2-D23) |
| 4 | 2026-10-15 | Status (OpenCode) | An invalid API key or a model outside the plan ends the turn as finished with no reply, not errored | 2 | `fix: error an OpenCode agent whose turn fails` (P2-D24) |
| 5 | 2026-10-15 | Agent pane (OpenCode) | OpenCode doesn't draw CJK characters; the text reaches the model intact | 3 | OpenCode's own drawing; not fixed |
| 6 | 2026-10-15 | Give work (OpenCode) | The update dialog can still open in the 150 ms between `Escape` and the paste, for a Give work sent right after launch | 3 | open |
| 7 | 2026-10-15 | Agent pane | UX F1: the title dropped `· TYPING` and `· scrolled` when cut | 2 | `fix: keep the agent pane's mode word when its title is cut` |
| 8 | 2026-10-15 | Footers | UX F2: a note replaced the Typing and zoom footers, and said "space" while typing | 3 | `fix: keep the Typing and zoom footers when a note shows` |
| 9 | 2026-10-15 | Worktrees | UX F3: a long `couldn't check` reason squeezed Branch at every size; no padding cell | 2 | `fix: keep worktree branches whole beside a long check failure` |
| 10 | 2026-10-15 | Worktrees | UX F4: agent names cut at the end, not the middle; the empty list is cut with no `…` and keeps headers and hints | 3 | open |
| 11 | 2026-10-15 | Settings, confirmations | UX F5: they showed the org pane's footer | 2 | `fix: give Settings and confirmations their own footer keys` |
| 12 | 2026-10-15 | Needs you | UX F6: names were cut harder in the wide layout than the compact one | 2 | `fix: stop cutting Needs you names harder in the wide layout` |
| 13 | 2026-10-15 | Chat (80×24) | UX F7: the one history row shows the tail of the last message | 3 | open (P2-D8 pane, replaced in 3.9) |
| 14 | 2026-10-15 | Role summary | UX F8: the Defaults line wraps without its indent at 80×24 and 100×30 | 3 | open |
| 15 | 2026-10-15 | Give work (80×24) | UX F9: the task box wraps mid-word | 3 | open |
| 16 | 2026-10-15 | Footers | UX F10: `enter type` shown for a stopped agent zoomed, and agent-pane keys on a team or role | 3 | open |
| 17 | 2026-10-15 | Agent pane (light theme) | UX F11: live cells use the terminal's own colours inside themed padding | 3 | open (theme `auto` lands in 4.9) |
| 18 | 2026-10-15 | Main panes | UX F12: `esc` does nothing in the main panes; UI §4.11 says it goes back one level (a guess at intent) | 3 | open |
| 19 | 2026-10-15 | Settings | UX F13: a detached `org defaults and Duo` hint line with no heading | 3 | open |
| 20 | 2026-10-16 | Status (OpenCode) | OpenCode 2.0 broke the plugin: it no longer loaded, so OpenCode agents reported no status | 1 | `fix: port the OpenCode adapter to OpenCode 2.0` (P2-D24) |
| 21 | 2026-10-16 | Agent pane (Typing) | Can't scroll an agent's output while typing: `PgUp` goes to the CLI, reading back needs `esc`, `PgUp`, `enter`, and the touchpad wheel scrolls the terminal's own buffer instead | 2 | `feat: scroll agent panes and the chat with the touchpad, typing too` (P2-D25) |
| 22 | 2026-10-16 | Agent pane | `cakekyc-auth-backend`'s window was 240×50 in a 146×40 pane; its rows ran past the box, wrapped, scrolled the screen and left stacked frames in the terminal's scrollback. Not reproduced in a 3-minute watch (windows resized within 1 s) | 2 | open: watch for it; clip the terminal widget to its pane when fixed; it would also offset P2-D25's wheel cell |
| 23 | 2026-10-16 | Main panes | Moving between agents and panes is keyboard only; a click on an agent or a pane does nothing | 2 | `feat: click to select agents and focus panes` (P2-D25) |

## Live checks

These are carried from phase 1 and spike S3. Record the result and the CLI version for each one.

| Check | CLI and version | Result | Notes |
|---|---|---|---|
| A newline and non-ASCII text in OpenCode's `--prompt=` | OpenCode 1.18.35 | pass | One prompt, intact. CJK isn't drawn on screen (entry 5) |
| `--effort=` on a real turn | Claude Code 2.1.300; OpenCode 1.18.35 | pass | Claude: `--effort=low` recorded in the transcript. OpenCode: `variant: low` in the agent config, not visible in the request |
| A task starting with `/` on OpenCode | OpenCode 1.18.35 | pass | Taken as text via `--prompt=`. A later pasted `/` task is not checked |
| A Claude turn that runs on past `Escape` | Claude Code 2.1.300 | fixed | Failed under vim mode (entry 1). The new keys exit with status 0 in 6 of 6 runs (vim and normal; at rest, early, late) |
| S3.7: the trust dialog in a new worktree path | Claude Code 2.1.300; OpenCode 1.18.35 | decided | Claude shows it, default `No, exit`; the author chose `~◆ waiting` plus a hint (P2-D23, entry 3). OpenCode shows none |
| The OpenCode 2.0 adapter: launch, hooks, reply, approvals, failed turn | OpenCode 2.0.29 | pass (plugin) | 2026-10-16, real TUI in a throwaway tmux socket with the shipped plugin and a logging hook: session-start, busy, turn-end with the reply, per-launch instructions, `edit` asked then busy on Allow once, `/exit` → session-end; a 400 from a local stub → turn-end `session.execution.failed` with its error. A full DuoHive session on 2.0.29 is part of the three-day run |
| Mouse reporting in agent panes (P2-D25) | Claude Code 2.1.301; OpenCode 2.0.29 | Claude: on | 2026-10-16: all four live Claude panes are full-screen with mouse reporting on (`mouse_any_flag=1`, `mouse_sgr_flag=1`, `alternate_on=1`), so the wheel goes to Claude and DuoHive's snapshot holds no history for them. OpenCode not yet checked (P2-D25 assumes nothing for it: the flags decide); Claude's own wheel and `PgUp` scrolling to be confirmed when P2-D25 is built |
| `send-keys -H` wheel bytes with Claude's extended keys on (P2-D25) | tmux 3.2a and latest | open | Claude sets modifyOtherKeys and kitty keys; check the `ESC` byte arrives unchanged |
| Text selection with mouse reporting on (P2-D25) | iTerm2 | open | `⌥`-drag expected; check it, and Terminal.app's modifier, when P2-D25 is built |

## UI/UX pass

Task 2.20 fills this in. Check each screen against the six [PRD UX principles](../product/prd.md#ux-principles):
- at 80×24, 100×30 and 160×48;
- in both themes;
- with `NO_COLOR`.

Mark each cell `pass` or `fixed (#entry)`.

Done 2026-10-15 at 80×24, 100×30 and 160×48, in the dark, light and mono palettes.

| Screen | 1 Glance | 2 Not colour only | 3 80×24 | 4 Same key | 5 One key away | 6 Calm |
|---|---|---|---|---|---|---|
| Main: org, Needs you, agent and chat panes | fixed (#12) | pass | open (#13, #14) | open (#18) | pass | pass |
| Agent pane: Selected, Typing, scrolled | pass | fixed (#7, #8) | fixed (#7) | pass | open (#16) | pass |
| Zoomed agent | fixed (#8) | pass | pass | pass | open (#16) | pass |
| Give work dialog | pass | pass | open (#15) | pass | pass | pass |
| Worktrees (`w`) | fixed (#9) | pass | fixed (#9), open (#10) | pass | pass | pass |
| Settings (`,`) and confirmations | pass | pass | open (#19) | pass | fixed (#11) | pass |
| Too-small screen | pass | pass | pass | — | — | pass |
| Desktop alert, alerts-unavailable note | pass | pass | pass | — | fixed (#8) | pass |

`w` and `,` appear in no footer, and `?` waits for phase 4 (P2-D9).

Carried item: long agent names keep UI §2.4's middle cut: pass (`billing-r…on-worker` at 50 columns, `billing…-worker` at 32).

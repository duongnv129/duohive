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
| | | | | | |

## Live checks

These are carried from phase 1 and spike S3. Record the result and the CLI version for each one.

| Check | CLI and version | Result | Notes |
|---|---|---|---|
| A newline and non-ASCII text in OpenCode's `--prompt=` | | | |
| `--effort=` on a real turn | | | |
| A task starting with `/` on OpenCode | | | |
| A Claude turn that runs on past `Escape` | | | |
| S3.7: the trust dialog in a new worktree path | | | If it appears, choose: accept the `~◆ waiting` it shows, or pre-trust per launch. Pre-trusting must never touch global config. |

## UI/UX pass

Task 2.20 fills this in. Check each screen against the six [PRD UX principles](../product/prd.md#ux-principles):
- at 80×24, 100×30 and 160×48;
- in both themes;
- with `NO_COLOR`.

Mark each cell `pass` or `fixed (#entry)`.

| Screen | 1 Glance | 2 Not colour only | 3 80×24 | 4 Same key | 5 One key away | 6 Calm |
|---|---|---|---|---|---|---|
| Main: org, Needs you, agent and chat panes | | | | | | |
| Agent pane: Selected, Typing, scrolled | | | | | | |
| Zoomed agent | | | | | | |
| Give work dialog | | | | | | |
| Worktrees (`w`) | | | | | | |
| Settings (`,`) | | | | | | |
| Too-small screen | | | | | | |
| Desktop alert | | | | | | |

Carried item: long agent names in the 50-column pane keep UI §2.4's middle cut (`billing-…-worker`).

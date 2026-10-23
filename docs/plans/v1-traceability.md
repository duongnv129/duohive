# DuoHive v1 — Traceability

**Status:** Living document · created 2026-10-05.

This matrix gives every requirement in the [PRD](../product/prd.md), every screen and rule in the [UI/UX spec](../ux/ui-spec.md) and every section of the [technical design](../design/technical-design.md) exactly one owning phase. A requirement that spans phases is split into named parts, one row per part. When a plan moves a requirement to another phase, update this file in the same commit.

**How tasks are cited:**

| Notation | Means | Where |
|---|---|---|
| `T1`–`T5` | A task of phase 0 | [phase-0-plan](phase-0-plan.md) |
| `T6`–`T30`, `D16` | A task or decision of phase 1 | [phase-1-plan](phase-1-plan.md) |
| `2.5`, `P2-D8` | A task or decision of phase 2 | [phase-2-plan](phase-2-plan.md) |
| `3.4` | A task of phase 3 | [phase-3-plan](phase-3-plan.md) |
| `4.3` | A task of phase 4 | [phase-4-plan](phase-4-plan.md) |
| `5.2` | A task of phase 5 | [phase-5-plan](phase-5-plan.md) |

## PRD — Core capabilities (MoSCoW)

| Priority | Capability (part) | Phase | Task |
|---|---|---|---|
| Must | Org chart: teams → roles → agents, edited in-app, saved to a shareable file; one org per install | 1 | T6–8, T14, T16, T24–25 |
| Must | Agent definition and inheritance; prompts stack | 1 | T7 |
| Must | One screen: the tree with live status and last reply | 1 | T23 |
| Must | One screen: the selected agent's live terminal | 2 | 2.3, 2.4, 2.7 |
| Must | One screen: chat with Duo (an interim pane in 2.5, P2-D8) | 3 | 3.8, 3.9 |
| Must | Duo: chat, see the org, message agents, start and stop them, approval cards, every action visible | 3 | 3.5, 3.6, 3.10 |
| Must | Agent-to-agent messages at rest; sender identified by token (tokens since D1) | 3 | 3.3, 3.4 |
| Must | Needs you: waiting, quiet and errored agents listed | 1 | T13, T23 |
| Must | Needs you: acted on, with desktop notifications | 2 | 2.6, 2.9, 2.10 |
| Must | Needs you: pending approval cards | 3 | 3.6, 3.10 |
| Must | Honest status: exact, `~`, quiet | 1 | T9, T12–13 |
| Must | Light at scale: 10 agents | 1 | T28 |
| Must | Light at scale: 100 agents | 4 | 4.1, 4.2, 4.10 |
| Must | Light at scale: measurement method and three-run script | 4 | 4.1 (`scripts/bench-gate.sh`), 4.10 |
| Must | Light at scale: release gate | 5 | 5.2 |
| Must | Agents keep running when DuoHive closes; reopening reconnects | 1 | T15, T29 |
| Should | Give work with one key (`space`) | 2 | 2.15 |
| Should | Type straight into an agent's pane | 2 | 2.7 |
| Should | Worktree option when giving work; removed once merged | 2 | 2.11–2.16 |
| Should | Model and effort picked from what each CLI offers | 4 | 4.5 (replaces D14) |
| Should | Every action by key and by mouse: the wheel and click to select (P2-D25) | 2 | 2.21 |
| Should | Every action by key and by mouse: the rest; `?` key map | 4 | 4.6–4.9 |
| Should | Key bindings you can change (Keys screen refuses clashes) | 4 | 4.6 |
| Must | Setup templates | 4 | 4.3, 4.4 |
| Must | Give a task to a role; none free → card | 3 | 3.7, 3.11 |
| Could | Zoom the agent pane | 2 | 2.8 |
| Could | Compact tree view: hide last replies (`v`) | 1 | T24 |
| Could | Compact tree view: folding at 80–99 columns | 2 | 2.5 |
| v1.1 | Resume an old session | v1.1 | [v1.1 roadmap](v1.1-roadmap.md) feature 1 |

## PRD — Other requirements

| Requirement (part) | Phase | Task |
|---|---|---|
| Lifecycle: start, Give work by state, free, stopping, deleting, renaming while stopped | 1 | T9, T11, T14, T24 |
| Lifecycle: no renaming while worktrees exist | 2 | 2.13 |
| Lifecycle: Give work by state, next turn confirmed by the agent leaving rest (P2-D11) | 2 | 2.12 |
| Worktree clean-up rules | 2 | 2.13, 2.14, 2.16 |
| Messages to a stopped agent | 3 | 3.4 |
| Where the org lives; missing directory | 1 | T8, T14, T16 |
| Org defaults: Edit org | 1 | T25 |
| Settings screen: You, Attention, Look | 2 | 2.17 (P2-D13) |
| Settings screen: the Keys row and the Org group's `New org from template…` row | 4 | 4.6, 4.4 |
| Approvals stay with you: OpenCode `edit` and `bash` ask; no auto-approve flags (except DuoHive's own tools from phase 3, P3-D1) | 1 | T20–21 |
| Approvals: each launch pre-approves only DuoHive's own MCP tools (P3-D1) | 3 | 3.2 |
| Approvals stay with you: no tool answers a prompt | 3 | 3.5 |
| Desktop alerts: Needs you entries and non-Duo finished | 2 | 2.9, 2.10 |
| Desktop alerts: a raised card | 3 | 3.6 |
| UX principles: phase 1–2 screens | 2 | 2.20 |
| UX principles: phase 3–4 screens | 5 | 5.1 |
| Friction log during phase 2 | 2 | 2.19 |
| User flow "install to a working team": step 1 (install) | 5 | 5.3a, 5.3b, 5.4 |
| User flow "install to a working team": step 2 (first run) | 4 | 4.3 |
| User flow "install to a working team": steps 3–4 | 2 | 2.15, 2.6, 2.10 |
| User flow "install to a working team": step 5 | 3 | 3.12 |
| User flow "install to a working team": step 6 | 1 | T15 |
| User flow "a story through the factory" | 3 | 3.12 |
| Success metric: 30-day daily use | 5 | 5.7, 5.9 |
| Success metric: time to first team | 5 | 5.6 |
| Success metrics: idle CPU, memory, status freshness, start-up | 4 | 4.1, 4.2, 4.10 |
| Success metric: binary size, measured | 4 | 4.1 |
| Success metric: binary size, gated | 5 | 5.2 |
| Name checks, install channels, README, CONTRIBUTING | 5 | 5.3a–5.5b |
| Interviews with 3–5 leads (not blocking) | 5 | 5.7 (optional) |

## UI/UX spec

| Section (part) | Phase | Task |
|---|---|---|
| §1 Design direction; §2 Visual language: status, glyphs, tokens, text | 1 | T23 |
| §2.1 Working spinner, static with Reduce motion | 1 | T23 |
| §2.1 Reduce motion toggle on screen | 2 | 2.17 |
| §2.2 `✉` messages waiting | 3 | 3.4, 3.9 |
| §2.2 `▾ ▸` folding | 2 | 2.5 |
| §2.3 Theme `auto` follows the terminal (P2-D26) | 2 | 2.22 |
| §2.1, §2.3, §6 Quiet colour: one accent, glyph-only status colour, new palettes (P2-D27) | 2 | 2.23 |
| §3 Layout: three panes, sizes, shrink order, focus, keyboard resize | 2 | 2.5 |
| §3 Layout: drag the chat edge | 4 | 4.7 |
| §4.1 Main screen: header, tree, roll-ups | 1 | T23 |
| §4.1 Main screen: three panes, per-pane footer | 2 | 2.5 |
| §4.1 Main screen: header count click | 4 | 4.7 |
| §4.1.1 Filter | 4 | 4.9 |
| §4.1.2 Agent pane: modes, live terminal, Duo, summaries | 2 | 2.5, 2.7 |
| §4.1.2 Agent pane: full role-summary copy | 3 | 3.11 |
| §4.1.2 Agent pane: full empty-org copy | 3 | 3.9 |
| §4.1.3 Needs you: agents | 2 | 2.6 |
| §4.1.3 Needs you: card rows | 3 | 3.10 |
| §4.1.3, §4.6 Clicks on card rows and card buttons (P2-D25) | 3 | 3.10 |
| §4.1.4 Desktop alerts: agents (click: P2-D18, UI edit in 2.20) | 2 | 2.9, 2.10, 2.20 |
| §4.1.4 Desktop alerts: a raised card | 3 | 3.6 |
| §4.2 Zoomed agent | 2 | 2.8 |
| §4.1.2, §4.12 Claude's trust-dialog hint (P2-D23; friction entry 3) | 2 | 2.20 |
| §4.1.2, §4.2, §4.5, §4.9, §4.10: UX pass fixes (footers that fit a stopped agent and a team or role, the wrapped Defaults line, the task box's word wrap, the worktree list's middle cut and empty list, Settings' Org heading; friction entries 10, 14–16, 19) | 2 | 2.20 |
| §4.3 First-run setup; no-CLI screen | 4 | 4.3 |
| §4.4.1 Forms, including the worktree radio's git-repo rule | 1 | T25 |
| §4.4.1 Forms: model and effort dropdowns | 4 | 4.5 |
| §4.4.2 New (chooser) and Delete | 1 | T24–25 |
| §4.4.2 Right-click menu, Move to…, drag and drop | 4 | 4.7, 4.8 |
| §4.5 Give work: agent variants | 2 | 2.15 |
| §4.5 Give work: role, no-free-agent, Send as message instead | 3 | 3.11 |
| §4.6 Approval cards and `→` action lines | 3 | 3.9, 3.10 |
| §4.7 Chat (interim pane 2.5, P2-D8) | 3 | 3.9 |
| §4.8 Confirmations: stop, delete | 1 | T24–25 |
| §4.8 Confirmations: worktree copy, Remove worktree | 2 | 2.16 |
| §4.8 Confirmations: Replace your org | 4 | 4.4 |
| §4.9 Worktrees | 2 | 2.16 |
| §4.10 Settings: You, Attention, Look | 2 | 2.17 |
| §4.10 Settings: the Keys row, the Org group's `New org from template…` row; the key-binding editor | 4 | 4.6, 4.4 |
| §4.11 Key map: phase 1 keys (D8) | 1 | T24 |
| §4.11 Key map: phase 2 keys (P2-D9) | 2 | 2.5–2.8, 2.15–2.17 |
| §4.11 Key map: chat and card keys | 3 | 3.9, 3.10 |
| §4.11 Key map: mouse wheel and click to select (P2-D25) | 2 | 2.21 |
| §4.11 Key map: `esc` in a main pane goes back to the org pane (P2-D28) | 2 | 2.20 |
| §4.11 Key map: `?`, `f`, `m`, the rest of the mouse, rebinding | 4 | 4.6–4.9 |
| §4.12 Errors and empty states: tmux missing, row errors (without `space retry`), banner, empty role (held-back copy) | 1 | T23 |
| §4.12 Errors and empty states: errored row's `space retry` | 2 | 2.15 |
| §4.12 Errors and empty states: empty role, full copy (`… or ask Duo`) | 3 | 3.9 |
| §4.12 Errors and empty states: worktree creation error | 2 | 2.12 |
| §4.13 Resume an old session | v1.1 | [v1.1 roadmap](v1.1-roadmap.md) feature 1 |
| §5 Interaction rules: keys, typing, quit, destructive confirms, calm | 1 | T24 |
| §5 Interaction rules: typing goes where focus is (agent pane, chat) | 2 | 2.5, 2.7 |
| §5 Interaction rules: cards answer only with focus | 3 | 3.10 |
| §5 Interaction rules: mouse wheel (Typing too) and click to select and focus (P2-D25) | 2 | 2.21 |
| §5 Interaction rules: mouse parity (right-click, drag, header counts; clicks in dialogs, forms, lists and first run, P4-D2) | 4 | 4.7 |
| §6 Accessibility: glyph and word, `NO_COLOR` | 1 | T23 |
| §6 Accessibility: colour-blind and contrast re-check | 4 | 4.9 |
| §7 Copy: phase 1–2 screens | 2 | 2.20 |
| §7 Copy: phase 3–4 screens | 5 | 5.1 |
| §8 UX checks: after phase 2 | 2 | 2.19, 2.20 |
| §8 UX checks: before launch | 5 | 5.1 |

## Technical design

| Section (part) | Phase | Task |
|---|---|---|
| §2 Decisions: language, runtime, CI-facing choices verified by spikes | 0 | T1–T5 |
| §2 Decisions: process model, status, concurrency, storage, CLI settings, approvals | 1 | T6–T30 |
| §2 Agents' tools | 3 | 3.1–3.3 |
| §3.1 Ports: `Runtime`, `CliAdapter`, `Store`, `Clock` | 1 | T10 |
| §3.1 Ports: `Notifier`, `Git`, `PaneView` (P2-D1) | 2 | 2.1, 2.2 |
| §3.2 Command/event boundary | 1 | T10 |
| §3.2 Command/event boundary: tool commands | 3 | 3.1 |
| §3.3 Concurrency: the core task and timer wheel; the TUI's draw-on-change at 30 fps | 1 | T15, T24 |
| §3.3 Concurrency: git checks and worktree re-check timers | 2 | 2.13, 2.14 |
| §3.3 Concurrency: the MCP listener | 3 | 3.1 |
| §3.4 Workspace | 0 | T1 |
| §3.4 `duohive-mcp` content | 3 | 3.1, 3.2 |
| §4.1–§4.2 Status pipeline and engine | 1 | T9, T12–13, T19 |
| §4.1 Start-up: git checks after the first frame | 2 | 2.13 |
| §4.2 The finished/idle next-turn row (P2-D11) | 2 | 2.12 |
| §4.2 Alert triggers: agents | 2 | 2.9 |
| §4.2 Alert triggers: a raised card | 3 | 3.6 |
| §4.2 Cards in Needs you | 3 | 3.6 |
| §4.3 Cost at scale: no pane streams | 1 | T19 |
| §4.3 Cost at scale: live view path | 2 | 2.1, 2.3 |
| §4.3 Cost at scale: 100-agent tuning | 4 | 4.2 |
| §5.1 Launch path | 1 | T11 |
| §5.1 Launch path: Run choice, delivery confirmation | 2 | 2.12 |
| §5.2 CLI adapters | 1 | T20–21 |
| §5.2 CLI adapters: OpenCode 2.0 port (P2-D24; friction entry 20) | 2 | 2.20 |
| §4.2, §5.5, §8 A failed turn errors the agent (P2-D24; friction entry 4) | 2 | 2.20 |
| §5.2 CLI adapters: MCP | 3 | 3.2 |
| §5.2 CLI adapters: model lists | 4 | 4.5 |
| §5.3 Worktrees | 2 | 2.11–2.14 |
| §5.3 Worktrees: a creation git never answers (P2-D22) | 2 | 2.20 |
| §5.4 Task to a role | 3 | 3.7 |
| §5.5 Stop and exit | 1 | T11, T12, T19 |
| §5.5 Vim-safe quit keys for Claude Code (friction entry 1) | 2 | 2.20 |
| §4.2, §5.5 OpenCode dialog check before the paste and before `Enter` (friction entry 6) | 2 | 2.20 |
| §6.1–§6.5 Identity, tools, inbox, Duo, Duo chat (token generation: D1) | 3 | 3.1–3.8 |
| §6.4 Changing Duo's CLI restarts Duo after a confirmation | 1 | T14, T25 |
| §6.3 Inbox: delivery counts once the recipient leaves rest (P2-D11) | 2 | 2.12 |
| §7 Storage: files, `state.db`, single instance (`run.lock`), quit and shutdown, logs | 1 | T16–17, T22, T27, T29 |
| §7 Storage: worktrees table, `sessions.worktree` | 2 | 2.11, 2.13 |
| §7 Storage: tool calls queued while closed, messages, cards, chat tables | 3 | 3.2, 3.4, 3.6, 3.8 |
| §7 Storage: timestamped template backup; replace-flow data rules | 4 | 4.4 |
| §7 Storage: `settings.toml` `[keys]` schema | 4 | 4.6 |
| §7 Storage: `model-cache.json` | 4 | 4.5 |
| §8 Errors: CLI, launch, crash, Duo, tmux, `org.toml`, directory, hook silent | 1 | T11–T16, T23 |
| §8 Errors: worktree, no notifier | 2 | 2.10, 2.12, 2.13 |
| §8 Errors: message undeliverable | 3 | 3.4 |
| §8 Errors: no agent CLI at first run (D28) | 4 | 4.3 |
| §9 Testing: core, adapters, tmux, store, TUI at 80×24 and 160×48, key tests | 1 | T6–T29 |
| §9 Testing: mouse tests for the wheel and click to select (P2-D25) | 2 | 2.21 |
| §9 Testing: mouse tests for every other action | 4 | 4.7 |
| §9 Testing: git integration tests | 2 | 2.14 |
| §9 Testing: end-to-end story | 3 | 3.12 |
| §10 Benchmark: basic | 1 | T28 |
| §10 Benchmark: live view | 2 | 2.18 |
| §10 Benchmark: full and tuning | 4 | 4.1, 4.2 |
| §10 Benchmark: release gate | 5 | 5.2 |
| §11 CI | 0 | T2 |
| §11 Dependencies: Ratatui and crossterm pinned, MSRV declared | 0 | T1 |
| §11 Release: `cargo-dist`, Homebrew, crate | 5 | 5.3a, 5.3b |
| §12 Delivery mapping | 5 | 5.9 (final check); kept in step by each phase's closing task: 2.20, 3.12, 4.10 |

## Phase-scoped adaptations

Interim behaviour that a later phase removes. Rows marked removed were taken out by the task named (phase 3, 2026-10-18). They are listed in the plans and here, never in the UI spec, which describes the final screens.

| Adaptation | Introduced | Removed by |
|---|---|---|
| D7: attach hint and side pane | phase 1 | 2.5, 2.7 (P2-D3), removed |
| D7 / P2-D3: empty-org and empty-role copy without "ask Duo" | phase 1 | 3.9, removed |
| P2-D3: role summary without `space give work to this role` | 2.5 | 3.11, removed |
| D14: free-text model and effort | phase 1 | 4.5 |
| D28: no full-screen "no agent CLI" message; a first run starts a blank Claude Code org | phase 1 | 4.3, removed |
| P2-D8: chat pane sends to Duo as Give work | 2.5 | 3.9, removed |
| P2-D10: no Give work to a role; no Send as message instead | 2.15 | 3.11, removed |
| P2-D25: clicks ignored in dialogs, confirmations, Settings and Worktrees | 2.21 | 4.7 (P4-D2) |
| P2-D13: Settings without the Keys row and the Org group's `New org from template…` row (the Org heading and its note show) | 2.17 | 4.4, 4.6 |
| P2-D9: no `?` (footer and header), `f`, `m` or mouse beyond P2-D25's wheel and click | phase 2 | 4.6–4.9 |
| T23: errored-row hint without `space retry` | phase 1 | 2.15 |

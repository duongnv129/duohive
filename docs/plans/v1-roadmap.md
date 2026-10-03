# DuoHive v1 — Roadmap

**Status:** Approved 2026-10-03 · Living document.
Sources: [PRD](../product/prd.md) · [UI/UX spec](../ux/ui-spec.md) · [Technical design](../design/technical-design.md). Each phase gets its own detailed, test-first plan in `docs/plans/` when it starts.
v1 supports two agent CLIs, Claude Code and OpenCode; Codex is deferred to after v1 (PRD What We're NOT Building).

## Conventions (set in the first commit; there is no earlier code)

| Category | Convention |
|---|---|
| Naming | Crates `duohive-*`; snake_case modules; domain types use the spec's terms (`Agent`, `Role`, `Team`, `GiveWork`, `ApprovalCard`) |
| Errors | `thiserror` per crate; no `unwrap` outside tests; errors become events the UI shows on rows (TD §8) |
| Logging | `tracing` to `~/.local/share/duohive/logs`, never the terminal |
| Data access | Only through the `Store` port; SQLite via `rusqlite`; migrations append-only |
| Tests | Unit tests beside the code; `insta` snapshots for TUI frames and launch commands; tmux integration tests on a throwaway socket |

## Phases

| # | Phase | Delivers | Done when | Estimate |
|---|---|---|---|---|
| 0 | Foundations and spikes | Cargo workspace (7 crates), toolchain, licence, CI; Makefile, cargo-deny, typos and community files (README, CONTRIBUTING, code of conduct, security policy); spike S0 (tmux activity subscriptions at 100 windows); spikes S1 Claude Code and S2 OpenCode (per-launch instructions, hooks, MCP, model lists, reply extraction) | Spike report written; TD updated where a mechanism changed | 3–5 days |
| 1 | Org and agents | Core domain (org, inheritance, validation); engine (command/event loop, full state machine, Give work by state, quiet, Needs you); store (`org.toml`, `settings.toml`, `state.db`); tmux runtime; the Claude Code and OpenCode adapters and `duohive hook`; minimal TUI (tree with live status, forms); basic `bench` | The author's daily agents run in DuoHive with correct status; `bench` reports at 10 agents | 2–3 weeks |
| 2 | One screen | Three-pane layout; agent pane (Selected and Typing modes, zoom); Needs you you can act on; desktop alerts; Give work with worktrees and merge clean-up; friction log, then the UI/UX improvement pass | A day's work needs no `tmux attach` | 2 weeks |
| 3 | Duo and messaging | MCP endpoint and helper; per-launch tokens and tool rights; inbox; Duo, Duo chat and `→` lines; approval cards; task to a role | A "what's blocked, then fix it" round trip happens entirely in chat; a task given to a role reaches a free agent | 2–3 weeks |
| 4 | Scale and polish | `bench` at 100 agents and tuning; first run with templates; model and effort picker; key-binding editor; key and mouse completeness | Every Success Metric target passes on Apple M-series | 1–2 weeks |
| 5 | Open-source launch | Pre-launch UX check; `cargo-dist` release, Homebrew tap, crate; README finished for launch; name checks | DuoHive is public; 30-day daily use and < 10 minutes to a first team | 1 week + 30-day test |

Total: roughly 9–12 weeks of focused work.

## Dependencies

- tmux 3.2 or newer; Claude Code and OpenCode at pinned versions for spikes S1 and S2.
- Crates: `ratatui`, `crossterm`, `tokio`, `rusqlite`, `toml`, `serde`, `thiserror`, `tracing`, `insta`, `vt100` (or similar); an MCP server crate chosen in phase 3.

## Risks

| Level | Risk | Response |
|---|---|---|
| High | Spike S0 fails: subscriptions don't report for paused panes cheaply | S0 runs first; fallback `list-windows`, re-measured against the 100-agent targets |
| High | A CLI lacks a per-launch hook or instructions mechanism | Fall back to screen reading (`~`) for that CLI; note it in the TD |
| Medium | Duo reply extraction differs per CLI and drifts between versions | Pinned versions, recorded fixtures, version notices |
| Medium | Ratatui pre-1.0 breaking changes | Pinned version, deliberate upgrades |
| Medium | Squash-merge detection accuracy | Patch-id comparison, tested against real squash merges |
| Low | macOS alerts can't report clicks without a helper | Documented; helper is a later item |
| Low | Users who run Codex can't manage those agents in v1 | Codex is deferred to after v1; adding it is one adapter and one spike behind the `CliAdapter` port |

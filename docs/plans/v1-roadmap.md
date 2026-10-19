# DuoHive v1 — Roadmap

**Status:** Approved 2026-10-03 · Living document · Phases 0 and 1 done 2026-10-04. Phase 2 done 2026-10-19 (code and UI/UX pass 2026-10-17, then the three-day run without `tmux attach`).
Sources: [PRD](../product/prd.md) · [UI/UX spec](../ux/ui-spec.md) · [Technical design](../design/technical-design.md). Roadmap phases 0 and 1 together are PRD and TD phase 1. Each phase gets its own detailed, test-first plan in `docs/plans/` when it starts: [phase 0](phase-0-plan.md) and [phase 1](phase-1-plan.md) (done) · [phase 2](phase-2-plan.md) and [phase 3](phase-3-plan.md) (full) · outlines for [phase 4](phase-4-plan.md) and [phase 5](phase-5-plan.md), expanded when each starts. [Traceability](v1-traceability.md) maps every requirement to its phase and task. Features agreed for after v1 are tracked in the [v1.1 roadmap](v1.1-roadmap.md).
v1 supports two agent CLIs, Claude Code and OpenCode; Codex is deferred to after v1 (PRD What We're NOT Building).

## Conventions (set in the first commit; there is no earlier code)

| Category | Convention |
|---|---|
| Naming | Crates `duohive-*`; snake_case modules; domain types use the spec's terms (`Agent`, `Role`, `Team`, `GiveWork`, `ApprovalCard`) |
| Errors | `thiserror` per crate; no `unwrap` outside tests; errors become events the UI shows on rows (TD §8) |
| Logging | `tracing` to `~/.local/share/duohive/logs`, never the terminal |
| Data access | Only through the `Store` port; SQLite via `rusqlite`; migrations append-only |
| Tests | Unit tests beside the code; `insta` snapshots for TUI frames and launch commands; tmux integration tests on a throwaway socket |

## Planning

How each phase's plan is written and kept consistent with the PRD, the UI/UX spec and the technical design (agreed 2026-10-05).

| Topic | Rule |
|---|---|
| Depth | The next phase gets a full test-first plan. Later phases get an outline (goal, done-when, spec coverage, tasks with files, test intent and spec references, risks, open questions), expanded to full detail when the phase starts. A full plan written too early would rest on code that doesn't exist yet |
| Format | A full plan follows [phase-2-plan.md](phase-2-plan.md): header, Global Constraints, Review Focus, File Structure, decisions where the specs are silent, then tasks with failing tests first and checkbox steps |
| Decisions | Numbered per phase: `D1…` (phases 0–1), `P2-D1…`, and so on. Outlines list open questions as `P3-Q1…`, each with a default. Earlier decisions stay in force unless a later plan says which one it replaces |
| Traceability | [v1-traceability.md](v1-traceability.md) gives every PRD requirement, UI spec section and TD section exactly one owning phase and task. A plan that moves a requirement updates it in the same commit |
| Conflicts | Settled by the precedence in [docs/README.md](../README.md): the PRD decides *what*, the UI/UX spec *how it looks*, the technical design *how it's built*. A change to a PRD requirement goes to the author first; wording and status fixes are made in place |
| Verification | Before a plan is used, two independent reviewers check it against one rubric: traceability, scope against this roadmap, spec fidelity, decisions, feasibility against the code and `AGENTS.md`, plan quality, internal consistency. Findings are fixed and re-reviewed, for up to three rounds; anything still open goes to the author |

## Phases

| # | Phase | Delivers | Done when | Estimate | Status |
|---|---|---|---|---|---|
| 0 | Foundations and spikes | Cargo workspace (7 crates), toolchain, licence, CI; Makefile, cargo-deny, typos and community files (README, CONTRIBUTING, code of conduct, security policy); spike S0 (tmux activity subscriptions at 100 windows); spikes S1 Claude Code and S2 OpenCode (per-launch instructions, hooks, MCP, model lists, reply extraction) | Spike report written; TD updated where a mechanism changed | 3–5 days | done 2026-10-04 |
| 1 | Org and agents | Core domain (org, inheritance, validation); engine (command/event loop, full state machine, Give work by state, quiet, Needs you); store (`org.toml`, `settings.toml`, `state.db`); tmux runtime; the Claude Code and OpenCode adapters and `duohive hook`; minimal TUI (tree with live status, forms); basic `bench` | The author's daily agents run in DuoHive with correct status; `bench` reports at 10 agents | 2–3 weeks | done 2026-10-04 |
| 2 | One screen | Three-pane layout; agent pane (Selected and Typing modes, zoom); Needs you you can act on; desktop alerts and their Settings; Give work with worktrees and merge clean-up; touchpad scrolling and click to select; theme `auto` follows the terminal; quiet colour; the OpenCode 2.0 port; friction log, then the UI/UX improvement pass | A day's work needs no `tmux attach` | 2 weeks | done 2026-10-19: code and UI/UX pass 2026-10-17; the three-day run without `tmux attach` passed; resume after a tmux server exit carried to v1.1 |
| 3 | Duo and messaging | MCP endpoint and helper; token-checked tool rights (tokens exist since phase 1); inbox; Duo, Duo chat and `→` lines; approval cards; task to a role | A "what's blocked, then fix it" round trip happens entirely in chat; a task given to a role reaches a free agent | 2–3 weeks | done 2026-10-18: acceptance on Claude Code and OpenCode, see [bench results](bench-results.md#phase-3-acceptance) |
| 4 | Scale and polish | `bench` at 100 agents and tuning; first run with templates; model and effort picker; key-binding editor; key and mouse completeness (`?`, filter, move, the mouse beyond phase 2's wheel and click); the accessibility re-check | Every benchmark target in Success Metrics (idle CPU, memory, status freshness, start-up, binary size) passes on Apple M-series | 3–4 weeks (P4-D4: the mouse on every screen and the keymap layer) | next: outline in [phase 4](phase-4-plan.md), full plan when it starts |
| 5 | Open-source launch | Pre-launch UX check; CI release gate; `cargo-dist` release, Homebrew tap, crate; README finished for launch; name checks | DuoHive is public; 30-day daily use and < 10 minutes to a first team | 1 week + 30-day test | pending |

Total: roughly 9–12 weeks of focused work.

## Dependencies

- tmux 3.2 or newer; Claude Code and OpenCode at pinned versions for spikes S1 and S2.
- Crates: `ratatui`, `crossterm`, `tokio`, `rusqlite`, `toml`, `serde`, `thiserror`, `tracing`, `insta`, `vt100` (pinned in phase 2); no MCP server crate (phase 3: `rmcp` took the binary over 10 MB, so the helper is a hand-written JSON-RPC over stdio, P3-D8).

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

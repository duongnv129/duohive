# Guidance for coding agents

DuoHive is a Rust/Ratatui terminal app that manages standing AI coding agents as an org chart.

## Read order

1. `docs/README.md`, then the doc it points to for your task.
2. The task in the current phase's plan in `docs/plans/` (see the roadmap) and its Global Constraints section. Phase 0's [Global Constraints](docs/plans/phase-0-plan.md#global-constraints) apply to every phase.

## Where features are specified

- Every feature and behaviour change lives in the three living documents, edited in place: the PRD (what), the UI/UX spec (how it looks and behaves) and the technical design (how it is built). Update them in the same change as the code.
- Do not write a separate spec or design document for a feature or a change (no `docs/superpowers/specs/`, no per-feature design files). Brainstorming and design conversations end in edits to those three documents.
- A file in `docs/plans/` is only an implementation plan: tasks, tests and order. It argues from the three documents and records decisions that supersede older ones (with an id), and it never holds the feature's specification.

## The gate

`make check` (format check, clippy, tests, `cargo deny`, `typos`) must pass before every commit. CI runs the same commands.

## Constraints

- Toolchain 1.99.0, edition 2024, licence Apache-2.0.
- No `unwrap`/`expect` outside tests; no `unsafe`, except `crates/duohive/src/bench/measure.rs` (`getrusage`, `getloadavg`), each block under `#[expect(unsafe_code, reason = "…")]`.
- Single instance is per data directory (`run.lock`); two instances with different `XDG_DATA_HOME` still share the `tmux -L duohive` server.
- `duohive-core` does no I/O.
- Ratatui and crossterm are pinned with `=`.
- No `std::env::set_var` in tests; tmux tests use a throwaway socket, never `duohive`.
- Never modify the user's global CLI configuration.
- Supported CLIs: Claude Code and OpenCode only.
- Test first, except for infrastructure tasks.

## Dependency rule (TD section 3.4)

`duohive-core` depends on no `duohive-*` crate. `duohive-tmux`, `duohive-cli`, `duohive-store`, `duohive-tui` and `duohive-mcp` depend only on `duohive-core`. Only `duohive` depends on all of them.

## Commits

Conventional commits (`feat:`, `fix:`, `docs:`, `test:`, `chore:`, `ci:`, `refactor:`, `perf:`). Never add a `Co-Authored-By` line. Never push.

# Guidance for coding agents

DuoHive is a Rust/Ratatui terminal app that manages standing AI coding agents as an org chart.

## Read order

1. `docs/README.md`, then the doc it points to for your task.
2. The task in the current phase's plan in `docs/plans/` (see the roadmap) and its Global Constraints section. Phase 0's [Global Constraints](docs/plans/phase-0-plan.md#global-constraints) apply to every phase.

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

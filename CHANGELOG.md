# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

Phases 0–4 built; the first release is v1.0.0 (see [the release runbook](docs/release-runbook.md)).

### Added since phase 1

- One screen (phase 2): a three-pane layout with the org tree, a live agent pane (watch, type into it, zoom) and *Needs you*; desktop alerts; Give work with optional git worktrees and merge clean-up; touchpad scrolling and click to select.
- Duo and messaging (phase 3): the `duohive mcp` helper each agent runs, token-checked tool rights, an inbox with delivery by paste, Duo and the Duo chat, visible `→` actions, approval cards, and task to a role.
- Scale and polish (phase 4): first-run setup with Agent Squad, Product Team and Engineering Org templates; *New org from template* (replacing an org keeps a timestamped `org.toml` backup); a model and effort picker that reads each CLI's own lists; a key map (`?`) and a key-binding editor that refuses clashes; a filter by state; *Move to…*; a right-click menu and drag; mouse support on every screen; an accessibility re-check; and a benchmark at 100 agents.
- `duohive bench --gate`: the release gate, three counted runs each at 10 and 100 agents with the worst value per target deciding. `scripts/bench-gate.sh` and `make bench-gate` run it.
- Release tooling: `scripts/set-version.sh`, `scripts/check-release-version.sh`, the release runbook, and (in a follow-up change) a tag-triggered release workflow: build, attest, GitHub Release, crates.io and Homebrew tap behind a protected environment.

### Changed

- A plain drag in the agent pane, the chat or Needs you selects text and copies it on release (no `⌥` needed); dragging an org row still moves it.

### Earlier

Phase 0 and Phase 1 done (2026-10-04).

### Added

- `make install` / `install.sh`: build in release mode and install `duohive` and the `dh` alias to `~/.local/bin` (or `PREFIX`); `make uninstall` removes them.

- Cargo workspace with seven crates, a pinned toolchain, shared lints and the Apache-2.0 licence.
- Makefile with `make check` as the local gate, plus cargo-deny and typos configuration.
- CI on Linux and macOS against tmux 3.2a and the latest tmux release.
- Contributing guide, code of conduct, security policy, issue and pull request templates, and Dependabot configuration.
- Phase 1, the core loop: an org of teams, roles and agents in `org.toml`, edited in the app or by hand (a broken hand edit is never overwritten).
- Agents on Claude Code and OpenCode, each in its own tmux window on DuoHive's own server, started and stopped from the TUI; stopping a waiting agent sends no keys, so its open prompt is never answered.
- Live states from each CLI's hooks (`duohive hook`), with screen reads where no hook exists; quiet, errored and Needs you; the last reply under each agent.
- Quitting leaves agents running; the next start reconnects and replays hooks fired while DuoHive was closed. A second instance on the same data directory is refused.
- `duohive bench` measures CPU, memory and hook latency with fake agents.

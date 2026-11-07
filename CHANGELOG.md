# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.1.0-alpha.3] - 2026-11-07

### Added

- Resume an old session: `r` on a stopped or errored agent (or *Resume…* in its right-click menu) opens a picker of that agent's earlier Claude Code or OpenCode sessions, from its directory and its kept worktrees, including sessions started outside DuoHive. The agent continues the one you pick with its full context. A session another agent is using is refused. `r` is a rebindable key.
- Click to type: one click on an agent in the org tree, in *Needs you* or in the agent pane, or `enter` on its row, starts typing into it. A click counts only if the pointer did not move, so dragging a node and selecting text work as before. When the agent's terminal is still opening, typing starts as soon as it shows, and until then DuoHive's own keys are ignored so a fast typist cannot give work or zoom by accident.

### Changed

- `enter` on an agent row now starts typing; it used to focus the agent pane and need a second `enter`. `tab` and the arrow keys still never start typing. Stopped and errored agents are unchanged, and a click on one only selects it.
- Dependencies: `rusqlite` 0.41, `vt100` 0.18 and the minor and patch group. GitHub Actions are pinned to commit SHAs.
- The macOS jobs in CI and the Apple-silicon release build run on `macos-15`; GitHub no longer provides the `macos-14` image.

## [0.1.0-alpha.2] - 2026-10-25

### Added

- A release workflow: pushing a `v*` tag builds `duohive` on GitHub for macOS (Apple silicon and Intel) and Linux (x86_64 and ARM64) and attaches the archives, their checksums and a build attestation to the tag's release. This is the first release made that way.
- A benchmark report in CI: the full gate (10 and 100 agents, three counted runs each) runs on a GitHub macOS runner on pushes to `main` and by hand, and publishes its table in the job summary.

### Fixed

- `duohive bench --gate` keeps the tiers it finished when a later one fails, and says the run is incomplete instead of printing nothing. Its start, turn and stop waits grow with the agent count, so 100 agents can finish on a small machine.

### Changed

- The README is shorter and easier to scan.

## [0.1.0-alpha.1] - 2026-10-24

The first pre-release: binaries for macOS (Apple silicon and Intel) and Linux (x86_64 and ARM64), built by the release workflow, and the source on GitHub. It is alpha: features and file formats can still change, and the 100-agent benchmark acceptance is not finished. Phases 0–4 are built (see [the roadmap](docs/plans/v1-roadmap.md)); crates.io, Homebrew and the v1.0.0 release come later ([release runbook](docs/release-runbook.md)).

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

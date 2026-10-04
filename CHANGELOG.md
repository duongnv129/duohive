# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Cargo workspace with seven crates, a pinned toolchain, shared lints and the Apache-2.0 licence.
- Makefile with `make check` as the local gate, plus cargo-deny and typos configuration.
- CI on Linux and macOS against tmux 3.2a and the latest tmux release.
- Contributing guide, code of conduct, security policy, issue and pull request templates, and Dependabot configuration.
- Phase 1, the core loop: an org of teams, roles and agents in `org.toml`, edited in the app or by hand (a broken hand edit is never overwritten).
- Agents on Claude Code and OpenCode, each in its own tmux window on DuoHive's own server, started and stopped from the TUI; stopping a waiting agent sends no keys, so its open prompt is never answered.
- Live states from each CLI's hooks (`duohive hook`), with screen reads where no hook exists; quiet, errored and Needs you; the last reply under each agent.
- Quitting leaves agents running; the next start reconnects and replays hooks fired while DuoHive was closed. A second instance on the same data directory is refused.
- `duohive bench` measures CPU, memory and hook latency with fake agents.

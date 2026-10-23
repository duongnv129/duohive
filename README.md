# DuoHive

**Run your AI coding agents as a team, not a pile of terminals.**

DuoHive is an open-source terminal app (TUI, written in Rust) that manages **Claude Code** and **OpenCode** agents as an org chart on top of tmux. You see who is working, who is waiting on you and who has finished, and you talk to **Duo**, a manager agent, to hand out work.

[![CI](https://github.com/duongnv129/duohive/actions/workflows/ci.yml/badge.svg)](https://github.com/duongnv129/duohive/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/duongnv129/duohive?include_prereleases&label=release)](https://github.com/duongnv129/duohive/releases)
[![License: Apache-2.0](https://img.shields.io/github/license/duongnv129/duohive)](LICENSE)
![Platforms: macOS and Linux](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux-blue)

> **Alpha.** DuoHive works from source and as a macOS binary, but it is early: features and file formats can still change, and the 100-agent benchmark acceptance is not finished. See the [roadmap](docs/plans/v1-roadmap.md).

```text
 DuoHive   6 agents · ● 2 working · ◆ 2 need you                                  ? help
┏━ Org ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓┌─ auth-service · Claude ──────────────┐
┃ ⬢ Duo · your manager agent    ◦ idle       1m  ┃│ ✓ go test ./... (14 passed)          │
┃ ├─ ▣ Payments                 ● 1 ◆ 1          ┃│ ✓ Ready for review · 3 files changed │
┃▌│  └─ auth-service ✉ 1        ● working    2m  ┃│ ❯                                    │
┃ │     billing-api             ◆ waiting    4m  ┃│   (live terminal of the selected     │
┃ └─ ◇ Generalist                                ┃│    agent)                            │
┣━ Needs you (1) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫│                                      │
┃ ◆ billing-api  waiting    4m  "Allow edit to…" ┃│                                      │
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└──────────────────────────────────────┘
┌─ ⬢ Duo · ◦ idle ────────────────────────────────────────────────────────────────────────┐
│ you   what's blocking the Payments team?                                                │
│ Duo   billing-api is waiting on a file-edit approval; auth-service is mid-test.         │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

## Highlights

- **An org of standing agents.** Teams, roles and agents, each with its own name, instructions, directory and CLI. Start from a template (Agent Squad, Product Team, Engineering Org) or build your own.
- **One glance answers "who needs me?"** Every agent shows working, waiting, finished, quiet or errored. The *Needs you* pane collects what is blocked on you.
- **Duo, the manager agent.** Ask in plain language what is blocked, or hand work to a role. Anything that changes the org is a card you approve first.
- **Watch and steer any agent.** Select one to see its live terminal, type into it, zoom it, or give it a task, optionally in its own git worktree. No `tmux attach`.
- **Agents keep running.** Quit DuoHive and they carry on in tmux; the next start reconnects.
- **Keyboard and mouse.** Every action has a key (rebindable; press `?`) and a mouse path. Drag to select and copy text, drag a row to move it.
- **Private.** No telemetry, and your global CLI configuration is never modified.

## Quick start

**1. Install** (details below)

```sh
git clone https://github.com/duongnv129/duohive.git && cd duohive
make install            # builds in release mode, installs duohive (and the dh alias) to ~/.local/bin
```

**2. Run it**

```sh
duohive
```

On first run it checks for tmux and for Claude Code or OpenCode, then offers a template. Pick one, choose the CLI and directories, and select *Create org*.

**3. Give work.** Select an agent and press `space`. Watch it in the agent pane (`enter` types into it, `z` zooms). When an agent needs you, it shows up in *Needs you*; ask Duo what is blocked.

## Install

| How | Command |
|---|---|
| **Alpha binary** (macOS, Apple silicon) | Download `duohive-<version>-aarch64-apple-darwin.tar.gz` from [Releases](https://github.com/duongnv129/duohive/releases), unpack it and put `duohive` on your `PATH`. If macOS blocks it: `xattr -d com.apple.quarantine duohive` |
| **From source** (macOS, Linux) | `make install` |

<details>
<summary>Installer options</summary>

`make install` installs tmux (with Homebrew, apt, dnf, yum, pacman, zypper or apk) and Rust (with rustup) when they are missing; `make deps` installs only those.

```sh
make install PREFIX=/usr/local    # another prefix (or: ./install.sh --prefix DIR)
make uninstall                    # remove them; your org and data are kept
```

`./install.sh --no-alias` skips `dh`; an existing `dh` that isn't DuoHive's is left alone. `--no-deps` only checks tmux and Rust. The `dh` alias comes only from `install.sh`.

</details>

**Requirements:** macOS or Linux (no Windows: agents run in tmux), tmux 3.2 or later, and Claude Code or OpenCode for the agents (you install and sign in to them). Building needs Rust 1.99 (`rustup` selects it from `rust-toolchain.toml`).

## Keys

| Key | Does |
|---|---|
| `↑` `↓` / `tab` | move in the tree / change pane |
| `space` | give work to the selected agent |
| `s` · `z` | start or stop · zoom the agent pane |
| `n` · `e` · `m` | new · edit · move |
| `w` · `,` · `o` | worktrees · settings · open `org.toml` |
| `?` | the full key map (every key can be rebound in Settings) |
| `q` | quit (agents keep running) |

`duohive bench` measures CPU, memory and hook latency with fake agents; `duohive bench --gate` runs the full benchmark gate. `duohive --help` lists the commands.

## How it works

- **tmux underneath.** DuoHive runs its own tmux server (`tmux -L duohive`) with one window per agent; the agent pane is a live view of that window. List them with `tmux -L duohive ls`.
- **Live state from the CLIs.** Claude Code and OpenCode call `duohive hook` on their lifecycle events, which report to the running app over a local Unix socket. Where a CLI has no hook, DuoHive reads the screen.
- **Duo talks MCP.** Each agent gets a `duohive mcp` helper (a stdio MCP server) with token-checked tools: list agents, send a message, check its delivery. Duo's rights are wider.
- **Plain files.** `org.toml` is your org (hand-editable; a broken edit is never overwritten), `settings.toml` is personal, and `state.db` (SQLite) holds runtime state, all under `~/.local/share/duohive` or `$XDG_DATA_HOME/duohive`. One DuoHive runs per data directory.
- **A Rust workspace** of seven crates around a core with no I/O. See the [technical design](docs/design/technical-design.md).

## FAQ

**Is it a tmux replacement?** No. It uses tmux to keep agents alive and shows their terminals in its own panes.

**Which agent CLIs work?** Claude Code and OpenCode. Others (for example Codex) are not supported yet.

**Does it send data anywhere?** No. Your agents talk to their own providers as before.

## Contributing

Issues and pull requests are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md); `make check` runs the same checks as CI. Report security problems as described in [SECURITY.md](SECURITY.md).

More: [docs](docs/README.md) (requirements, UI/UX spec, technical design, plans) · [CHANGELOG](CHANGELOG.md) · [release runbook](docs/release-runbook.md) · [Apache-2.0 licence](LICENSE)

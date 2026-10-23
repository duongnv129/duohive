# DuoHive: manage Claude Code and OpenCode agents as a team in your terminal

**DuoHive is an open-source terminal app (TUI, written in Rust) that runs your AI coding agents as an org chart.** Teams, roles and standing agents live in one screen, on top of tmux. You see who is working, who is waiting on you and who has finished, and you talk to **Duo**, a manager agent, to hand out work. It supports **Claude Code** and **OpenCode**.

[![CI](https://github.com/duongnv129/duohive/actions/workflows/ci.yml/badge.svg)](https://github.com/duongnv129/duohive/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/github/license/duongnv129/duohive)](LICENSE)
![Rust 1.99](https://img.shields.io/badge/rust-1.99-orange)
![Platforms: macOS and Linux](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux-blue)

> **Status: pre-alpha · phases 0–4 built; phase 5 (open-source launch) is in preparation.** Phase 4's acceptance (the 100-agent gate run, a one-hour run and a screen walk) is still open, so there is no release yet. The install channels below marked *from v1.0.0* do not work today; building from source does.

## Why DuoHive

Running several AI coding agents usually means a pile of terminal tabs or tmux panes: you lose track of which agent is stuck on a question, which one finished, and which prompt went to which directory. DuoHive gives that pile a structure.

- **An org of standing agents.** Teams, roles and agents, each with its own name, instructions, directory and CLI, kept over days rather than started per task. Start from a template (Agent Squad, Product Team, Engineering Org) or build your own.
- **Live status at a glance.** Every agent shows working, waiting, finished, quiet or errored, with its last reply. A *Needs you* pane collects what is blocked on you, so one glance answers "who needs me?".
- **Duo, the manager agent.** Chat with Duo in plain language; it can see the whole org, message any agent and start or stop them. Anything that changes the org is a card you approve first.
- **Watch and steer any agent.** Select an agent to see its live terminal, type into it, zoom it, or give it a task, optionally in its own git worktree. No `tmux attach`.
- **Your agents keep running.** Each agent lives in a window on DuoHive's own tmux server. Quit DuoHive and the agents carry on; the next start reconnects.
- **Keyboard and mouse.** Every action has a key (rebindable in Settings, `?` shows them all) and a mouse path. Drag to select and copy text, drag a row to move it.
- **Private and light.** No telemetry, and your global CLI configuration is never modified. Designed to stay light from 10 to 100 agents; the targets (idle CPU under 1% and under 20 MB at 10 agents) are checked by a release benchmark.

```text
 DuoHive   6 agents · ● 2 working · ◆ 2 need you                                  ? help
┏━ Org ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓┌─ auth-service · Claude ──────────────┐
┃ ⬢ Duo · your manager agent    ◦ idle       1m  ┃│ ✓ go test ./... (14 passed)          │
┃ ├─ ▣ Payments                 ● 1 ◆ 1          ┃│ ✓ Ready for review · 3 files changed │
┃ │  └─ ◇ Backend                                ┃│                                      │
┃▌│     ├─ auth-service ✉ 1     ● working    2m  ┃│ ❯                                    │
┃ │     └─ billing-api          ◆ waiting    4m  ┃│   (live terminal of the selected     │
┃ └─ ◇ Generalist                                ┃│    agent)                            │
┃    └─ squad-1                 ● working    1m  ┃│                                      │
┣━ Needs you (2) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫│                                      │
┃ ◆ billing-api  waiting    4m  "Allow edit to…" ┃│                                      │
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└──────────────────────────────────────┘
┌─ ⬢ Duo · ◦ idle ────────────────────────────────────────────────────────────────────────┐
│ you   what's blocking the Payments team?                                                │
│ Duo   billing-api is waiting on a file-edit approval; auth-service is mid-test.         │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

*A sketch of the main screen from the [UI spec](docs/ux/ui-spec.md).*

## How it works

- **tmux underneath.** DuoHive starts its own tmux server (`tmux -L duohive`) and puts each agent in its own window. The terminal you see in the agent pane is a live view of that window.
- **Live state from the CLIs.** Claude Code and OpenCode call `duohive hook` on their lifecycle events; the hook reports to the running app over a local Unix socket, and where a CLI has no hook, DuoHive reads the screen.
- **Duo talks MCP.** Each agent gets a small `duohive mcp` helper (a stdio MCP server) with token-checked tools: list agents, send a message and check its delivery. Duo's rights are wider (it can also read and wait on other agents).
- **Plain files.** Your org is `org.toml` (hand-editable; a broken edit is never overwritten), personal settings are `settings.toml`, and runtime state is a SQLite `state.db`.
- **A Rust workspace.** Seven crates around a core with no I/O: `duohive-core`, `-tmux`, `-cli`, `-store`, `-tui` (Ratatui), `-mcp` and the `duohive` binary. See the [technical design](docs/design/technical-design.md).

## Requirements

- macOS or Linux (Windows is not supported: DuoHive runs your agents in tmux)
- tmux 3.2 or later
- Claude Code or OpenCode, for the agents (you install and sign in to them yourself)
- Rust 1.99, only to build from source (`rustup` selects it from `rust-toolchain.toml`)

## Install

| Channel | Command | Available |
|---|---|---|
| Homebrew | `brew install duongnv129/tap/duohive` (installs tmux with it) | from v1.0.0 |
| crates.io | `cargo install duohive` | from v1.0.0 |
| Release binary | download the archive for your platform from the GitHub Releases page, unpack it and put `duohive` on your `PATH` | from v1.0.0 |
| From source | `make install` (or `./install.sh`) | now |

Release binaries are not notarised in v1. If macOS blocks a downloaded `duohive`, run `xattr -d com.apple.quarantine duohive` once; `brew` and `cargo install` are not affected.

`make install` builds in release mode and installs `duohive` and `dh` to `~/.local/bin`; it installs tmux (with Homebrew, apt, dnf, yum, pacman, zypper or apk) and Rust (with rustup) when they're missing. `make deps` installs only those.

```sh
make install                     # build in release mode, install duohive + dh to ~/.local/bin
make install PREFIX=/usr/local    # or another prefix (or: ./install.sh --prefix DIR)
make uninstall                   # remove them; your org and data are kept
```

`./install.sh --no-alias` skips `dh`; an existing `dh` that isn't DuoHive's is left alone. `--no-deps` only checks tmux and Rust instead of installing them. The `dh` alias comes only from `install.sh`; the other channels install `duohive`.

## Your first team in ten minutes

1. Run `duohive`. On first run it checks for tmux and for Claude Code and OpenCode, and tells you what to install if something is missing.
2. Pick a template: **Agent Squad** (a pool of generalist agents that any free one picks tasks from), **Product Team** (Backend, Frontend and QA, one agent each) or **Engineering Org** (several teams, each with Backend and QA). Choose the CLI and directories it asks for, then *Create org*.
3. Select an agent and press `space` to give it work. Watch it in the agent pane; press `enter` in the pane to type into it.
4. When an agent needs you (a question, an approval), it appears in *Needs you*. Chat with Duo to ask what is blocked or to hand work to a role.

Press `?` at any time for the full key map; every key can be changed in *Settings*, and every action also works with the mouse.

## Build

```sh
make release  # target/release/duohive
make tools   # once: install the pinned dev tools
make check   # format, lint, test, dependency and spelling checks
```

## Usage

```sh
duohive          # open the org: ↑↓ move, tab pane, space give work, s start/stop, z zoom,
                 # n new, e edit, w worktrees, , settings, o open org.toml, ? keys, q quit
duohive bench    # measure CPU, memory and hook latency with fake agents
duohive bench --gate   # the release gate: the PRD targets at 10 and 100 agents
duohive --help
```

- Each agent runs in its own window on DuoHive's tmux server. `q` quits DuoHive and leaves your agents running; the next `duohive` reconnects to them. On quit, DuoHive says how many are still running and how to list them (`tmux -L duohive ls`).
- Select text with a plain drag in the agent pane, the chat or Needs you; releasing copies it to the clipboard. Dragging an org row moves it, and the mouse also clicks, scrolls and opens the right-click menu.
- To watch an agent, select it: the agent pane shows its terminal live. `enter` in the agent pane types into it (`ctrl+q` stops), `z` zooms and `space` gives it work. You don't need `tmux attach`.
- One DuoHive runs per data directory (`~/.local/share/duohive`, or `$XDG_DATA_HOME/duohive`): a second one there is refused. Two DuoHives with different data directories still share the `tmux -L duohive` server, so don't run them side by side.

## FAQ

**What is DuoHive?** A terminal app that manages several AI coding agents (Claude Code, OpenCode) as one team, with live status, a manager agent and approval cards, on top of tmux.

**Is it a tmux replacement?** No. It uses tmux to keep your agents alive and shows their terminals in its own panes. You can still inspect them with `tmux -L duohive ls`.

**Which agent CLIs work?** Claude Code and OpenCode today. Other CLIs (for example Codex) are not supported yet.

**Does it work on Windows?** No. It needs tmux, so macOS and Linux only.

**Does it send data anywhere?** No telemetry. Your agents talk to their own providers exactly as they did before.

**Is it ready to use?** It is pre-alpha: usable from source, with no release yet. See the status note above.

## Contributing

Issues and pull requests are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md); `make check` runs the same checks as CI. To report a security problem, see [SECURITY.md](SECURITY.md).

## Documentation

- [docs/README.md](docs/README.md): product requirements, UI/UX spec, technical design and plans
- [CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md) · [CHANGELOG.md](CHANGELOG.md)
- [docs/release-runbook.md](docs/release-runbook.md): how a release is made and undone
- [LICENSE](LICENSE)

## Licence

Apache-2.0

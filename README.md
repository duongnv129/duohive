# DuoHive

Run your AI coding agents as a team, not a pile of terminals.

[![CI](https://github.com/duongnv129/duohive/actions/workflows/ci.yml/badge.svg)](https://github.com/duongnv129/duohive/actions/workflows/ci.yml)

> **Status: pre-alpha · phases 0–4 built; phase 5 (open-source launch) is in preparation.** Phase 4's acceptance (the 100-agent gate run, a one-hour run and a screen walk) is still open, so there is no release yet. The install channels below marked *from v1.0.0* do not work today; building from source does.

DuoHive is an open-source terminal app that runs your AI coding agents as an org chart, steered from one screen. It is **Duo + your Engineer Factory**: Duo is a manager agent you talk to, and the factory is the standing team of agents it can see and direct.

- **An org of standing agents.** Teams, roles and agents, each with its own name, instructions, directory and CLI, kept over days rather than started per task.
- **Live status at a glance.** See who is working, who is waiting on you and who has finished, across the whole team. A *Needs you* pane collects what is blocked on you.
- **Duo, the manager agent.** Chat with Duo in plain language; Duo can see the whole org, message any agent and start or stop them. Anything that changes the org is a card you approve.
- **Claude Code and OpenCode.** Both are supported in v1. DuoHive never modifies your global CLI configuration.
- **One lightweight process.** Light enough to forget it is running, from 10 to 100 agents. The command is `duohive`, with the short alias `dh` (installed by `install.sh` only).
- **No telemetry.** DuoHive sends nothing anywhere; your agents talk to their own providers as they always did.

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
- Select text with a plain drag in the agent pane, the chat or Needs you; releasing copies it to the clipboard. Dragging an org row moves it, and the mouse also clicks, scrolls and opens the right-click menu. `Settings → Mouse` off hands the mouse to the terminal.
- To watch an agent, select it: the agent pane shows its terminal live. `enter` in the agent pane types into it (`ctrl+q` stops), `z` zooms and `space` gives it work. You don't need `tmux attach`.
- One DuoHive runs per data directory (`~/.local/share/duohive`, or `$XDG_DATA_HOME/duohive`): a second one there is refused. Two DuoHives with different data directories still share the `tmux -L duohive` server, so don't run them side by side.

## Documentation

- [docs/README.md](docs/README.md): product requirements, UI/UX spec, technical design and plans
- [CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md) · [CHANGELOG.md](CHANGELOG.md)
- [docs/release-runbook.md](docs/release-runbook.md): how a release is made and undone
- [LICENSE](LICENSE)

## Licence

Apache-2.0

# DuoHive

Run your AI coding agents as a team, not a pile of terminals.

[![CI](https://github.com/duongnv129/duohive/actions/workflows/ci.yml/badge.svg)](https://github.com/duongnv129/duohive/actions/workflows/ci.yml)

> **Status: pre-alpha, phase 1 (core loop works; daily-use acceptance pending).** Build from source; there are no releases yet.

DuoHive is an open-source terminal app that runs your agents as an org chart, steered from one screen.

- **An org of standing agents.** Teams, roles and agents, each with its own name, instructions, directory and CLI, kept over days rather than started per task.
- **Live status at a glance.** See who is working, who is waiting on you and who has finished, across the whole team.
- **Duo, the manager agent.** Chat with Duo in plain language; Duo can see the whole org, message any agent and start or stop them.
- **Claude Code and OpenCode.** Both are supported in v1.
- **One lightweight process.** Light enough to forget it is running, from 10 to 100 agents. The command is `duohive`, with the short alias `dh`.

## Requirements

- tmux 3.2 or later
- Rust 1.99 to build (`rustup` selects it from `rust-toolchain.toml`)

## Install

```sh
make install                     # build in release mode, install duohive + dh to ~/.local/bin
make install PREFIX=/usr/local    # or another prefix (or: ./install.sh --prefix DIR)
make uninstall                   # remove them; your org and data are kept
```

`./install.sh --no-alias` skips `dh`; an existing `dh` that isn't DuoHive's is left alone.

## Build

```sh
make release  # target/release/duohive
make tools   # once: install the pinned dev tools
make check   # format, lint, test, dependency and spelling checks
```

## Usage

```sh
duohive          # open the org: ↑↓ move, s start/stop, n new, e edit, o open org.toml, q quit
duohive bench    # measure CPU, memory and hook latency with fake agents
duohive --help
```

- Each agent runs in its own window on DuoHive's tmux server. `q` quits DuoHive and leaves your agents running; the next `duohive` reconnects to them. On quit, DuoHive says how many are still running and how to list them (`tmux -L duohive ls`).
- To watch or type into an agent directly, attach to its window: the side pane shows the command, `tmux -L duohive attach -t duohive:<agent>`.
- One DuoHive runs per data directory (`~/.local/share/duohive`, or `$XDG_DATA_HOME/duohive`): a second one there is refused. Two DuoHives with different data directories still share the `tmux -L duohive` server, so don't run them side by side.

## Documentation

- [docs/README.md](docs/README.md): product requirements, UI/UX spec, technical design and plans
- [CONTRIBUTING.md](CONTRIBUTING.md)
- [LICENSE](LICENSE)

## Licence

Apache-2.0

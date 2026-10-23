# DuoHive

Run your AI coding agents as a team, not a pile of terminals.

[![CI](https://github.com/duongnv129/duohive/actions/workflows/ci.yml/badge.svg)](https://github.com/duongnv129/duohive/actions/workflows/ci.yml)

> **Status: pre-alpha · phases 0–3 done (phase 3 on 2026-10-18, phase 2 on 2026-10-19); phase 4 (scale and polish) is built (2026-10-22), with the gate run, the one-hour run and the screen walk still open.** Build from source; there are no releases yet.

DuoHive is an open-source terminal app that runs your agents as an org chart, steered from one screen.

- **An org of standing agents.** Teams, roles and agents, each with its own name, instructions, directory and CLI, kept over days rather than started per task.
- **Live status at a glance.** See who is working, who is waiting on you and who has finished, across the whole team.
- **Duo, the manager agent.** Chat with Duo in plain language; Duo can see the whole org, message any agent and start or stop them.
- **Claude Code and OpenCode.** Both are supported in v1.
- **One lightweight process.** Light enough to forget it is running, from 10 to 100 agents. The command is `duohive`, with the short alias `dh`.

## Requirements

- tmux 3.2 or later
- Rust 1.99 to build (`rustup` selects it from `rust-toolchain.toml`)
- Claude Code or OpenCode, for the agents

`make install` installs tmux (with Homebrew, apt, dnf, yum, pacman, zypper or apk) and Rust (with rustup) when they're missing; `make deps` installs only those. The agent CLIs are yours to install.

## Install

```sh
make install                     # build in release mode, install duohive + dh to ~/.local/bin
make install PREFIX=/usr/local    # or another prefix (or: ./install.sh --prefix DIR)
make uninstall                   # remove them; your org and data are kept
```

`./install.sh --no-alias` skips `dh`; an existing `dh` that isn't DuoHive's is left alone. `--no-deps` only checks tmux and Rust instead of installing them.

## Build

```sh
make release  # target/release/duohive
make tools   # once: install the pinned dev tools
make check   # format, lint, test, dependency and spelling checks
```

## Usage

```sh
duohive          # open the org: ↑↓ move, tab pane, space give work, s start/stop, z zoom,
                 # n new, e edit, w worktrees, , settings, o open org.toml, q quit
duohive bench    # measure CPU, memory and hook latency with fake agents
duohive --help
```

- Each agent runs in its own window on DuoHive's tmux server. `q` quits DuoHive and leaves your agents running; the next `duohive` reconnects to them. On quit, DuoHive says how many are still running and how to list them (`tmux -L duohive ls`).
- To watch an agent, select it: the agent pane shows its terminal live. `enter` in the agent pane types into it (`ctrl+q` stops), `z` zooms and `space` gives it work. You don't need `tmux attach`.
- One DuoHive runs per data directory (`~/.local/share/duohive`, or `$XDG_DATA_HOME/duohive`): a second one there is refused. Two DuoHives with different data directories still share the `tmux -L duohive` server, so don't run them side by side.

## Documentation

- [docs/README.md](docs/README.md): product requirements, UI/UX spec, technical design and plans
- [CONTRIBUTING.md](CONTRIBUTING.md)
- [LICENSE](LICENSE)

## Licence

Apache-2.0

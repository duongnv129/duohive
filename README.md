# DuoHive

Run your AI coding agents as a team, not a pile of terminals.

[![CI](https://github.com/duongnv129/duohive/actions/workflows/ci.yml/badge.svg)](https://github.com/duongnv129/duohive/actions/workflows/ci.yml)

> **Status: pre-alpha, phase 0.** The workspace and tooling are in place; there is nothing to install or run yet.

DuoHive is an open-source terminal app that runs your agents as an org chart, steered from one screen.

- **An org of standing agents.** Teams, roles and agents, each with its own name, instructions, directory and CLI, kept over days rather than started per task.
- **Live status at a glance.** See who is working, who is waiting on you and who has finished, across the whole team.
- **Duo, the manager agent.** Chat with Duo in plain language; Duo can see the whole org, message any agent and start or stop them.
- **Claude Code and OpenCode.** Both are supported in v1.
- **One lightweight process.** Light enough to forget it is running, from 10 to 100 agents. The command is `duohive` (alias `dh`).

## Requirements

- tmux 3.2 or later
- Rust 1.90 to build (`rustup` selects it from `rust-toolchain.toml`)

## Build

```sh
cargo build --release
make check   # format, lint, test, dependency and spelling checks
```

## Documentation

- [docs/README.md](docs/README.md): product requirements, UI/UX spec, technical design and plans
- [CONTRIBUTING.md](CONTRIBUTING.md)
- [LICENSE](LICENSE)

## Licence

Apache-2.0

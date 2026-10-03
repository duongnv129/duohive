# Contributing to DuoHive

Thank you for helping. Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Setup

1. Install [rustup](https://rustup.rs). It picks the toolchain (1.90.0) from `rust-toolchain.toml`.
2. Install tmux 3.2 or later.
3. Run `make tools` to install `cargo-deny`, `typos` and `cargo-llvm-cov`.

`make help` lists every target.

## Workflow

- Write the test first, then the code. The exceptions are infrastructure, spikes and acceptance tasks that have no code under test.
- Run `make tools` once to install the pinned dev tools, then `make check` before every commit. It runs the same commands as CI: format check, clippy, tests, `cargo deny` and `typos`.
- Use conventional commit types: `feat`, `fix`, `docs`, `test`, `chore`, `ci`, `refactor`, `perf`.

## Where the truth lives

Three docs, read in this order of precedence (see [docs/README.md](docs/README.md)):

1. [PRD](docs/product/prd.md) decides what DuoHive does.
2. [UI/UX spec](docs/ux/ui-spec.md) decides how it looks.
3. [Technical design](docs/design/technical-design.md) decides how it is built.

## Dependency rule

From the technical design, section 3.4: `duohive-core` depends on no other `duohive-*` crate. `duohive-tmux`, `duohive-cli`, `duohive-store`, `duohive-tui` and `duohive-mcp` depend only on `duohive-core`. Only the `duohive` binary crate depends on all of them.

## Constraints that matter in review

- No `unwrap` or `expect` outside tests (clippy denies both).
- No `unsafe` code.
- `duohive-core` does no I/O: no filesystem, process, socket or terminal access.
- No `std::env::set_var` in tests. Code that reads the environment takes a map of variables; tests that run the binary use `Command::env`.
- tmux integration tests run on a throwaway socket, never on DuoHive's own `duohive` socket.
- Ratatui and crossterm are pinned with `=` and upgraded on purpose only.
- The user's global CLI configuration is never modified.

# DuoHive Phase 0 Implementation Plan — Foundations and spikes

**Status:** Done 2026-10-04 · every task built and reviewed (results in [bench-results.md](bench-results.md)); decisions made during execution are folded into the tasks and decision log.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the DuoHive workspace (seven crates, toolchain, licence, CI, repo hygiene) and verify the tmux and CLI mechanisms with spikes S0 (tmux activity at 100 windows), S1 (Claude Code) and S2 (OpenCode) before any adapter is written.

**Companion plan:** [phase-1-plan.md](phase-1-plan.md) (Tasks 6–30). Task numbers run across both plans. The decisions `D1`–`D32` cited here, and the Review Focus and File Structure, live in the phase 1 plan; the Global Constraints below apply to both.

**Architecture:** Seven crates with inward-only dependencies (TD §3.4). `duohive-core` holds the domain, the port traits and a sans-I/O engine. The engine is one task that owns all state, takes `Command`s and adapter `Input`s, and publishes `Event`s (TD §3.2–3.3). Adapters (tmux runtime, CLI adapters, SQLite/TOML store, hook listener) run as their own tasks and reach the core only through channels or the port traits. The TUI renders events and sends commands.

**Tech Stack:** Rust 1.99.0 (edition 2024), tokio 1.53, ratatui =0.30.2, crossterm =0.29.0, rusqlite 0.40 (bundled SQLite, WAL), toml 1.1, serde 1, serde_json 1, thiserror 2, tracing 0.1 + tracing-subscriber 0.3 + tracing-appender 0.2, shell-words 1.1, getrandom 0.4, libc 0.2, insta 1.49, tempfile 3. tmux 3.2 or newer. Versions checked on the crates.io index (2026-10-03): every pin is the latest stable release (patch updates come through `Cargo.lock` and Dependabot); the highest MSRVs are ratatui 0.30.2 (1.88), toml 1.1 and getrandom 0.4 (1.85), all below the 1.99 toolchain.

**Spec:** [docs/product/prd.md](../product/prd.md) (what), [docs/ux/ui-spec.md](../ux/ui-spec.md) (how it looks), [docs/design/technical-design.md](../design/technical-design.md) (how it is built), [docs/plans/v1-roadmap.md](v1-roadmap.md) (order). When they disagree, the PRD decides *what*, the UI/UX spec decides *how it looks*, and the TD decides *how it's built*. Section references below are "TD §4.2", "UI §2.1", "PRD Org model".

## Global Constraints

- Toolchain `1.99.0` in `rust-toolchain.toml`; `rust-version = "1.99"`; `edition = "2024"`; licence `Apache-2.0`.
- Crates: `duohive-core`, `duohive-tmux`, `duohive-cli`, `duohive-store`, `duohive-tui`, `duohive-mcp`, `duohive` (binary). Dependency rule: `duohive-core` depends on no `duohive-*` crate; `duohive-tmux`, `duohive-cli`, `duohive-store`, `duohive-tui`, `duohive-mcp` depend only on `duohive-core`; only `duohive` depends on all of them.
- `duohive-core` does no I/O: no filesystem, process, socket or terminal access (tokio `sync`/`time` only, for the driver loop).
- Ratatui and crossterm pinned with `=`; upgraded on purpose only.
- Errors: `thiserror` per crate; no `unwrap`/`expect` outside tests (clippy `unwrap_used`, `expect_used` = deny; allowed in `#[cfg(test)]` code by `clippy.toml`, and in `duohive-core`'s test helpers by `#[cfg(any(test, feature = "testkit"))] #[allow(clippy::unwrap_used, clippy::expect_used, reason = "test-only helpers")] pub mod testkit;` in `lib.rs`, since `clippy.toml`'s test allowances don't cover feature-gated modules; the `test` arm lets `cargo test -p duohive-core` compile without `--features testkit`). Errors become events shown on rows (TD §8), never only in a log.
- Logging: `tracing` to `<data_dir>/logs/`, never the terminal.
- Data access only through the `Store` port; SQLite via `rusqlite`; migrations append-only.
- Paths (D18): config dir `$XDG_CONFIG_HOME/duohive` else `~/.config/duohive`; data dir `$XDG_DATA_HOME/duohive` else `~/.local/share/duohive`; `DUOHIVE_CONFIG_DIR` / `DUOHIVE_DATA_DIR` override both (tests, bench). Files: `org.toml`, `settings.toml` (config dir); `state.db`, `run.sock`, `run/<agent>/`, `logs/` (data dir).
- tmux: DuoHive's own server `tmux -L duohive` (overridable by `DUOHIVE_TMUX_SOCKET`), one session `duohive`, one window per agent; tmux ≥ 3.2 required.
- The user's global CLI configuration is never modified; every hook, instruction and setting is passed per launch from `<data_dir>/run/<agent>/`. DuoHive passes no auto-approve flags of its own (TD §2).
- States (PRD, UI §2.1): `stopped ○`, `starting ◌`, `working ●`, `waiting ◆`, `finished ✓`, `idle ◦`, `errored ✕`; `quiet ◷` overlay on working and starting only; `~` marks a screen-read state. Rows say "waiting"; definitions say "waiting on you".
- Quiet threshold default 5 minutes, from `settings.toml` (`quiet_after_minutes`, minimum 1, D19).
- Duo goes straight to idle after a turn and is never "finished".
- Needs you = waiting, quiet and errored agents (cards arrive in phase 3), oldest first by the time each entered that state; a quiet agent is ordered by the start of its silence (D13).
- Copy: sentence case; agent names are monospace identifiers, never title-cased; "Give work" is the only term for handing over a task; colour is never the only signal.
- Tests: unit tests beside the code; `insta` snapshots for TUI frames and launch commands; tmux integration tests on a throwaway socket (`duohive-test-<random>`), never on `duohive`. No test calls `std::env::set_var` (unsafe in edition 2024, and the workspace denies `unsafe_code`): code that reads the environment takes a `HashMap<String, String>` (`Paths::from_vars`, `run_hook`), and tests that run the built binary set variables with `Command::env`.
- Test-first applies to every task except Tasks 1b, 2–5 and 30, which are infrastructure, spikes and acceptance with no code under test. They are validated instead by: Task 2, `install-tmux.sh` printing the requested `tmux -V` locally and `actionlint` passing on `ci.yml`; Tasks 3–5, a spike report whose every subsection holds measured values and names the fixture or log that proves it; Task 30, the recorded bench output and the author's working day checked against the state table.
- Supported CLIs: Claude Code and OpenCode only. Codex is deferred to after v1 (PRD What We're NOT Building); nothing in this plan builds, detects or tests it.
- Commits: conventional (`feat:`, `fix:`, `docs:`, `test:`, `chore:`, `ci:`, `refactor:`, `perf:`), never a `Co-Authored-By` line; do not push.
- `unsafe`: denied workspace-wide; the one exception is `crates/duohive/src/bench/measure.rs` (`getrusage`, `getloadavg`), each block under `#[expect(unsafe_code, reason = "…")]` (Task 28).
- Spike code is throwaway: it lives under `spikes/`, is excluded from the workspace, and nothing in `crates/` imports it. Recorded fixtures (hook payloads, screen captures) are data and go to `crates/duohive-cli/tests/fixtures/`.


---

# Phase 0 — Foundations and spikes

### Task 1: Workspace, toolchain, licence and the dependency rule

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `LICENSE`, `.gitignore`
- Create: `crates/<each of the 7 crates>/Cargo.toml` and `src/lib.rs` (`crates/duohive/src/main.rs` too)
- Test: `crates/duohive/tests/dependency_rule.rs`

**Interfaces:**
- Produces: the seven crates, empty; workspace dependency versions (Tech Stack) in `[workspace.dependencies]`; `[workspace.lints]` inherited by every crate (`[lints] workspace = true`); `duohive-core` feature `testkit`.

- [x] **Step 1: Write the failing test** `crates/duohive/tests/dependency_rule.rs`. It reads each `crates/*/Cargo.toml` with `toml` (dev-dependency) and collects `[dependencies]` keys starting with `duohive`.

```rust
#[test]
fn crates_depend_inward_only() {
    let deps = workspace_duohive_deps(); // BTreeMap<String, BTreeSet<String>>
    assert_eq!(deps["duohive-core"], set(&[]));
    for c in ["duohive-tmux", "duohive-cli", "duohive-store", "duohive-tui", "duohive-mcp"] {
        assert_eq!(deps[c], set(&["duohive-core"]), "{c}");
    }
    assert_eq!(deps["duohive"], set(&["duohive-core", "duohive-tmux", "duohive-cli",
        "duohive-store", "duohive-tui", "duohive-mcp"]));
    assert_eq!(deps.len(), 7);
}
```

- [x] **Step 2: Run it and see it fail.** `cargo test -p duohive --test dependency_rule` → FAIL (no workspace yet, or missing crates).

- [x] **Step 3: Create the workspace.**
  - First re-check the MSRVs: `cargo info <crate>@<version>` for each Tech Stack pin shows a `rust-version` ≤ 1.99 (checked when this plan was written; see Tech Stack). Lower a pin only if one fails.
  - `rust-toolchain.toml`: `channel = "1.99.0"`, `components = ["rustfmt", "clippy"]`.
  - Root `Cargo.toml`: `members = ["crates/*"]`, `exclude = ["spikes"]`, `resolver = "3"`, `[workspace.package]` with `edition = "2024"`, `rust-version = "1.99"`, `license = "Apache-2.0"`.
  - `[workspace.lints.rust] unsafe_code = "deny"`; `[workspace.lints.clippy] unwrap_used = "deny"`, `expect_used = "deny"`, `dbg_macro = "deny"`, `print_stdout = "deny"`, `print_stderr = "deny"` (the TUI and subcommands that must print use a local `#[expect(clippy::print_stdout)]` with a reason).
  - `clippy.toml`: `allow-unwrap-in-tests = true`, `allow-expect-in-tests = true`, `allow-print-in-tests = true`.
  - `rustfmt.toml`: `max_width = 100` (stable options only).
  - `LICENSE`: the Apache-2.0 text from `https://www.apache.org/licenses/LICENSE-2.0.txt`, unmodified.
  - `.gitignore`: `/target`, `spikes/**/target`, `*.profraw`.
  - Each crate's `Cargo.toml` declares exactly the `duohive-*` dependencies in the test; `duohive-core` has `[features] testkit = []`.
  - `duohive-store`, `duohive-cli`, `duohive-tui` and `duohive` add `duohive-core = { workspace = true, features = ["testkit"] }` under `[dev-dependencies]` (their tests use `example_org()`, `StubAdapter` and the fake ports); `duohive-tmux` and `duohive-mcp` don't. The dependency-rule test reads `[dependencies]` only, so these don't count.

- [x] **Step 4: Run the checks.** `cargo test --workspace --all-features` → PASS; `cargo fmt --all --check` → no output; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → no warnings.

- [x] **Step 5: Commit.** `git add -A && git commit -m "chore: cargo workspace with seven crates, toolchain and licence"`

### Task 1b: Repo hygiene — Makefile, supply-chain and spelling checks, community files

Brings the repo to the baseline of well-run Rust projects (ripgrep, ratatui, uv, helix, zellij), so every later task inherits one way to format, lint, test and contribute. Infrastructure: no code under test, validated by Step 3.

**Files:**
- Create: `Makefile`, `deny.toml`, `_typos.toml`, `.editorconfig`
- Create: `README.md` (replaces the empty staged file), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `CHANGELOG.md`, `AGENTS.md`, `CLAUDE.md`
- Create: `.github/pull_request_template.md`, `.github/ISSUE_TEMPLATE/bug_report.yml`, `.github/ISSUE_TEMPLATE/feature_request.yml`, `.github/ISSUE_TEMPLATE/config.yml`, `.github/dependabot.yml`

**Interfaces:**
- Produces: `make check` = the local gate (`fmt-check`, `lint`, `test`, `deny`, `typos`), running exactly the commands CI runs (Task 2), so a green `make check` predicts a green CI. Later tasks' "run the checks" steps may use `make check`.

- [x] **Step 1: Makefile and tool configs.**
  - `Makefile`: `.DEFAULT_GOAL := help`; every target `.PHONY` and carrying a `## description` that `make help` prints. Targets and their exact commands:
    - `fmt`: `cargo fmt --all`
    - `fmt-check`: `cargo fmt --all --check`
    - `lint`: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
    - `test`: `cargo test --workspace --all-features --locked`
    - `deny`: `cargo deny check`
    - `typos`: `typos`
    - `check`: depends on `fmt-check lint test deny typos`
    - `cov`: `cargo llvm-cov --workspace --all-features --html` (report only; the 80% gate is added once phase 1 has code)
    - `doc`: `cargo doc --workspace --no-deps --all-features`
    - `tools`: `cargo install --locked cargo-deny typos-cli cargo-llvm-cov` and `rustup component add llvm-tools-preview`
    - `clean`: `cargo clean`
    - `deny` and `typos` first test for their binary and, when it is missing, fail with `<tool> not found · run make tools`.
  - `deny.toml` (cargo-deny ≥ 0.18 schema): `[graph] all-features = true`; `[advisories]` defaults (vulnerable and unmaintained crates fail); `[licenses] allow = ["Apache-2.0", "MIT", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib"]`, `confidence-threshold = 0.9`; `[bans] multiple-versions = "warn"`, `wildcards = "deny"`; `[sources] unknown-registry = "deny"`, `unknown-git = "deny"`. Workspace crates set `publish = false` until phase 5, so their own licence is not checked as a dependency.
  - `_typos.toml`: excludes `target/`, `*.lock`, `crates/duohive-cli/tests/fixtures/`, `docs/ux/ui-spec.md` mock-ups only if they trip it (add `[default.extend-words]` entries instead where a word is intentional).
  - `.editorconfig`: `root = true`; `*`: `charset = utf-8`, `end_of_line = lf`, `insert_final_newline = true`, `trim_trailing_whitespace = true`, `indent_style = space`, `indent_size = 4`; `*.{yml,yaml,toml,md,json}`: `indent_size = 2`; `Makefile`: `indent_style = tab`; `*.md`: `trim_trailing_whitespace = false`.

- [x] **Step 2: Community and agent files.**
  - `README.md`: name and one-line pitch from the PRD vision; status badge line (CI); "Status: pre-alpha, phase 0" note; what DuoHive is (3–5 bullets from the PRD: org of standing agents, live status at a glance, Duo the manager agent, Claude Code and OpenCode, one lightweight process); requirements (tmux ≥ 3.2, Rust 1.99 to build); build and run (`cargo build --release`, `make check`); links to `docs/README.md`, `CONTRIBUTING.md`, `LICENSE`; licence line "Apache-2.0". No screenshots or install commands that don't exist yet.
  - `CONTRIBUTING.md`: dev setup (`rustup` picks the toolchain from `rust-toolchain.toml`, `make tools`), the workflow (test first, `make check` before every commit), conventional commit types (`feat`, `fix`, `docs`, `test`, `chore`, `ci`, `refactor`, `perf`), the docs' precedence (PRD → UI spec → TD), the dependency rule (TD §3.4), the Global Constraints that bite contributors (no `unwrap`/`expect` outside tests, no `unsafe`, `duohive-core` does no I/O, no `std::env::set_var` in tests, tmux tests on a throwaway socket).
  - `CODE_OF_CONDUCT.md`: Contributor Covenant 2.1, verbatim from `https://www.contributor-covenant.org/version/2/1/code_of_conduct/code_of_conduct.md`, with the contact set to `duongnv129@gmail.com`.
  - `SECURITY.md`: supported versions (latest only, pre-1.0); report privately via GitHub security advisories or `duongnv129@gmail.com`, not public issues; scope notes (per-launch tokens, hook socket, the user's CLI configuration is never modified).
  - `CHANGELOG.md`: Keep a Changelog 1.1.0 format with an `## [Unreleased]` section listing the workspace and repo setup.
  - `AGENTS.md`: guidance for coding agents working on this repo: read order (`docs/README.md`, then the plan task), `make check` is the gate, Global Constraints summary, commit rules (conventional, never a `Co-Authored-By` line, never push), the dependency rule. `CLAUDE.md` contains one line, `@AGENTS.md`, so both agents read the same file.
  - `.github/pull_request_template.md`: summary, linked plan task, test plan checklist (`make check` passes, tests added first), docs updated if behaviour changed.
  - Issue forms (YAML): `bug_report.yml` (DuoHive version, OS, tmux version, CLI and version, steps, expected, actual, `duohive` log excerpt), `feature_request.yml` (problem, proposal, alternatives), `config.yml` with `blank_issues_enabled: false`.
  - `.github/dependabot.yml`: `version: 2`; ecosystems `cargo` (directory `/`) and `github-actions` (directory `/`), both `interval: weekly`, grouped minor and patch updates. Ratatui and crossterm are ignored for automatic updates (pinned with `=`, upgraded on purpose: Global Constraints).

- [x] **Step 3: Validate.** `make help` lists every target; `make tools` (once); `make check` passes; `typos` passes over the whole repo, docs included (fix real typos in the docs; whitelist intentional words in `_typos.toml`); `actionlint` has nothing to say about `dependabot.yml` (it isn't a workflow; check it with `yq . .github/dependabot.yml` instead) and the issue forms parse with `yq`.

- [x] **Step 4: Commit.** `git add -A && git commit -m "chore: makefile, cargo-deny, typos, editorconfig and community files"`

### Task 2: CI on macOS and Linux against tmux 3.2a and latest

**Files:**
- Create: `.github/workflows/ci.yml`, `.github/scripts/install-tmux.sh`, `.github/tmux-latest`

**Interfaces:**
- Produces: `install-tmux.sh <version>` installs tmux `<version>` into `$HOME/.local/tmux-<version>` and prints `tmux -V`; later tasks' tmux integration tests run in this matrix.

- [x] **Step 1: Write the script.** `install-tmux.sh <version>`: on Linux `sudo apt-get update && sudo apt-get install -y libevent-dev libncurses-dev bison pkg-config`; on macOS `brew install libevent ncurses pkg-config bison`; download `https://github.com/tmux/tmux/releases/download/<version>/tmux-<version>.tar.gz`, `./configure --prefix=$HOME/.local/tmux-<version> && make -j && make install`, append the `bin` dir to `"${GITHUB_PATH:-/dev/null}"` (so a local run, where `GITHUB_PATH` is unset, doesn't fail). Exit non-zero if the resulting `tmux -V` doesn't print `tmux <version>`. `.github/tmux-latest` holds `3.7c`.

- [x] **Step 2: Check the script locally.** `bash .github/scripts/install-tmux.sh 3.2a && ~/.local/tmux-3.2a/bin/tmux -V` → `tmux 3.2a`.

- [x] **Step 3: Write `ci.yml`.** Triggers: `push`, `pull_request`. Job `lint` (ubuntu-24.04): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`. Job `deny` (ubuntu-24.04): `EmbarkStudios/cargo-deny-action@v2` with `command: check`. Job `typos` (ubuntu-24.04): `crate-ci/typos` pinned to a release tag. These run the same checks as `make check` (Task 1b). Job `test`: matrix `os: [ubuntu-24.04, macos-14]` × `tmux: ["3.2a", "latest"]`; resolve `latest` from `.github/tmux-latest`; cache `~/.local/tmux-*` keyed by OS and version; run the script, then `cargo test --workspace --all-features --locked` (the same flags as Task 29's local gate). Use `actions/checkout@v4`, `Swatinem/rust-cache@v2`; the toolchain comes from `rust-toolchain.toml`.

- [x] **Step 4: Validate the workflow syntax.** `actionlint .github/workflows/ci.yml` (install with `brew install actionlint`) → no output.

- [x] **Step 5: Commit.** `git add -A && git commit -m "ci: fmt, clippy and tests on macOS and Linux with tmux 3.2a and latest"`

### Task 3: Spike S0 — tmux activity for silenced panes at 100 windows

Deliverable: `docs/plans/spike-report.md` section **S0** with the headings below, plus a TD §4.1 update if the mechanism changes. Throwaway code in `spikes/s0-tmux-activity/` (a shell script and, if needed, a tiny standalone Rust or Python reader); it is never imported by `crates/`.

**Files:**
- Create: `spikes/s0-tmux-activity/README.md` (states "throwaway"), `spikes/s0-tmux-activity/run.sh`
- Create: `docs/plans/spike-report.md` (with an S0 section and empty S1 and S2 headings)
- Modify (only if the mechanism changes): `docs/design/technical-design.md` §4.1, §13

**Interfaces:**
- Produces, for Task 18–19: report section **S0.4 Formats** (the exact `%subscription-changed` line for a window subscription and a pane subscription, the `%window-close` and `%exit` lines, the client flags accepted by `refresh-client -f` on 3.2a), and **S0.5 Decision**: `subscribe` or `poll`, and the silencing method `no-output` or `pause` (D25), with the exact commands DuoHive sends.

- [x] **Step 1: Write `run.sh <tmux-binary> <windows> <seconds> <silence>`** (`<silence>` is `no-output` or `pause`). It starts a throwaway server `-L duohive-s0-$$`, sets `remain-on-exit on`, creates session `duohive` with a keeper window (`tail -f /dev/null`), then `<windows>` windows: one third noisy (`while :; do date; sleep 0.1; done`), one third bursty (10 s of output every 30 s), one third idle (`cat`). It attaches one control client `tmux -L … -C attach -t =duohive` and silences every pane: with `no-output`, `refresh-client -f no-output,ignore-size`; with `pause`, `refresh-client -f pause-after=1,ignore-size` and then `refresh-client -A '%<pane>:pause'` for every pane. It then sends `refresh-client -B 'act:@*:#{window_activity}'`, `refresh-client -B 'dead:%*:#{pane_dead} #{pane_dead_status}'`, and timestamps every line it reads into `out/<version>-<silence>-notifications.log`. It samples `ps -o %cpu=,rss=` for the tmux server and the reader every second into `out/<version>-<silence>-cpu.csv`. It also opens one dedicated window running `sh -c 'sleep 20; exit 3'` (outside the mix), whose pane dies with status 3 at t ≈ 20 s, for the `pane_dead_status` check.

- [x] **Step 2: Run on both versions with both silencing methods.** `bash .github/scripts/install-tmux.sh 3.2a`, then `./run.sh ~/.local/tmux-3.2a/bin/tmux 100 120 no-output`, `… pause`, and the same two with `$(which tmux)` (latest). Also run `poll` mode (the same windows, no subscriptions, `list-windows -F '#{window_id} #{window_activity} #{pane_dead} #{pane_dead_status}'` once a second) for the cost comparison.

- [x] **Step 3: Write the report S0** with these headings and measured values:
  - **S0.1 Setup:** machine, OS, tmux versions, window mix.
  - **S0.2 Reports for silenced panes:** for each silencing method (`no-output`, `pause`), do `act` reports keep arriving for windows whose output is not sent? Median and p95 delay from a change to its report; reports per window per second (expected ≤ 1). For `pause`, whether panes stay paused (no `%continue` without a request) and whether a new window must be paused explicitly.
  - **S0.3 Cost:** tmux server and reader CPU % and RSS at 100 windows, idle and noisy, for `subscribe` with each silencing method vs `poll`; and with no control client at all (baseline).
  - **S0.4 Formats:** the exact lines (see Interfaces). Whether `pane_dead_status` arrives for the `exit 3` window.
  - **S0.5 Decision:** `subscribe` if, on both 3.2a and latest, reports arrive for silenced windows with median delay ≤ 1.1 s and the reader's CPU at 100 noisy windows is no more than `poll`'s; else `poll`. Silencing (D25): the method under which reports keep arriving on both versions at the lower tmux server + reader CPU at 100 noisy windows; on a tie, `no-output` (one command, nothing to do per new window). Record the exact commands for the chosen method, including what a new window needs.
  - **S0.6 TD changes:** none, or the exact edit made.

- [x] **Step 4: Update the TD** only if S0.5 differs from TD §4.1 (fallback chosen, or a different flag): edit §4.1's tmux-activity row and the S0 bullet, and §13's settle-mechanism row. Name the chosen silencing method in §4.1 in place of "chosen by spike S0".

- [x] **Step 5: Commit.** `git add -A && git commit -m "docs: spike S0 tmux activity subscriptions at 100 windows"`

### Tasks 4–5: Spikes S1–S2 — Claude Code, OpenCode

One task per CLI, same procedure and the same report headings, so Tasks 20–21 can cite them. Pinned versions for this plan: **Claude Code 2.1.288**, **OpenCode 1.18.34** (the versions `claude --version` and `opencode --version` reported on 2026-10-03; if a newer one is installed at execution, use it and record which).

| Task | CLI | Report section | Throwaway dir | Fixtures dir |
|---|---|---|---|---|
| 4 | Claude Code | S1 | `spikes/s1-claude/` | `crates/duohive-cli/tests/fixtures/claude/<version>/` |
| 5 | OpenCode | S2 | `spikes/s2-opencode/` | `crates/duohive-cli/tests/fixtures/opencode/<version>/` |

**Interfaces:**
- Produces, per CLI, report subsections with these exact numbers (`Sn.k`), consumed by Tasks 20–21:
  - **Sn.1 Version:** the `--version` output and the version-string regex.
  - **Sn.2 Standing instructions:** the flag or file mechanism, verified by a sentinel ("reply with the word HIVE-SENTINEL if you can read this").
  - **Sn.3 Hooks:** a table mapping CLI hook/event → DuoHive event (`session-start`, `busy`, `needs-input`, `turn-end`, `session-end`) with any matcher; how the payload arrives (stdin, which `duohive hook` reads; any other way goes to Sn.13); the per-launch file that configures them; which of the five have no hook (this becomes the adapter's `HookCoverage`).
  - **Sn.4 Last reply:** where the last reply is (payload field, transcript path and format, session log) and the extraction rule.
  - **Sn.5 MCP:** the per-launch MCP mechanism, verified by listing a dummy stdio server's tool (for phase 3).
  - **Sn.6 Model and effort:** the flags, and how to list models and efforts at runtime, or "no listing" (for phase 4).
  - **Sn.7 Screens:** captures (`capture-pane -p -J`) of: at rest, working, an approval prompt, any start-up prompt (trust, login, update notice); the distinguishing text for each (these become `read_screen` rules, matched against the screen with trailing blank rows dropped and each line's trailing whitespace trimmed, as `capture-pane` pads both).
  - **Sn.8 Ready:** what signals the CLI is ready, with and without a first prompt; what start-up prompts can appear.
  - **Sn.9 Quit:** the keys or command that exit with status 0 and fire the end hook (if any).
  - **Sn.10 Paste:** whether `paste-buffer -p` of a multi-line task followed by `Enter` submits it as one turn; what the submit key is.
  - **Sn.11 Approvals untouched:** with DuoHive's launch command, a file write still prompts; no approval-bypass flag is passed.
  - **Sn.12 Global config untouched:** checksums of the CLI's global config files (`~/.claude/settings.json`, `~/.config/opencode/*`) before and after a full launch–turn–quit cycle are equal.
  - **Sn.13 Fallbacks and TD changes:** missing mechanisms, the fallback used (a temporary file in `run/<agent>/`, or screen reading), and the TD §5.2 edit made.

For each of Tasks 4 and 5:

- [x] **Step 1: Write the throwaway launcher** `spikes/sN-<cli>/launch.sh <agent> <task?>` that builds the per-launch files in `/tmp/duohive-spike/run/<agent>/`, starts the CLI in a window of `tmux -L duohive-spike`, and points every hook at `spikes/sN-<cli>/record-hook.sh <agent> <event>`, which writes its stdin and arguments to `fixtures/…/hooks/<event>-<n>.json`. Starting points from the TD §5.2 table: Claude Code `--append-system-prompt`, `--settings <file>` (hooks `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `Notification`, `Stop`, `SessionEnd`), `--mcp-config`, `--model`, `--effort`; OpenCode an agent definition and plugin selected via `OPENCODE_CONFIG` (and/or `OPENCODE_CONFIG_DIR`).

- [x] **Step 2: Run the scenarios** and record fixtures: start with no task; start with a task; a turn that asks for approval (ask it to create a file), answered yes; one answered no; a turn that ends normally; quit with the Sn.9 keys; a crash (`kill -9` the CLI). Save screen captures for Sn.7 into `screens/<name>.txt` and replies into `replies/`.

- [x] **Step 3: Write report section Sn** with all thirteen subsections filled in. Every table row names the fixture file that proves it.

- [x] **Step 4: Update the TD** §5.2 row for that CLI (and §6.5 if reply extraction differs) wherever a mechanism changed; add the fallback to §5.2's notes if one was needed.

- [x] **Step 5: Commit.** `git add -A && git commit -m "docs: spike SN <cli> launch, hooks and replies"`


---

## Self-review notes

- **Spec coverage:** roadmap Phase 0 (workspace, toolchain, licence, CI, S0, S1 Claude Code, S2 OpenCode) → Tasks 1–5.
- **Size:** 5 tasks (Tasks 1–5, with 1b).

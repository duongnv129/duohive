# DuoHive Phase 5 Implementation Plan — Open-source launch

**Status:** Agent tasks built 2026-10-23 (5.1, 5.2, 5.3a, 5.4, 5.5a); human tasks open (5.3b, 5.5b, 5.6–5.9) · plan verified by a third review round, both reviewers PASS, 2026-10-23 · outline written 2026-10-05, reviewed against the PRD, UI/UX spec, technical design and the repository by two independent reviewers on 2026-10-23 (both FAIL on the outline; every finding is resolved below), expanded into this task-by-task plan 2026-10-23. Depends on phase 4, which is **built with acceptance open** (see [Entry gate](#entry-gate)).

Sources: [PRD](../product/prd.md) (Delivery Phases row 5, Success Metrics, Key Hypothesis, Decisions Log: Distribution, Branding, Name) · [UI/UX spec](../ux/ui-spec.md) §1, §4.3, §4.12, §5, §6, §8 · [Technical design](../design/technical-design.md) §10–§12 · [Roadmap](v1-roadmap.md) row 5 · [Traceability](v1-traceability.md) · [Phase 4 plan](phase-4-plan.md).

**Goal:** Release DuoHive v1 through its install channels, gated on the release benchmark, then test the key hypothesis.

**Done when (roadmap):** DuoHive is public; 30-day daily use and < 10 minutes to a first team.

## Who does what

Every task is marked **A** (an agent can do it inside the repository: code, config, docs, local dry-runs), **H** (human only) or **C** (calendar time).

**Rule for agents, from the author's global rules:** never push a tag, never publish to crates.io or any tap, never register a name or domain, never post an announcement. Those are **H** steps, listed in [Human steps](#human-steps-the-author-runs-in-this-order). A pull request is opened by the author or at their explicit request.

## Entry gate

Phase 4 is *built*; these items are open (phase-4-plan "Open items carried to the end of the phase"): the gate run under the TD §10 method (never completed: the development machine's load stayed above 2), tuning if it misses, the one-hour run, the iTerm2 walk, the author's approvals, the sign-off. Consequences for this phase:

- Tasks 5.1, 5.2 (code), 5.3a, 5.4 and 5.5a are **A** and start now: none depends on a passing gate.
- **No tag, no publish (5.3b, 5.8) before** phase 4 reads `Status: Done` and a gate run recorded under the method (5.2's attestation, below) shows every target met. The binary-size headroom is thin (9,829,520 B before phase 4's profile settings; 6,715,312 B measured 2026-10-22 against the 10,000,000 B limit), and `~` freshness at 10 agents was 1983.65 ms against 2000 ms: a miss goes back to phase 4 Task 4.2, never to lowering a target (PRD change required).

## Constraints carried from earlier phases

- Every Global Constraint of [phase 0](phase-0-plan.md#global-constraints) and [phase 2](phase-2-plan.md) still applies, as do decisions D1–D32, P2-D1–P2-D28, P3-D1–P3-D31, P4-D1–P4-D4.
- Release targets are gated on Apple M-series. Linux numbers are reported but never block a release (PRD Success Metrics, TD §10).
- Licence Apache-2.0; the repo stays the author's own (PRD Decisions Log: Distribution). Positioning leads with "Duo + your Engineer Factory", not "Hive" (Branding).
- Toolchain 1.99.0 (`rust-toolchain.toml`), MSRV 1.99 (`rust-version`, TD §11 "MSRV declared"), edition 2024.
- No `unwrap`/`expect` outside tests; no `unsafe` outside `bench/measure.rs`; `duohive-core` does no I/O; the dependency rule of TD §3.4.
- tmux 3.2 or newer (install.sh `tmux_min`; CI tests 3.2a and latest). Windows is out (tmux).
- No telemetry (PRD non-goal): the README says so.
- The gate's exit codes stay as phase 4 set them: 0 every target met, 1 a target missed (or any other failure), 3 discarded for load, 130 interrupted. One deliberate exception (TD §10: Linux numbers are reported, never block): on a `linux-*` platform a miss is printed with `reported only` and the exit is 0.

## Author decisions (defaults, for the author to confirm)

| # | Decision | Default and why |
|---|---|---|
| P5-D1 | Where the release gate runs | **On the author's own M-series machine, plugged in, load at most 2**, by `duohive bench --gate --json`; the result is committed as `docs/release-gates/<version>.json` and the release workflow only *verifies* it (version equals the tag, `all_pass` true, platform `macos-aarch64`, binary size under the limit). Hosted runners are shared VMs that cannot meet the method (`ci.yml` already says so). A self-hosted runner is the alternative; it is not needed for v1 |
| P5-D2 | One gate implementation | `duohive bench --gate` in the binary is the single implementation; `scripts/bench-gate.sh` and `make bench-gate` become thin wrappers. The tests of the script's behaviour move to the Rust gate |
| P5-D3 | Binary size gate | The unpacked **macOS arm64** binary produced by the release build (`dist` profile, which inherits `release`) must be under 10,000,000 B; the release workflow checks it. Other targets report their size |
| P5-D4 | Release targets (TD §11 names none; this is the proposal) | macOS arm64 (gated), macOS x86_64 and Linux x86_64 (built, `--version` smoke-tested). **Linux aarch64 is not shipped in v1**: CI has no runner or tests for it, and `rusqlite` is built `bundled` (needs a C toolchain). Add it in v1.1 if asked |
| P5-D5 | Channels | GitHub Release binaries (cargo-dist), a Homebrew tap, crates.io. `install.sh` stays a source installer (it needs a Rust toolchain; it installs the `dh` alias). Only the source installer creates `dh`; the README says so per channel |
| P5-D6 | Crates | Publish all seven at one version (P5-Q2 default): `cargo install duohive` needs the six libraries on crates.io. Order: `duohive-core`, then `duohive-{cli,store,tmux,tui,mcp}`, then `duohive`. `cargo publish --dry-run` can pass only for `duohive-core` before its dependents' dependencies exist: the dry-run of the others is a `cargo package --list` check, and the real publish is the proof |
| P5-D7 | Versioning | The workspace version moves `0.1.0` → `1.0.0-rc.1` → `1.0.0` (one edit of `[workspace.package]` and the six `version = "…"` pins in `[workspace.dependencies]`; `scripts/set-version.sh` does both and a test checks they agree). The tag is `v<version>`; the release workflow refuses a tag that differs from the Cargo version |
| P5-D8 | `publish = false` | Stays `false` in the repository until the author flips it in the release commit (5.3b). Until then a stray `cargo publish` cannot succeed |
| P5-D9 | macOS Gatekeeper | v1 binaries are **not notarised** (no paid Apple developer account assumed). The README gives the one-line quarantine removal for tarball installs, and says that `brew` and `cargo install` are not affected. Signing is a v1.1 item if the author has a certificate |
| P5-D10 | Rollback | A bad release is answered by a patch release (`1.0.1`), `cargo yank` of the bad version, deleting the GitHub Release and reverting the tap formula commit. Written as a runbook in `docs/release-runbook.md` (5.3a) |
| P5-D11 | The 30-day clock | Starts the day `v1.0.0` is tagged; earlier days count if the log shows DuoHive was already the only tool (P5-Q3 default) |

## Spec coverage

| Requirement | Source | Task |
|---|---|---|
| Pre-launch UX check of the phase 3–4 screens, the six principles, three sizes, three themes and `NO_COLOR` | PRD UX checks; UI §1, §5, §6, §8 item 2 | 5.1 |
| The no-tmux / tmux-too-old screen and the no-CLI screen | UI §4.12, §4.3 | 5.1 |
| Release gate: the full benchmark on every release; macOS arm64 must meet every target; Linux reported | TD §10 Release gate, §11 | 5.2 |
| `--gate` does the counted part inside the binary | TD §10 (method paragraph) | 5.2 |
| Binary < 10 MB (10,000,000 B) | PRD Success Metrics; TD §10 | 5.2 |
| `cargo-dist` release, Homebrew tap, the `duohive` crate | Roadmap row 5; TD §11 | 5.3a, 5.3b |
| MSRV declared; tmux minimum | TD §11; install.sh | 5.3a, 5.4 |
| CI release gate | PRD Delivery Phases row 5 | 5.2, 5.3a |
| README finished for launch; CONTRIBUTING, SECURITY, CHANGELOG; "docs" | Roadmap row 5; PRD row 5; TD §12 row 5 | 5.4 |
| PRD user flow "install to a working team" step 1 (needs tmux and one agent CLI) | PRD User flow | 5.4 |
| Name checks: USPTO and WIPO, classes 9 and 42; `duohive.dev` or `duohive.ai`; GitHub org; the `duohive` crate | PRD Delivery Phases row 5; Decisions Log: Name | 5.5a, 5.5b |
| Time to first team < 10 min, README only, 3 first-time users | PRD Success Metrics; Key Hypothesis | 5.6 |
| 30 consecutive days of daily use | PRD Success Metrics (primary); Key Hypothesis | 5.7 |
| Interviews with 3–5 leads (worth doing, not blocking) | PRD Evidence; Open Questions | 5.7 (optional) |
| Launch and close | Roadmap row 5; traceability | 5.8, 5.9 |

## File structure

| Crate or path | Create | Modify |
|---|---|---|
| `duohive` | `src/bench/gate.rs` (pure verdict and the runner), `tests/gate.rs` | `src/args.rs` (`bench --gate`), `src/main.rs` (exit code), `src/bench/mod.rs` (`pub mod gate`), `tests/bench_gate_script.rs` (becomes the wrapper test) |
| `scripts/` | `set-version.sh`, `check-release-version.sh` | `bench-gate.sh` (thin wrapper) |
| `.github/workflows/` | `release.yml` | `ci.yml` (the bench job stays informational; add the `actionlint` step and a `package` job) |
| repo root | `dist-workspace.toml`, `docs/release-runbook.md`, `docs/release-gates/.gitkeep` | `Cargo.toml` (`[profile.dist]`, no version change until 5.3b), each crate's `Cargo.toml` (description, repository, readme, keywords, categories), `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md` |
| `duohive-tui` | a prelaunch matrix test module | screen fixes only where the check finds a failure |
| `docs/plans/` | `ux-check-prelaunch.md`, `name-checks.md`, `first-team-trials.md`, `daily-use-log.md` | `v1-roadmap.md`, `v1-traceability.md`, `bench-results.md` |

## Tasks

### Task 5.1: Pre-launch UX check (A, with H screenshots)

**Files:** `docs/plans/ux-check-prelaunch.md`; a matrix test in `duohive-tui`; fixes in `crates/duohive-tui` only where it finds a failure.

- [x] **Step 1: Write the failing matrix test** `prelaunch_screens_fit_every_size`: every phase 3–4 screen (Duo chat and cards, the suggestion popup, task to a role, the first-run wizard's three steps and the no-CLI screen, the tmux-missing and tmux-too-old screens (UI §4.12), `Replace your org?`, Settings with the Keys and Org rows, the key-binding editor, the `?` key map, the filter chip, Move to…, the right-click menu, the model and effort dropdowns) is drawn at 80×24, 100×30 and 160×48 into a `TestBackend`, in each of `auto`, `dark` and `light`, with and without `NO_COLOR`; assert it does not panic, no line is wider than the area, and every focusable control's label is on screen (reuse the a11y helpers of 4.9: contrast ratios from UI §6, state glyphs and text under `NO_COLOR`).
- [x] **Step 2: Run, see FAIL** for any screen the test cannot reach or that overflows; each failure is a row in the check file and a fix.
- [x] **Step 3: Fix** what the test finds, each with its updated `insta` snapshot reviewed against the UI spec.
- [x] **Step 4: Fill `ux-check-prelaunch.md`:** every screen × each of the six PRD UX principles as `pass`, `fixed (commit)` or `needs a human look`, with the test names that back each `pass`. The principles that a test cannot judge stay `needs a human look` for the author's walk, which is phase 4's Task 4.10 step 3 done once for both.
- [x] **Step 5: Run, see PASS.** `make check`. Commit `test: pre-launch screen matrix` (and one `fix:` commit per screen fix).

### Task 5.2: Release gate in the binary (A)

**Files:** create `crates/duohive/src/bench/gate.rs`, `crates/duohive/tests/gate.rs`; modify `args.rs`, `main.rs`, `bench/mod.rs`, `scripts/bench-gate.sh`, `tests/bench_gate_script.rs`, TD §10.

**Interfaces:**
- Produces: `duohive bench --gate [--json]`, valid alone (it implies 10 then 100 agents and the TD §10 method; combining it with `--agents`, `--idle-secs`, `--turns` or `--view` is a usage error `--gate takes no other bench option`). `pub fn worst_of(reports: &[BenchReport]) -> BenchReport` (largest value per target; `first_frame_ms` the maximum, `None` if any run lacks it), `pub fn gate_passes(&GateReport) -> bool`, `GateReport::to_json() -> Value` (`all_pass`, `version`, `platform`, `binary_bytes`, per-tier worst values and the three runs), and `run_gate(runner: &mut dyn FnMut(u32, bool) -> RunResult) -> Result<GateReport, GateError>` where `RunResult` is `Report(BenchReport)`, `Discarded`, `Interrupted` (a child that exits 130, or Ctrl-C on the gate itself) or `Failed(String)`. The production runner executes `current_exe() bench --agents N [--view] --json` as a child, so every run is a fresh process (the bench installs its own signal handlers and temp state).
- Exit codes: 0, 1, 3 and 130 as in Constraints. A tier whose counted run is discarded three times in a row ends the gate with 3 (a retry is at most two).
- `BenchReport::from_json(&Value) -> Result<BenchReport, String>` (in `bench/report.rs`, the inverse of `to_json`; a `"discarded"` payload is not a report and is handled by the runner from the exit code 3 before parsing). `GateReport` also records `git_sha` (from `git rev-parse HEAD`, `None` outside a checkout) and `profile` (`release`).
- Consumes: `Targets::for_agents`, `BenchReport`, `BenchError::LoadTooHigh`.

- [x] **Step 1: Write the failing tests** (inline for the pure parts, `tests/gate.rs` for the process): `worst_of_takes_the_largest_value_per_target`; `gate_fails_on_each_missed_target` (five cases: CPU, memory, hook, `~`, first frame); `gate_passes_at_target`; `gate_fails_when_first_frame_missing`; `counted_runs_use_the_view` (the runner is called with `view = true` three times per tier, then once per tier with `false`, informational only and never judged); `discarded_run_is_retried_twice` and `three_discards_end_with_exit_three`; `other_failure_ends_with_exit_one_and_its_stderr`; `interrupted_run_ends_with_exit_130`; `linux_platform_reports_but_never_gates` (a `linux-x86_64` miss is printed with `reported only` and the exit is 0); `from_json_round_trips_to_json`, `from_json_accepts_a_null_first_frame`, `from_json_names_a_missing_field`; `gate_json_has_version_platform_git_sha_and_binary_bytes`; `gate_rejects_other_bench_options` in `args.rs`; `bench_gate_script_delegates` (the script runs `$DUOHIVE_BIN bench --gate "$@"` and returns its exit code, tested with a stub binary).
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive --lib -- gate` and `cargo test -p duohive --test gate --test bench_gate_script`.
- [x] **Step 3: Implement** `gate.rs` (the pure parts first, then the child-process runner reading the same `--json` the script read), the `--gate` argument, the exit-code mapping in `main.rs`, and the thin `bench-gate.sh`. The old script tests' behaviours (worst run decides, retry twice, exit 3) are covered by the `gate.rs` tests above; delete none of them without its replacement.
- [x] **Step 4: Release check script** (its own commit, `ci: release check script`). `scripts/check-release-version.sh <tag>` exits 1 unless `<tag>` is `v` plus the workspace version and `docs/release-gates/<version>.json` exists with `"all_pass": true`, `"platform": "macos-aarch64"` and `binary_bytes` under 10,000,000. Red first, in `tests/release_check.rs` against fixtures in a temp directory: `release_check_passes_a_good_gate_file`, `release_check_refuses_a_tag_that_differs_from_the_version`, `release_check_refuses_a_missing_gate_file`, `release_check_refuses_a_failed_gate`, `release_check_refuses_a_non_arm64_gate`, `release_check_refuses_an_oversize_binary`, `release_check_refuses_a_gate_sha_that_differs_from_the_crates_tree` (the gate file's `git_sha` must equal the tag commit, or differ only by commits that touch nothing outside `docs/`; this is how the attestation is tied to the shipped code). The script parses JSON with `sed`/`grep` like `bench-gate.sh` did, with no `jq`. `release.yml` (5.3a) calls it.
- [x] **Step 5: Docs.** TD §10: the method paragraph says `--gate` does the counted part inside the binary (now true; record P5-D1), and the size definition is made exact: *the unpacked `duohive` binary of the release build; `[profile.dist]` inherits `release` with no overrides, so the `dist` binary is the same size, and the release workflow asserts it is under the limit too*. `GateReport.binary_bytes` is the size of the gate's own executable, so the gate must be run from `make release`'s binary. Also the `--help` text in `USAGE`.
- [x] **Step 6: Run, see PASS.** `make check`. Commit `feat: duohive bench --gate` (the gate, its wrapper script and the TD edit; Step 4 is the second commit).

### Task 5.3a: Packaging, release workflow and runbook (A, no publishing)

**Files:** `dist-workspace.toml`, `.github/workflows/release.yml`, `.github/workflows/ci.yml`, workspace and crate `Cargo.toml` metadata, `scripts/set-version.sh` (created here; `scripts/check-release-version.sh` is created in 5.2), `docs/release-gates/.gitkeep`, `docs/release-runbook.md`, `CHANGELOG.md`.

- [x] **Step 1: Crate metadata.** Each crate gets `description`, `repository = "https://github.com/duongnv129/duohive"`, `readme`, `keywords`, `categories`; `license` and `rust-version` are inherited. `publish = false` stays (P5-D8). Add `[profile.dist]` inheriting `release` and compare `cargo build --profile dist` with the release binary in bytes in the commit message.
- [x] **Step 2: `set-version.sh <version>`** edits `[workspace.package].version` and the six workspace-dependency pins together. Red first: `cargo test -p duohive --test set_version` with `set_version_changes_every_pin` and `set_version_refuses_a_non_semver_argument` against a fixture copy of `Cargo.toml` in a temp directory, then the script.
- [x] **Step 3: `dist-workspace.toml`:** installers `shell` and `homebrew`, targets per P5-D4 (`aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`), archives with `LICENSE` and `README.md`, checksums on, GitHub attestations on, the tap named by a placeholder the author fills (P5-Q5), the formula depending on `tmux`. Verify with `dist plan` if `cargo-dist` is installed on the machine; if it is not, say so in the commit and keep the file for the author's first `dist plan` (this plan does not install it).
- [x] **Step 4: `release.yml`.** The workflow is hand-written and `dist-workspace.toml` sets `allow-dirty = ["ci"]`, so `dist` never regenerates or rejects it; `dist` is used for `dist plan`, `dist build` and the installer artifacts (the Homebrew formula is built by `dist` and pushed by our `publish-tap` job). Triggered by a `v*` tag only; permissions `contents: write`, `id-token: write` and `attestations: write` for the jobs that need them and nothing else; jobs in order: `verify` (checkout with `fetch-depth: 0`, which the sha check of `check-release-version.sh` needs, `scripts/check-release-version.sh`, `fmt-check`, `lint`, `test`, `deny`, `typos`), `build` (dist, per target, with the size check of P5-D3 and a `duohive --version` smoke test per target on a matching runner), `publish-crates` and `publish-tap` guarded by `environment: release` (which the author configures with a required reviewer and the secrets `CARGO_REGISTRY_TOKEN` and `TAP_TOKEN`), `github-release` last. `publish-crates` publishes in the P5-D6 order with a wait for the index between crates. Nothing in the file runs on a branch or a pull request.
- [x] **Step 5: `ci.yml`:** add an `actionlint` step (pinned version) over `.github/workflows`, and a `package` job: `cargo package --list -p <crate>` for every crate (`cargo package` works while `publish = false`; `cargo publish --dry-run` does not, so it is not used until 5.3b).
- [x] **Step 6: `docs/release-runbook.md`:** the Human steps below as a checklist, the rollback of P5-D10, the secrets and environment the workflow needs, and how to re-run a failed publish job (a re-run skips versions already published).
- [x] **Step 7: Run.** `actionlint` if installed (else the commit says it was not run), `make check`, `cargo package --list` for each crate, and `cargo build --profile dist` size equal to the release binary's. Commit `ci: release workflow and packaging`. The workflow has no unit test: its guards are the tag-only trigger, the `release` environment and `check-release-version.sh`, whose tests are in 5.2.

### Task 5.3b: Release candidate (H)

The author, after the [Entry gate](#entry-gate) and 5.5b: runs `scripts/set-version.sh 1.0.0-rc.1`, sets `publish` to `true` in `[workspace.package]`, runs the gate on the release machine and commits `docs/release-gates/1.0.0-rc.1.json`, opens the release PR, tags `v1.0.0-rc.1` on the merged commit, and approves the `release` environment. The rc goes to crates.io as a pre-release version, a GitHub pre-release and the tap. Checks: `brew install --build-from-source` on a clean macOS user, `cargo install duohive --version 1.0.0-rc.1`, and `duohive --version` equal to the tag.

### Task 5.4: README, CONTRIBUTING, SECURITY, CHANGELOG (A)

**Files:** `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, `docs/README.md`, TD §12 row 5.

- [x] **Step 1: README** covers, in this order: what DuoHive is in two sentences (leads with "Duo + your Engineer Factory"); requirements (tmux 3.2 or newer, at least one of Claude Code or OpenCode, macOS or Linux, no Windows, no telemetry); install per channel (Homebrew, `cargo install duohive`, the release tarball with the quarantine line of P5-D9, `install.sh` from source and its `dh` alias, with a note that only `install.sh` creates `dh`); the first team in under ten minutes (one template, then Give work, then Needs you); Duo; a screenshot or recording slot marked `TODO(author)` because an agent cannot capture the real terminal; the key map pointer (`?`); links to the docs. Channels not yet live are marked "available from v1.0.0" so the text is true at every commit.
- [x] **Step 2: CONTRIBUTING** states the gate (`make check`), conventional commits, the toolchain, and where the specs live; **SECURITY** names the reporting address (placeholder `TODO(author)` if none is set) and the supported versions; **CHANGELOG** gets an `[Unreleased]` entry drafted from the phase plans' user-visible items, dated at release.
- [x] **Step 3: `typos` and a link check.** `typos` passes; every relative link in the touched files resolves. The README's commands are run as written on a clean machine during 5.6, not here.
- [x] **Step 4: Docs status** (`docs/README.md`, TD §12 row 5 "docs" becomes the list above). Commit `docs: launch README, changelog and policy files`.

### Task 5.5a: Name checks, the part an agent can do (A)

**Files:** `docs/plans/name-checks.md`.

- [x] **Step 1: Read-only availability lookups,** each recorded with its date and the command or URL: the `duohive` and `duohive-*` names on crates.io (the public API, no login), the GitHub user or organisation `duohive`, DNS and RDAP for `duohive.dev` and `duohive.ai`. No registration, no claim, no purchase.
- [x] **Step 2: Write the results and the open items** (USPTO and WIPO searches in classes 9 and 42 need a person's judgement and are 5.5b).
- [x] **Step 3: Commit** `docs: name check lookups`.

### Task 5.5b: Name checks, registration and legal (H)

USPTO and WIPO searches in classes 9 and 42; register `duohive.dev` or `duohive.ai`; claim the GitHub organisation and the crate names. The launch waits on a clear result; a conflict is decided with the author before 5.3b publishes anything (PRD Branding allows a rename at launch).

### Task 5.6: Time to first team (H)

Three first-time users install the release candidate from the README only and get three working agents; the author times each and records where they stalled in `docs/plans/first-team-trials.md`. All three under 10 minutes, or the blocking item is fixed and the trial repeated. The README's commands run as written on a clean machine here.

### Task 5.7: 30-day daily use (C)

`docs/plans/daily-use-log.md`: one line a day (was DuoHive the only agent tool; any other tool and why). Optionally interview 3–5 leads who run five or more agents. Thirty consecutive "only tool" days; a break restarts the count and its reason becomes a fix task.

### Task 5.8: Launch (H)

After 5.1–5.6 pass: the author runs `scripts/set-version.sh 1.0.0`, commits the gate result for that version, tags `v1.0.0`, approves the `release` environment; the release workflow publishes every channel. `brew install`, `cargo install duohive` and the release binary each run `duohive --version`. The announcement is the author's. Roadmap row 5 becomes "released · 30-day test running".

### Task 5.9: Hypothesis tested (H, after 30 days)

Mark roadmap row 5 and the PRD Delivery Phases row done; flip the PRD status from Draft; make TD §12 match; update `docs/README.md`, the traceability matrix and this plan's header to `Status: Done`. Both 5.6 and 5.7 must be recorded as passed.

## Human steps, the author runs in this order

1. Finish phase 4's open items and its sign-off (the Entry gate). The repository is public (P5-Q1 answered); `main` is protected: pull requests only, one owner approval with an admin bypass, squash merge, and seven required checks (`lint`, `deny`, `typos` and the four `test` jobs). Tags `v*` are creatable only by admins.
2. Decide P5-D1–P5-D11 and P5-Q1–P5-Q6 (or accept the defaults).
3. Task 5.5b: legal searches; register the domain; claim the organisation and the crate names.
4. Create the tap repository and the `release` environment; add the secrets `CARGO_REGISTRY_TOKEN` and `TAP_TOKEN`; set a required reviewer.
5. Run the release gate on the release machine; commit `docs/release-gates/<version>.json`.
6. Task 5.3b (rc), then 5.6; then 5.8 (tag), then 5.7 and 5.9.

## Order

5.1, 5.2, 5.4 and 5.5a are independent. 5.3a needs 5.2 (the workflow calls `check-release-version.sh`). 5.3b needs the Entry gate, 5.3a and 5.5b. 5.6 needs 5.3b and 5.4. 5.8 needs 5.1–5.6. 5.7 starts at the tag; 5.9 closes the phase.

## TODO checklist

**Before 5.1**
- [ ] The author confirms the defaults in "Author decisions" (P5-D1–P5-D11)
- [ ] Phase 4 acceptance is done and signed off (Entry gate; blocks 5.3b and 5.8 only)

**Tasks**
- [x] 5.1 Pre-launch UX check (A) — matrix test; the author's walk is listed in `ux-check-prelaunch.md`
- [x] 5.2 Release gate in the binary (A)
- [x] 5.3a Packaging, release workflow and runbook (A) — `dist` and `actionlint` not run here; first `dist plan` is the author's. **`release.yml` and the new `ci.yml` jobs (`workflows`, `package`) are not in this change**: the push token lacked the `workflow` scope, so they follow in a separate pull request from the branch `feat/phase-5-gate`
- [ ] 5.3b Release candidate (H)
- [x] 5.4 README, CONTRIBUTING, SECURITY, CHANGELOG (A) — screenshot slot left for the author
- [x] 5.5a Name checks: lookups (A) — found the repository was private (P5-Q1, since answered: public)
- [ ] 5.5b Name checks: registration and legal (H)
- [ ] 5.6 Time to first team (H)
- [ ] 5.7 30-day daily use (C)
- [ ] 5.8 Launch (H)
- [ ] 5.9 Hypothesis tested (H)

## Risks

| Risk | Response |
|---|---|
| The release gate fails at 100 agents on Apple M-series | Phase 4 tuned this; the gate shows which target failed. Fix before tagging (phase 4 Task 4.2); never lower a target without a PRD change |
| The size is over 10,000,000 B in the `dist` profile | P5-D3 checks the unpacked arm64 binary in the workflow; fall back to `opt-level = "s"` as phase 4's Step 5 allows, and record it |
| A name check finds a conflict | Decide with the author before 5.3b publishes (PRD Branding: Duocrew and DuoHQ were considered). A rename invalidates 5.3a's metadata and 5.4's README, which is why 5.5b comes first |
| Publishing seven crates for `cargo install duohive` | P5-D6 order; a partial publish is finished by re-running the job; a bad version is yanked (runbook) |
| A first-time user stalls on tmux or CLI install | UI §4.12 and §4.3 first-run screens give the steps; the README covers both; 5.1 checks the screens |
| Unsigned binaries are blocked by Gatekeeper | P5-D9: README line; `brew` and `cargo install` unaffected |
| `cargo-dist` is not installed on the development machine | 5.3a keeps the config and says it was not run; the author's first `dist plan` is in the runbook |
| Hosted CI cannot run the gate | P5-D1: the gate runs on the release machine and CI verifies the committed result |

## Open questions (decide when the phase starts)

| # | Question | Default if not decided |
|---|---|---|
| P5-Q1 | Answered 2026-10-23: the repository **is public now** (it was private when the plan was written). "DuoHive is public" still needs the v1.0.0 release on every channel and an announcement | Tag, channels and an announcement post (the author's) |
| P5-Q2 | Publish all seven crates, or ship only the binary crate with the others vendored? | Publish all seven at the same version (P5-D6) |
| P5-Q3 | When does the 30-day clock start? | The day `v1.0.0` is tagged; earlier days count if the log shows it (P5-D11) |
| P5-Q4 | Release targets | P5-D4: macOS arm64 and x86_64, Linux x86_64; Linux aarch64 is v1.1 |
| P5-Q5 | The tap repository's name and owner | `<owner>/homebrew-tap`, with the formula `duohive` |
| P5-Q6 | Does the author have an Apple developer certificate for signing and notarising? | No: unsigned in v1 (P5-D9) |

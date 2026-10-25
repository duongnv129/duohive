# Status board

**As of 2026-10-25.** One page: every phase, what is done, what CI/CD verifies automatically, and what remains. The plans hold the detail; this file is the index. Keep it in step with the phase plans and the [roadmap](v1-roadmap.md).

**Legend:** **Done** merged and verified · **Done (CI)** covered by an automated CI/CD job (the evidence is its run) · **Done (author)** done by the author and confirmed · **Open: author** manual, needs the author · **Open: agent** an agent can do it once asked or decided · **Later** after v1.

## Open tasks for the author (manual)

These need a person: a decision, an account, real users or time. Nothing in CI/CD can do or check them.

1. **Approve and decide (Phase 4):** the defaults, the starter prompts (P4-Q1) and the bench profile (P4-Q2); OpenCode's effort list on a build that lists variants (P4-Q7); whether the 1–3 % headroom on `~` freshness at 10 agents is acceptable (P4-Q8); and the 10-agent first frame, which is 145–168 ms on the CI runner against 100 ms: accept the runner as the reference or re-measure on the M-series machine (P4-Q9).
2. **Decide the Phase 5 defaults:** P5-D1 to P5-D11 and P5-Q1 to P5-Q6.
3. **Release infrastructure:** the Homebrew tap repository, the secrets `CARGO_REGISTRY_TOKEN` and `TAP_TOKEN`, and the `release` environment with a reviewer.
4. **Launch:** the go-ahead for a release candidate and v1.0.0 (5.3b), three first-time users installing from the README against the clock (5.6), a README screenshot or recording, the announcement (5.8), and the 30 days of daily use (5.7).
5. **Small decisions:** keep or remove `dist-workspace.toml` (no workflow uses it); how often the benchmark job runs (every push to `main` now); whether the new CI jobs below become required checks once they are green.

## Open tasks for an agent (not manual)

1. Phase 4 sign-off docs (4.10-4): after the author's item 1.
2. Run the `publish` workflow on the first release candidate once the author's item 3 is done (the crate names are claimed by the first publish; the name-check task 5.5b was removed, P5-D12). The workflow is written; it has not run.
3. Make the new CI jobs required checks: after they run green on `main`.
4. Phase 5 close-out docs (5.9): after the 30 days.
5. Watch `plugin_replays_recorded_events` (the OpenCode plugin replay, about 40 s): it failed once under load locally and has not failed in CI.

## Verified automatically by CI/CD

| Job | What it verifies | Status |
|---|---|---|
| `lint`, `deny`, `typos` | Formatting and clippy, dependency licences and advisories, spelling | **Done (CI)**, required |
| `test` ×4 (Linux and macOS, tmux 3.2a and latest) | The whole test suite, including the pre-launch screen matrix and the scripts' own tests | **Done (CI)**, required |
| `bench` | A 10-agent smoke run | **Done (CI)** |
| `bench-gate` | The full gate at 10 and 100 agents; the table is in the job summary | **Done (CI)**; runs on pushes to `main` and by hand |
| `release-binaries` | On a `v*` tag: builds macOS (Apple silicon, Intel) and Linux (x86_64, ARM64); the version equals the tag; each binary is under 10,000,000 B; checksums and build attestations; attaches them to the release | **Done (CI)**: `v0.1.0-alpha.1` by hand, `v0.1.0-alpha.2` by tag push |
| `docs` | Relative links in the core docs resolve, `CHANGELOG.md` has an entry for the version, the README lists every release target | **Added**, awaiting its first green run |
| `workflows (actionlint)` | Every workflow file is valid | **Added**, awaiting its first green run |
| `package (every crate packs)` | Each crate lists its package files; the core builds from its own tarball | **Added**, awaiting its first green run |
| `install from source` (Linux, macOS) | The README's source install on a clean runner: install, `duohive` and `dh` run and agree, uninstall removes only what it installed | **Added**, awaiting its first green run |
| `verify-release` | After a release is built, on each of the four platforms: download the archive, check the checksum and the attestation, the version equals the tag, `--help` runs, and a short real run with tmux works | **Added**, awaiting its first run; it is the first test of the Linux and Intel Mac binaries on their own platforms |
| `security` (`advisories`, `secret scan`, `CodeQL`, `dependency review`) | Weekly and on every PR and push to `main`: new advisories on our dependencies, secrets in the full history (gitleaks), CodeQL for Rust, risky dependency changes in a PR | **Added**, awaiting its first run |
| `publish` (crates.io, Homebrew tap) | After `Release binaries` for a release candidate or stable tag: publishes the seven crates in order and updates the tap. Skips itself while `CARGO_REGISTRY_TOKEN`, `TAP_TOKEN` or `publish = true` is missing; alpha and beta tags are never published | **Added**, awaiting the author's setup (item 3 above) and the first rc |

## Phases

| Phase | Status | Evidence |
|---|---|---|
| 0. Foundations and spikes | **Done** 2026-10-04 | [phase-0-plan](phase-0-plan.md) |
| 1. Org and agents | **Done** 2026-10-04 | [phase-1-plan](phase-1-plan.md) |
| 2. One screen | **Done** 2026-10-19 | [phase-2-plan](phase-2-plan.md) |
| 3. Duo and messaging | **Done** 2026-10-18 | [phase-3-plan](phase-3-plan.md), [bench results](bench-results.md#phase-3-acceptance) |
| 4. Scale and polish | **Built; benchmark Done (CI); the one-hour run and screen walk Done (author)** 2026-10-25; approvals and sign-off open | [phase-4-plan](phase-4-plan.md), [bench results](bench-results.md#phase-4-acceptance) |
| 5. Open-source launch | **Agent tasks built; alpha releases Done (CI)**; launch steps open | [phase-5-plan](phase-5-plan.md), [releases](https://github.com/duongnv129/duohive/releases) |
| v1.1 | **Later**: starts after v1 | [v1.1-roadmap](v1.1-roadmap.md) |

Phases 0 to 3 have no open task boxes.

## Phase 4: scale and polish

| Item | Status | Notes |
|---|---|---|
| 4.1 Full benchmark (`bench`, `--view`, gate script) | **Done** | |
| 4.3 First-run setup and templates | **Done** | |
| 4.4 New org from template | **Done** | |
| 4.5 Model and effort picker | **Done** | OpenCode effort is typed text on 2.1.1 (no `--verbose`); see P4-Q7 |
| 4.6 Key map and binding editor | **Done** | |
| 4.7 Mouse parity, plus drag-to-select and copy | **Done** | |
| 4.8 Move to… | **Done** | |
| 4.9 Filter and accessibility re-check | **Done** | |
| Expand the outline into the full plan, reviewed | **Done** | |
| 4.2 Tuning at 100 agents | **Done (CI)** | No 100-agent target missed in the CI gate; no tuning was needed there |
| 4.10-1 Gate run at 10 and 100 agents | **Done (CI)** | [Run 40161967618](https://github.com/duongnv129/duohive/actions/runs/40161967618): 100 agents pass every target in all three runs; 10 agents pass all but first frame (167.7 ms against 100 ms) on a shared 3-CPU runner with the load check off |
| 4.10-2 One-hour run with real Claude Code and OpenCode agents | **Done (author)** 2026-10-25 | Confirmed by the author |
| 4.10-3 Walk the screens in iTerm2 at 80×24 and 160×48 | **Done (author)** 2026-10-25 | Confirmed by the author; the automated part is in [ux-check-prelaunch](ux-check-prelaunch.md) |
| 4.10-4 Sign-off: TD §12, roadmap, PRD, READMEs, plan header to `Done` | **Open: agent** | After the author's approvals and decisions (P4-Q1, P4-Q2, P4-Q7 to P4-Q9) |

## Phase 5: open-source launch

| Item | Status | Notes |
|---|---|---|
| 5.1 Pre-launch UX check (screen matrix test, check file) | **Done (CI)** | Runs in the test jobs; the human-only judgements are listed in [ux-check-prelaunch](ux-check-prelaunch.md) |
| 5.2 Release gate in the binary (`duohive bench --gate`) | **Done (CI)** | Runs in CI on pushes to `main` and by hand |
| 5.3a Packaging config, set-version and release-check scripts, runbook | **Done** | |
| GitHub release binaries for macOS (Apple silicon, Intel) and Linux (x86_64, ARM64), with checksums and attestations | **Done (CI)** | `release-binaries.yml`; verified per platform by `verify-release` once it has run |
| `actionlint` and `cargo package` CI jobs | **Added** | See the CI table; they were written earlier on `feat/phase-5-gate` and are now in `ci.yml` |
| crates.io publish job (seven crates, in order) | **Open: agent** | Needs the author's items 3 and 4 |
| Homebrew tap formula and publish job | **Open: agent** | Needs the tap repository and `TAP_TOKEN` |
| 5.3b Release candidate and v1.0.0 through crates.io and Homebrew | **Open: author** | The GitHub alpha pre-releases are done |
| 5.4 README, CONTRIBUTING, SECURITY, CHANGELOG | **Done** | Links and the version entry are now checked by the `docs` job |
| README screenshot or recording | **Open: author** | |
| 5.5a Name lookups | **Done** | The names look free on crates.io, GitHub, `.dev` and `.ai` |
| 5.5b Trademark searches, domain, GitHub organisation, crate names | **Removed** | By the author, 2026-10-25 (P5-D12) |
| Confirm the Phase 5 defaults (P5-D1 to P5-D11, P5-Q1 to P5-Q6) | **Open: author** | P5-D1 (gate run on the author's machine, verified by a committed file) is now partly overtaken by the CI gate |
| 5.6 Three first-time users install from the README in under 10 minutes | **Open: author** | The commands themselves are checked by the `install from source` and `verify-release` jobs; the stopwatch and the real users are manual |
| 5.8 Launch: v1.0.0 tag, announcement | **Open: author** | |
| 5.7 30 consecutive days of daily use | **Calendar** | Starts at the v1.0.0 tag |
| 5.9 Hypothesis tested; close-out docs | **Open: agent** | After 5.7 |

## CI/CD and repository

| Item | Status |
|---|---|
| `main` protected: pull requests only, one owner approval with an admin bypass, squash merge only, branches deleted after merge, seven required checks, no force-push or deletion | **Done** |
| `v*` tags creatable only by repository admins | **Done** |
| Secret scanning with push protection, private vulnerability reporting, Dependabot alerts, fork PR workflows wait for approval, read-only default token | **Done** |
| Repository public | **Done** |
| `dist-workspace.toml` is not used by any workflow (the release workflow builds with plain `cargo`) | **Open: author** to decide |
| The benchmark job runs on every push to `main` (about an hour); weekly and by hand would be lighter | **Open: author** to decide |

## Later (v1.1)

Resume an old session (needs an OpenCode spike first); Codex as a third CLI and alert clicks that select the agent are candidates, not agreed. See [v1.1-roadmap](v1.1-roadmap.md).

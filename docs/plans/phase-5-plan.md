# DuoHive Phase 5 Plan — Open-source launch (outline)

**Status:** Outline · written 2026-10-05 · expanded into a full test-first plan, in the format of [phase-2-plan.md](phase-2-plan.md), when phase 5 starts. Depends on phase 4.
Sources: [PRD](../product/prd.md) (Delivery Phases row 5, Success Metrics, Key Hypothesis, Decisions Log: Distribution, Branding, Name) · [UI/UX spec](../ux/ui-spec.md) §8 · [Technical design](../design/technical-design.md) §10–§11 · [Roadmap](v1-roadmap.md) row 5 · [Traceability](v1-traceability.md).

**Goal:** Release DuoHive v1 through its install channels, gated on the release benchmark, then test the key hypothesis.

**Done when (roadmap):** DuoHive is public; 30-day daily use and < 10 minutes to a first team.

## Constraints carried from earlier phases

- Every Global Constraint of [phase 0](phase-0-plan.md#global-constraints) and [phase 2](phase-2-plan.md) still applies, as do decisions D1–D32, P2-D1–P2-D21, P4-D1, and the phase 3–4 open-question answers recorded when those phases are expanded.
- Release targets are gated on Apple M-series. Linux numbers are reported but never block a release (PRD Success Metrics, TD §10).
- The licence is Apache-2.0, and the repo stays the author's own (PRD Decisions Log: Distribution).
- Positioning leads with "Duo + your Engineer Factory", not "Hive" (PRD Decisions Log: Branding).

## Spec coverage

| Requirement | Source | Task |
|---|---|---|
| Pre-launch UX check of the phase 3–4 screens | PRD UX checks; UI §8 item 2 | 5.1 |
| Release gate: the full benchmark on every release; macOS arm64 must meet every target | TD §10 Release gate, §11 | 5.2 |
| Binary < 10 MB | PRD Success Metrics | 5.2 |
| `cargo-dist` release, Homebrew tap, the `duohive` crate | Roadmap row 5; TD §11 | 5.3 |
| README finished for launch; CONTRIBUTING | Roadmap row 5; PRD phase 5 | 5.4 |
| Name checks: USPTO and WIPO, classes 9 and 42; `duohive.dev` or `duohive.ai`; GitHub org; `duohive` crate | PRD Delivery Phases row 5 | 5.5 |
| Time to first team < 10 min, README only, 3 first-time users | PRD Success Metrics, Key Hypothesis | 5.6 |
| 30 consecutive days of daily use | PRD Success Metrics (primary), Key Hypothesis | 5.7 |
| Interviews with 3–5 leads (worth doing, not blocking) | PRD Evidence, Open Questions | 5.7 (optional) |

## Tasks

| # | Task | Files | Test or check | Spec |
|---|---|---|---|---|
| 5.1 | **Pre-launch UX check.** Check every phase 3–4 screen (Duo chat and cards, task to a role, first-run templates, Settings' Keys and Org rows, the key-binding editor) against the six PRD UX principles at 80×24 and 160×48, in both themes and with `NO_COLOR`. File each failure as a fix task and fix it here | `docs/plans/ux-check-prelaunch.md`; screen fixes in `crates/duohive-tui` | Each fix lands with its updated `insta` snapshot; the check file lists every screen × principle as pass or fixed | PRD UX principles; UI §1, §5, §6, §8 |
| 5.2 | **Release gate.** Add `duohive bench --gate`, which exits non-zero when a PRD target for the agent count is missed. A release workflow runs it at 10 and 100 agents on macOS arm64 (gating) and Linux x86_64 (reported), and checks the binary size | `crates/duohive/src/bench/{mod,gate}.rs`; `.github/workflows/release.yml`; `[profile.release]` in `Cargo.toml` | Unit: `gate_fails_on_each_missed_target`, `gate_passes_at_target`, `linux_never_gates`. Workflow: `actionlint` passes; a dry run on a tag in a fork | TD §10, §11; PRD Success Metrics |
| 5.3 | **Packaging and a release candidate.** `cargo-dist` builds GitHub Release binaries (macOS arm64 and x86_64, Linux x86_64 and aarch64), a Homebrew tap formula that depends on tmux, and the crates.io publish. Reconcile `install.sh` and `make install` with the released binaries. Publish `v1.0.0-rc.1` through every channel as a pre-release | `dist-workspace.toml` (or `[workspace.metadata.dist]`); `.github/workflows/release.yml`; `install.sh`; crate `Cargo.toml` metadata (description, repository, readme, keywords) | `cargo publish --dry-run` for each crate in dependency order; `brew install --build-from-source` of the generated formula on a clean macOS user; `duohive --version` matches the tag | TD §11; roadmap row 5 |
| 5.4 | **README for launch.** Cover install (all channels), the first team in under 10 minutes (one template, then Give work), Duo, Needs you, the tmux requirement, and a screenshot or recording. Bring CONTRIBUTING, SECURITY and CHANGELOG (v1.0.0 entry) up to date | `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md` | `typos` passes; every command in the README runs as written on a clean machine (done during 5.6) | PRD User flow: install to a working team; Branding |
| 5.5 | **Name checks.** USPTO and WIPO searches in classes 9 and 42; register `duohive.dev` or `duohive.ai`; claim the GitHub org and the `duohive` crate name (and `duohive-*` for the workspace crates) | `docs/plans/name-checks.md` (results and dates) | Each check recorded with its date and result; the launch waits on a clear result | PRD Delivery Phases row 5; Decisions Log: Name |
| 5.6 | **Time to first team.** Three first-time users install the release candidate from the README only and get three working agents. Time each one and record where they stalled; fix what blocks | `docs/plans/first-team-trials.md` | All three under 10 minutes, or the blocking item fixed and the trial repeated | PRD Success Metrics; Key Hypothesis |
| 5.7 | **30-day daily use.** The author keeps a self-report log: one line a day saying whether DuoHive was the only agent tool, plus any other tool used and why. Optionally, interview 3–5 leads who run 5+ agents | `docs/plans/daily-use-log.md`; `docs/product/prd.md` (Status line and Evidence once validated) | 30 consecutive "only tool" days. A break restarts the count and logs its reason as a fix task | PRD Success Metrics (primary); Key Hypothesis; Evidence |
| 5.8 | **Launch.** Tag `v1.0.0`; the release workflow publishes every channel; mark roadmap row 5 "released · 30-day test running" | `docs/plans/v1-roadmap.md`, `docs/README.md` | The release workflow is green on the tag; `brew install`, `cargo install duohive` and the release binary each run `duohive --version` | Roadmap row 5 |
| 5.9 | **Hypothesis tested.** After 5.7's 30 days pass: mark roadmap row 5 and the PRD Delivery Phases row done, and flip the PRD status from Draft | `docs/plans/v1-roadmap.md`, `docs/product/prd.md`, `docs/README.md`, `docs/design/technical-design.md` §12 | 5.6 and 5.7 both recorded as passed; TD §12 matches the roadmap | Roadmap row 5; PRD Key Hypothesis |

**Order:** 5.1, 5.2 and 5.5 can run in parallel. 5.3 needs 5.2 (one workflow), and 5.4 needs 5.3. 5.6 runs on the release candidate and needs 5.3 and 5.4. 5.8 tags once 5.1–5.6 pass. 5.7 runs over the 30 days after the tag, and 5.9 closes the phase.

## Risks

| Risk | Response |
|---|---|
| The release gate fails at 100 agents on Apple M-series | Phase 4 tuned this; the gate shows which target failed. Fix before tagging; never lower a target without a PRD change |
| A name check finds a conflict | PRD Branding allows a rename at launch (Duocrew, DuoHQ were considered); decide with the author before 5.3 publishes crates |
| Publishing seven crates for `cargo install duohive` | Publish in dependency order (`duohive-core` first), or see P5-Q2 |
| A first-time user stalls on tmux or CLI install | The first-run screens for no tmux and no CLI (UI §4.12, §4.3) give the steps; README covers both |

## Open questions (decide when the phase starts)

| # | Question | Default if not decided |
|---|---|---|
| P5-Q1 | The GitHub repository is already public. Does "DuoHive is public" mean the v1.0.0 release on every channel plus an announcement? | Yes: tag, channels and an announcement post |
| P5-Q2 | `cargo install duohive` needs every `duohive-*` crate on crates.io. Publish all seven, or ship only the binary crate with the others vendored? | Publish all seven at the same version |
| P5-Q3 | When does the 30-day clock start? | The day `v1.0.0` is tagged. Earlier days count if DuoHive was already the only tool, and the log shows it |
| P5-Q4 | Release targets (TD §11 names none) | macOS arm64 and x86_64, Linux x86_64 and aarch64 |

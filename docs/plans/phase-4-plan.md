# DuoHive Phase 4 Implementation Plan — Scale and polish

**Status:** Built, acceptance open · tasks 4.1 and 4.3–4.9 built (2026-10-20 to 2026-10-22); the items under [Open items](#open-items-carried-to-the-end-of-the-phase) are not done · outline written 2026-10-05, specs revised 2026-10-19 after a dual-reviewer check, expanded into this task-by-task plan 2026-10-20. Depends on phase 3 (Agent Squad needs task to a role). The defaults in Constraints (the measurement method, the 3–4 week estimate, one binding per action with context-scoped conflicts, the gate run with the view on) are the author's to confirm; they are the first item in the TODO checklist.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
Sources: [PRD](../product/prd.md) (Success Metrics; capability rows for templates, model and effort, key and mouse; Setup templates; UX principles) · [UI/UX spec](../ux/ui-spec.md) §4.1.1, §4.3, §4.4.1 (model fields), §4.4.2 (menu, move, drag), §4.8 (replace org), §4.10, §4.11, §5, §6 · [Technical design](../design/technical-design.md) §4.3, §5.2 (model lists), §7, §8, §10 · [Roadmap](v1-roadmap.md) row 4 · [Traceability](v1-traceability.md).

**Goal:** DuoHive meets every Success Metric at 100 agents on Apple M-series. A new user gets a ready-made org from a template. Model and effort come from each CLI. Every action works by key and by mouse, with a `?` map and editable bindings.

**Done when (roadmap):** Every benchmark target in Success Metrics (idle CPU, memory, status freshness, start-up, binary size) passes on Apple M-series.

## Review Focus

The spec is silent on these; each has a test in the owning task.

1. A `settings.toml` `[keys]` table that is garbage (an unknown id, a key that does not parse, two actions on one key): defaults apply, notices show, nothing crashes (4.6).
2. A CLI whose model command hangs or is not installed: the form still opens and typing works (4.5, `list_models_times_out_after_10s`).
3. Replacing the org while an agent waits on an approval or holds a worktree: nothing is written if a stop fails, worktree rows are kept (4.4).
4. A template directory written with `~`, a trailing space or a path that does not exist (4.3, `squad_directory_kept_as_typed_and_trimmed` and the error states).
5. Mouse events past the last row or column, or during a resize (4.7, `mouse_outside_screen_ignored`).

## Constraints carried

- **Inherited:** every Global Constraint of phases 0 and 2, decisions D1–D32, P2-D1–P2-D28, P3-D1–P3-D31 (P3-D8 sets the binary-size budget, P3-D20 the Move edit) and the phase 3 open-question answers (P3-Q).
- **Targets** (PRD Success Metrics), gated on Apple M-series; Linux numbers are reported, not gating:

  | Metric | At 10 agents | At 100 agents |
  |---|---|---|
  | Idle CPU | < 1 % | < 3 % |
  | DuoHive memory | < 20 MB | < 60 MB |
  | Hook state to screen | < 1 s | < 2 s |
  | Screen-read (`~`) state to screen | < 2 s | < 2 s |
  | First frame | < 100 ms | < 300 ms |
  | Binary size | < 10 MB | |

- **How the targets are measured** (the gate). TD §10 "Method (phase 4)" is the one definition; this is its summary. Drafted 2026-10-19 as defaults for the author to confirm when the phase starts:
  - **Machine and runs:** Apple M-series, plugged in; a run whose 1-minute load average is above 2 is discarded (the bench exits 3 with `discarded: load`). Three counted runs each at 10 and 100 agents with the live view on (figures without it are reported); the worst of the three decides, and a discarded run is retried, twice at most, before the gate fails.
  - **Idle CPU:** mean over 60 s after 30 s of settling.
  - **Memory:** DuoHive's resident size plus the tmux control clients' (the bench's own and, with the view on, the view's), sampled at the end of that window; agents and the `duohive mcp` helpers excluded (the helpers' total is reported). The whole-run peak is still reported but doesn't gate.
  - **Hook state to screen:** the slowest of 100 samples. **Screen-read (`~`) state to screen:** p95 of 100 samples; only every fifth fake agent is read from the screen, so at 100 agents (20 of them) five turns give 100 samples, and `--turns` defaults from the agent count to reach 100 of each kind (about 50 at 10 agents, 5 at 100; test `turns_default_reaches_100_samples`).
  - **First frame:** the reopen case. The bench starts the system with N agents, stops DuoHive leaving their tmux sessions, starts it again, and times from the start of `run()` to the first complete draw into the test backend. Exec and dynamic-linking time is not included; the wall time of `duohive --version` is reported beside it.
  - **Binary size:** the file written by `cargo build --release`, where MB means 10,000,000 bytes.
- **Templates** (PRD Setup templates, UI §4.3):

  | Template | What it creates |
  |---|---|
  | Agent Squad | One Generalist role directly under Duo, with N agents (default 3, 1–100) sharing one directory and CLI, worktrees on (set on the role, not in the org defaults) |
  | Product Team | One team with Backend, Frontend and QA roles, one agent each, with a starter prompt per role |
  | Engineering Org | Several named teams, each with Backend and QA roles and its own repo |
  | Blank | You and Duo only |

  Everything a template creates stays editable.
- **What a first run writes in `[org]`:** only `cli`, which TD §7 requires: Duo's CLI chosen in step 1. Every other org default is left unset, and `[duo]` gets the name `Duo` and the same CLI. A template then sets CLI, directory and worktrees on the teams or the Generalist role it creates, never in `[org]`. Blank writes just `[org] cli` and `[duo]`.
- **New org from template** replaces the org's tree (teams, roles, agents) after a confirmation, keeping `[duo]` and the org-level defaults and prompt (P4-D3). Running agents in the old tree are stopped first; Duo keeps running; and the old `org.toml` is kept as a timestamped backup (TD §7, UI §4.8).
- **Model and effort lists** are read from each CLI at runtime, never hard-coded:
  - **Claude Code:** the aliases and `--effort` choices come from `claude --help`, and any other model is typed;
  - **OpenCode:** `opencode models` lists the models. The plan expected `opencode models --verbose` to give each model's variants as its efforts; spike S4 found no `--verbose` on OpenCode 2.1.1, so the Effort field takes typed text there (P4-Q7).

  While loading, the field shows `loading models…`. On failure it shows `✕ couldn't list <cli>'s models · type a name or keep inherit` (TD §5.2, UI §4.4.1).

## Author decisions

P4-D1 is recorded in the scope note below.

| # | Decision |
|---|---|
| P4-D2 | **The mouse everywhere.** Decided by the author on 2026-10-17. Task 4.7 adds clicks on dialog and form buttons, list rows (Worktrees, Settings, Move to…, pickers, the key-binding editor) and first-run steps, each doing what its key does. P2-D25's ignored clicks in dialogs, confirmations, Settings and Worktrees are phase 2 only. In every phase the wheel still does nothing there, the too-small screen ignores the mouse (wheel included), and clicks are ignored while zoomed; UI §4.1.2 and §5 say so. PRD Should is unchanged. |
| P4-D4 | **Order, estimate and who owns `[profile.release]`.** Set on 2026-10-19 after the spec check. The keymap layer (4.6) comes before the keys and screens that use it; the mouse (4.7) is last, since each click test needs its screen. The estimate becomes 3–4 weeks (roadmap row 4), since P4-D2 put the mouse on every screen. Task 4.1 owns `[profile.release]` (strip, lto, codegen-units) and the size measurement; 5.2 only gates on the result. |
| P4-D3 | **What New org from template keeps.** Decided by the author on 2026-10-17. `[duo]` (name, CLI) and the org-level defaults and prompt are kept; only the tree (teams, roles, agents) is replaced. Running agents in the old tree are stopped after the confirmation. Duo keeps running. Agent Squad's directory, CLI and worktrees on live on its Generalist role, not in the org defaults, so keeping the org defaults doesn't conflict. Product Team's and Engineering Org's CLI and worktree choices are set on each team they create, not in the org defaults. Built in 4.4; TD §7 and UI §4.8 say so. |

## Scope note

- **What this phase adds beyond the roadmap's wording:** roadmap row 4 says "key and mouse completeness". This plan reads that as every remaining UI §4.11 binding (`?`, `f`, `m`), the rest of the mouse (UI §5) beyond P2-D25's wheel and left click, and Settings' Keys row and the Org group's `New org from template…` row (P2-D13).
- **Also assigned here:** the accessibility re-check (UI §2.3, §6), as polish; no other phase owns it. Theme `auto` moved to phase 2 (P2-D26).
- **The three phase tables match:** roadmap row 4, PRD Delivery Phases row 4 and TD §12 row 4 list the accessibility re-check, since 2026-10-05 (theme `auto` moved to phase 2 on 2026-10-16, P2-D26). The rest falls under their "key and mouse completeness", "the key-binding editor" and "first-run setup with templates".
- **Recorded as P4-D1:** the accessibility re-check (theme `auto` moved to phase 2, P2-D26), `f`, `m` and Settings' Keys row and the Org group's `New org from template…` row belong to phase 4. The UI spec requires them, and no other phase owns them.

## Spec coverage

| Requirement | Source | Task |
|---|---|---|
| Full `bench`: 100 agents, `~` freshness, first frame at 10 and 100, binary size; tuning | PRD Success Metrics; TD §4.3, §10; roadmap row 4 | 4.1, 4.2 |
| First-run setup (3 steps), the no-CLI screen | PRD Setup templates, User flow step 2; UI §4.3; TD §8 (deferred by D28) | 4.3 |
| Templates: Agent Squad, Product Team, Engineering Org, Blank | PRD Must; UI §4.3 | 4.3 |
| New org from template, replace confirmation, timestamped backup | PRD The org file; UI §4.8, §4.10; TD §7 | 4.4 |
| Model and effort picker | PRD Should; TD §5.2; UI §4.4.1; replaces D14 | 4.5 |
| `?` key map; key-binding editor; bindings applied everywhere | PRD Should; UI §4.10, §4.11 | 4.6 |
| Mouse parity: right-click menu, the wheel over the org pane and Needs you, drag node, drag chat edge, header counts, clicks in dialogs, forms, lists and first run (P4-D2; wheel and click to select: P2-D25) | PRD Should; UI §4.4.2, §5 | 4.7 |
| `m` Move to… with the inherited-values diff | UI §4.4.2 | 4.8 |
| `f` filter by state | UI §4.1.1 | 4.9 |
| Colour-blind safety: a state is never shown by colour alone (glyph and word per state), contrast re-verified (theme `auto` follows the terminal since P2-D26) | UI §2.3, §4.10, §6 | 4.9 |
| Phase 4 acceptance: every target on Apple M-series | Roadmap row 4 | 4.10 |

## File structure

| Crate | New | Modified |
|---|---|---|
| `duohive` | `tests/bench_gate_script.rs`; `scripts/bench-gate.sh`; `src/system/models.rs` (model listing after the first frame) | `src/bench/{mod,report,measure,observe}.rs`, `src/args.rs`, `src/main.rs`, `src/fake_agent.rs` (`--stamp-file`), `src/system.rs`, `src/system/detection.rs` and `src/system/enriching_store.rs` (first-run hand-off, model listing, forwarding `replace_org`), `Makefile`, `.github/workflows/ci.yml` |
| workspace | | root `Cargo.toml` (`[profile.release]`) |
| `duohive-cli` | `src/models.rs` (parsers for `claude --help` and `opencode models`; S4); fixtures under `tests/fixtures/models/` (under `crates/duohive-cli/tests/fixtures/models/`, which `_typos.toml` already excludes; loaded with `include_str!`) | `src/{claude,opencode,fake}.rs` (`fake.rs` passes `--stamp-file`), `src/lib.rs` |
| `duohive-core` | `src/templates.rs`; `src/engine/{create_org,replace}.rs` with `create_org/tests.rs` and `replace/tests.rs` beside them (the repo's layout: a module's tests sit in `<module>/tests.rs`, registered with `#[cfg(test)] mod tests;`) | `src/{command,event,adapter,ports,lib}.rs` (new `Command`, `Event`, `Input` and `Store` items), `src/engine/{mod,timers,lifecycle}.rs` (`set_first_run`, the replace guard in `launch_or_queue`, `next_deadline` and `on_tick` for the pending replace, the new match arms), `src/testkit.rs` (the fake `Store` and a stub `list_models`) |
| `duohive-store` | `src/model_cache.rs` | `src/org_file.rs` (timestamped backup, `backup_stamp`), `src/sqlite_store.rs` (the new `Store` method), `src/lib.rs`, `src/settings_file.rs` (tests only: `[keys]` round trip) |
| `duohive-tui` | `src/keymap.rs`; `src/filter.rs`; `src/first_run/{mod,steps,forms}.rs`; `src/forms/{dropdown,move_to}.rs`; `src/render/{help,keys_editor,menu,first_run,dropdown}.rs`; `src/input/{menu,drag}.rs` | `src/{input,model,theme}.rs`, `src/input/{panes,mouse,settings,worktrees,give_work,cards}.rs`, `src/chat.rs`, `src/render/{header,org_pane,settings,dialog,mod}.rs`, `src/forms/{field,org,agent,role,team}.rs`, `src/tree.rs` |

Dependency rule (TD §3.4) is unchanged: `duohive-core` gains `templates.rs` and a `list_models` method on `CliAdapter` but does no I/O; the model cache and the backup live in `duohive-store`; the parsers live in `duohive-cli`.

## Tasks

The task numbers are not in execution order: 4.6 runs before 4.3, and 4.7 runs after 4.9. The Order paragraph after the last task and the TODO checklist give the order to follow.

Test names are the ones the checklist and the Spec coverage table cite. Every task ends with `make check` and a conventional commit on its own branch. Where a signature is given, it is the one later tasks use.

### Task 4.1: Full benchmark

**Files:**
- Modify `crates/duohive/src/bench/{mod.rs, report.rs, measure.rs, observe.rs}`, `args.rs`, `main.rs`, `fake_agent.rs` (the `--stamp-file <path>` flag of `bench-agent`), `crates/duohive-cli/src/fake.rs` (`fake_adapters_with_stamps(exe, dir)`, which passes `--stamp-file <dir>/<agent>` from the `LaunchSpec`'s agent name).
- Create `scripts/bench-gate.sh`, `crates/duohive/tests/bench_gate_script.rs`.
- Modify `Makefile` (`bench-gate`), `.github/workflows/ci.yml` (a macOS job that runs `duohive bench --agents 100` and prints the report; reported, not gating), the workspace root `Cargo.toml` (`[profile.release]`).

**Interfaces:**
- Consumes: `run_bench_detailed(BenchArgs) -> Result<BenchOutcome, BenchError>`; `BenchReport`, `Targets::for_agents`, `BenchOutcome::to_json`; `measure::{load_average, process_stats, peak_rss_mb}`; `Observer`; `SCREEN_READ_EVERY`.
- Produces: `BenchError::LoadTooHigh { load: f64 }` (exit code 3, message `discarded: load <n>`); `BenchReport` gains `rss_end_mb: f64`, `tilde_ms: Percentiles`, `first_frame_ms: Option<f64>` (the existing `peak_rss_mb` stays, reported but not gated), and `BenchOutcome` gains `helpers_rss_mb: Option<f64>` and `exec_ms: Option<f64>` (every `BenchReport`/`BenchOutcome` struct literal in `bench/mod.rs`'s tests, at `bench/mod.rs:182` (production) and about 590, 612, 688 and 726, gets the new fields, and the `Display` and JSON tests are updated with them); `Targets` gains `tilde_ms: 2000.0` and `first_frame_ms: 100.0` / `300.0`; `BenchOutcome::to_json` gains `targets: {idle_cpu, memory, hook, tilde, first_frame: {value, target, pass}}` and `all_pass: bool`; `BenchArgs::default()` becomes `idle_secs: 60` and `turns: 0` meaning "derive" (only the default may be 0: `args.rs`'s `count()` still rejects `--turns 0`) through `pub fn turns_for(agents: u32, requested: u32) -> u32`; `measure::current_rss_mb(pid: u32) -> Option<f64>` (a `process_stats` wrapper, no new `unsafe`). `BenchArgs` gains no bool (the workspace denies `struct_excessive_bools`).

- [x] **Step 1: Write the failing tests** (inline `#[cfg(test)]`, beside the code)
  - `report.rs`: `report_lists_every_target` (the `Display` text has a line each for idle CPU, memory, hook, `~` and first frame, each ending `MISSED` only when over); `memory_is_end_of_window_not_peak` (a report with `peak_rss_mb: 80.0`, `rss_end_mb: 15.0` at 10 agents passes memory); `tilde_freshness_judged_on_p95` (p95 1.9 s passes, 2.1 s is `MISSED`); `first_frame_missed_is_said_so` (`Some(320.0)` at 100 agents is `MISSED`; `None` prints `not measured` and counts as a miss in `all_pass`); `json_has_per_target_pass` (the keys above).
  - `mod.rs`: `turns_default_reaches_100_samples` (`turns_for(10, 0) >= 50`, `turns_for(100, 0) >= 5`, and an explicit `--turns 7` is returned unchanged); `helpers_memory_reported` (the outcome carries `helpers_rss_mb: Option<f64>` and the report prints it as `reported`); `first_frame_measured_on_reopen` (an integration test in `tests/bench.rs`: `--agents 2 --idle-secs 2 --turns 1 --json` has `first_frame_ms > 0`); `tilde_freshness_measured` (an integration test with `--agents 5`: every `SCREEN_READ_EVERY`-th agent is a no-hook fake, so `tilde_ms.max > 0`); `fake_agent_stamps_turn_end` (in `fake_agent.rs`: with `--stamp-file`, the fake appends `now_ms()` to the file at the moment it prints the prompt after a turn).
  - `args.rs`: `bench_defaults_are_the_gate_method` (`idle_secs == 60`).
  - `main.rs`: `load_above_ceiling_exits_3_discarded` (`bench_failure_code(&BenchError::LoadTooHigh{load: 2.5}) == 3`; the message starts `discarded: load`).
  - `tests/bench_gate_script.rs`: `worst_of_three_decides` (the script run against a stub `DUOHIVE_BIN` that prints `--json` fixtures: two passing runs and one failing run exit non-zero, the failing target named; all passing exits 0); `gate_runs_with_the_view_flag` (the stub records its arguments: every counted run has `--view`, the extra informational runs do not); `discarded_run_is_retried_twice` (a stub exiting 3 twice then passing exits 0; exiting 3 three times exits 3).
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive --lib -- bench` and `cargo test -p duohive --bins -- load_above_ceiling` and `cargo test -p duohive --test bench --test bench_gate_script`
- [x] **Step 3: Implement the bench changes**, one at a time, each turning its tests green:
  - [x] 3a. `Targets`, `BenchReport`, `Display` and `to_json` additions, and the literal updates (`report_lists_every_target`, `json_has_per_target_pass`, the miss tests);
  - [x] 3b. end-of-window memory: `measure::current_rss_mb(pid)` for DuoHive and the control clients (the bench's own and, with `--view`, the view's), sampled at the end of the idle window (`memory_is_end_of_window_not_peak`);
  - [x] 3c. `turns_for` and the new defaults (`turns_default_reaches_100_samples`, `bench_defaults_are_the_gate_method`);
  - [x] 3d. the `~` timing: the no-hook fakes get `--stamp-file`; `Observer` gains a second sample set where t0 is the last value in the agent's stamp file (the fake's own `now_ms()` when it printed the prompt) and t1 is the end of the frame that shows the agent at rest (`tilde_freshness_measured`, `fake_agent_stamps_turn_end`);
  - [x] 3e. first frame on reopen: after the turns, shut the first system down with `handle.shutdown()` (it leaves the tmux sessions, as `q` does); start the clock just before the second `start_system` call on the same `Paths` (the point `startup.rs`'s `run` reaches after parsing its arguments, so the span covers reading the files, binding the hook listener, connecting to tmux and the core's `start_up`), with `SystemOptions.first_frame: Some(rx)`, and stop it at the first complete draw into the `TestBackend` driven by `run_app_on` (`duohive-tui/src/app.rs:166`); also time `duohive --version` as `exec_ms` (`first_frame_measured_on_reopen`);
  - [x] 3f. the helpers: spawn N `duohive mcp` processes with no calls beside the fakes and sum their `process_stats` (`helpers_memory_reported`);
  - [x] 3g. the load check before the agents start (the end-of-run load includes the bench's own agents, so it is reported, not checked), above 2.0 returning `BenchError::LoadTooHigh`; with `--json` the bench prints `{"discarded": "load", "load": <n>}` before exiting 3, so the script can read it (`load_above_ceiling_exits_3_discarded`).
- [x] **Step 4: `scripts/bench-gate.sh` and `make bench-gate`.** The script (POSIX `sh`, `DUOHIVE_BIN` default `target/release/duohive`) runs `bench --agents 10 --view --json` and `--agents 100 --view --json`, three counted runs each (the gate is the live-view figure, per Constraints), then one extra run of each without `--view`, printed as information only; a run that exits 3 is retried at most twice before the script exits 3; the result is the worst value per target and `all_pass` over the worst runs; it prints one line per target and exits 0 or 1.
- [x] **Step 5: `[profile.release]`.** Add `strip = true`, `lto = "thin"`, `codegen-units = 1` to the workspace `Cargo.toml`; build with `make release`; record the size before (`make release` on this machine measured 9,829,520 B on 2026-10-19; the Phase 3 acceptance figure was 9,711,712 B) and after in `bench-results.md` "Phase 4". If the size is not under 10,000,000 B, add `opt-level = "s"` and record that. Update TD §10.
- [x] **Step 6: Run, see PASS.** `make check`; then `make release && make bench-gate` once, and record the first numbers at 10 and 100 agents in `bench-results.md` under "Phase 4" (the format of "Phase 3 acceptance", Item 4).
- [x] **Step 7: Commit.** `feat: full benchmark at 100 agents`

### Task 4.2: Tuning at 100 agents

**Files:** as found by profiling; `docs/plans/bench-results.md`. This task has no failing-test step: it is measured before and after.

**Interfaces:** consumes the gate from 4.1 (`make bench-gate`).

- [ ] **Step 1: Run `make bench-gate`** and list every target that misses at 100 agents, with its three values.
  - *Analysis 2026-10-23, from six CI runs and the local first reading (see [bench-results](bench-results.md#phase-4-acceptance)):* the one target with almost no headroom is `~` freshness at 10 agents (p95 1.94–1.98 s of 2 s). It is structural, not a slow code path. The activity poll is a `list-windows` once a second reading `#{window_activity}`, which tmux reports in whole seconds (`POLL_EVERY` in `duohive-tmux/src/runtime.rs`), so polling faster adds no finer signal; the screen read then waits `SETTLE_AFTER` = 1.5 s (`duohive/src/system.rs`). TD §4.1 estimates "about 1–1.5 s"; measured p95 is about 0.5 s above that. The cheap levers all trade something: lowering `SETTLE_AFTER` (say to 1.0 s) saves about 0.5 s but guesses `idle` sooner when an agent pauses between outputs; raising the target needs a PRD change. Decision: P4-Q8.
- [ ] **Step 2: For each miss, in order,** profile (`cargo flamegraph` or `sample` on macOS), change one thing, and re-run the gate. Candidates: activity-poll batching and settle-read concurrency (`duohive-tmux`), render diffing (`duohive-tui`), start-up work deferred after the first frame. Record before and after in `bench-results.md` "Phase 4". The S0 fallback (pause unselected panes harder, read on settle only; TD §13) is the last resort; a target that still misses after it goes to the author.
- [ ] **Step 3: Each fix is its own commit** on a `perf/phase-4-<topic>` branch: `perf: <what changed>`. A fix that changes behaviour gets a failing test first.
- [ ] **Step 4: Repeat** after 4.5 (the model lists load after the first frame) and after any task that adds start-up or render work; the task is finished when all targets pass in three runs at 10 and 100 agents.

### Task 4.6: Keymap registry, the `?` key map and the binding editor

**Files:**
- Create `crates/duohive-tui/src/keymap.rs`, `render/{help.rs, keys_editor.rs}`.
- Modify `input.rs`, `input/{panes.rs, settings.rs, worktrees.rs, cards.rs}`, `model.rs` (`UiModel.keymap`, `Dialog::KeyMap`, `Dialog::Keys`), `render/{header.rs, settings.rs, dialog.rs}`, `forms/settings.rs` (`SettingsRow::Keys`; `SettingsRow::ALL` grows from `[Self; 7]`, and `toggled` and the settings tests and snapshots follow), `app.rs` (build the keymap from `settings.keys`, send `UpdateSettings` on a change).
- Test: inline `keymap.rs`, `input/keymap_tests.rs`, `render/help_tests.rs`; the existing key tests stay unchanged.

**Interfaces:**
- Produces (`keymap.rs`):
  - `pub enum ActionId { GiveWork, StartStop, New, Edit, Move, Delete, Zoom, LastReply, OpenOrg, Settings, Worktrees, Filter, KeyMap, CardApprove, CardEdit, CardReject }` with `pub fn id(self) -> &'static str` (the TD §7 snake_case ids) and `ActionId::from_id(&str) -> Option<ActionId>`.
  - `pub enum Context { Everywhere, OrgPane, NeedsYou, AgentPane, Chat }`.
  - `pub struct Binding { pub code: KeyCode, pub mods: KeyModifiers }`, `pub fn parse_binding(&str) -> Option<Binding>` (`"g"`, `"shift+n"`, `"ctrl+g"`, `"f2"`), `Binding::label(&self) -> String`.
  - `pub struct Keymap`; `Keymap::from_settings(&BTreeMap<String, String>) -> (Keymap, Vec<String>)` (notices for an unknown id or a key that does not parse; the default applies); `key(&self, ActionId) -> Binding`; `action_for(&self, Context, &KeyEvent) -> Option<ActionId>`; `rebind(&mut self, ActionId, Binding) -> Result<(), RebindError>`; `reset(&mut self, ActionId)`; `overrides(&self) -> BTreeMap<String, String>` (only changed bindings); `contexts(ActionId) -> &'static [Context]`.
  - `pub enum RebindError { Reserved(String), Conflict { key: String, action: ActionId } }`.
  - Defaults (UI §4.11): `GiveWork` space, `StartStop` `s`, `New` `n`, `Edit` `e`, `Move` `m`, `Delete` `d`, `Zoom` `z`, `LastReply` `v`, `OpenOrg` `o`, `Settings` `,`, `Worktrees` `w`, `Filter` `f`, `KeyMap` `?`, `CardApprove` `y`, `CardEdit` `e`, `CardReject` `n`. `GiveWork` and `Zoom` live in `OrgPane` and `AgentPane`; the card actions in `Chat`; `Settings`, `Worktrees`, `Filter`, `KeyMap` in `Everywhere`.
  - Reserved (refused, and counted by the conflict check): `tab`, `shift+tab`, `esc`, `ctrl+c`, `ctrl+q`, `q`, `ctrl+up`, `ctrl+down`, the arrows, `enter`, `pageup`, `pagedown`, and the chat's `@` and `/`.
- Consumes: `Settings.keys: BTreeMap<String, String>`; `Action`, `map_key`, `update`, `UiEffect::Send(Command::UpdateSettings(..))`.

- [x] **Step 1: Write the failing tests.**
  - `keymap.rs`: `defaults_match_the_spec` (every `ActionId` has the default key above); `every_action_has_a_default_key`; `parse_and_label_round_trip` (`"ctrl+g"`, `"shift+n"`, `"f2"`, `" "`); `rebind_applies_everywhere` (`Zoom` rebound to `x`: `action_for(OrgPane, x) == Some(Zoom)` and `action_for(AgentPane, x) == Some(Zoom)`, and `z` no longer maps); `two_context_action_rebinds_both`; `conflict_refused_in_same_context` (`Edit` to `s` gives `Conflict { action: StartStop, .. }`); `same_key_allowed_in_different_contexts` (`CardEdit` stays `e` beside `Edit`); `everywhere_key_conflicts_with_pane_key` (`Filter` to `s` is refused); `reserved_keys_refused` (`q`, `ctrl+up`, `enter`, `esc`); `conflict_check_covers_reserved_keys`; `overrides_only_changed_bindings`; `backspace_restores_default` (`reset`); `invalid_saved_key_falls_back_to_default` and `unknown_id_notice_shown` (`from_settings` with `{"nope": "x", "zoom": "???"}` gives two notices and default bindings).
  - `input/keymap_tests.rs`: `rebind_applies_to_map_key` (after `rebind(StartStop, "x")`, `map_key(x)` is `Action::StartStop` and `s` is `None`); `typing_and_forms_unaffected` (in Typing mode, a form and the chat input, every key, `?` included, reaches the agent or the field); `esc_cancels_capture`; `enter_captures_next_key_and_sends_update_settings`; `reserved_key_refused_with_message` (`q is reserved`); `conflict_message_names_the_action` (`s is already Start / stop · rebind that first`); `question_mark_opens_the_key_map_selected_mode_only` and `help_key_works_zoomed`; `question_mark_ignored_in_dialogs`.
  - `render/help_tests.rs`: `help_lists_current_bindings` (a rebinding shows in the map); snapshots `key_map_160x48` (three columns), `key_map_100x30` (exactly 100 wide), `key_map_80x24` (one column, `↓ more`), `keys_editor_80x24`, `keys_editor_conflict_notice`, `keys_editor_ignored_settings_line`.
  - `render/pane_tests.rs`: update `footer_matches_focused_pane` for the `? help` strings: the org footer `… · ? help`, the Needs you footer `↑↓ move · enter go to agent · ? help`; the header carries `? help` in Selected mode and not in Typing mode; add `help_hint_in_header_and_org_needs_you_footers_80x24`.
  - `forms/settings`: `settings_has_a_keys_row_that_opens_the_editor`.
  - `duohive-store/src/settings_file.rs`: `keys_round_trip` (a `[keys]` table with an unknown id loads and saves unchanged).
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui -- keymap help keys` and `cargo test -p duohive-store -- keys_round_trip`
- [x] **Step 3: Implement `keymap.rs`** to the interfaces above (a `Vec<(ActionId, Binding)>` and a context table; no I/O).
- [x] **Step 4: Thread the keymap through input.** Add `Action::Move`, `Action::Filter` and `Action::KeyMap` with no-op handlers (4.8 and 4.9 fill them in). `UiModel.keymap: Keymap` built in `app.rs` from `settings.keys` (notices kept in `UiModel.keys_notices`); `tree_key(code)` and `pane_key(pane, code)` become functions of `(&Keymap, ..)` and map through `action_for` (a signature change; their callers and tests follow); `chat_key` is unchanged, since every character in the chat input is text; the card keys are resolved in `card_action` (`input/cards.rs`, only while `m.focus.card` is set), which maps `CardApprove`, `CardEdit` and `CardReject` through the keymap; the footers in `render/header.rs` become functions of `(&Keymap, mode)` that print `Binding::label` (the default strings are unchanged apart from `? help`: the existing snapshots of the footers at 80×24, 100×30 and 160×48 stay byte-identical except for that suffix, which `footer_matches_focused_pane` pins); the Worktrees screen's `d` stays fixed (it is that screen's own remove key, not the `Delete` action).
- [x] **Step 5: `?`, the key map and the editor.** `?` opens `Dialog::KeyMap` from the main screen (any pane, zoomed too); it is a character in Typing mode, forms and the chat input, and ignored in dialogs. `Dialog::Keys` is opened by Settings' new Keys row: `↑↓`/`PgUp`/`PgDn` move, `enter` starts the capture (`press the new key · esc cancels`), the next key is taken through `Keymap::rebind`, `backspace` calls `reset`; on success emit `Command::UpdateSettings` with the new `keys`; on error show the refusal on the row (UI §4.10 wording).
- [x] **Step 6: Run, see PASS.** `make check`; review the snapshot diffs against UI §4.10–§4.11.
- [x] **Step 7: Edit the docs** (TD §7 and UI §4.10 against what was built), then commit. `feat: keymap registry, key map and binding editor`

### Task 4.3: First-run setup and templates

**Files:**
- Create `crates/duohive-core/src/templates.rs`; `crates/duohive-tui/src/first_run/{mod.rs, steps.rs, forms.rs}`, `render/first_run.rs`.
- Modify `duohive-core/src/{command.rs, event.rs, lib.rs, engine/*}`, `duohive/src/system.rs` and `startup.rs` (hand the first-run flag to the app), `duohive-tui/src/{model.rs, app.rs, input.rs}`.

**Interfaces:**
- Produces (`templates.rs`):
  - `pub enum TemplateInput { AgentSquad(SquadInput), ProductTeam(ProductInput), EngineeringOrg(EngineeringInput), Blank }`, all `Clone, Debug, PartialEq, Eq, Serialize, Deserialize` (they ride inside `Command`, which derives the same; `ModelList` and `ModelListError` derive them too, for `Input` and `Event`).
  - `pub struct SquadInput { pub size: u32, pub directory: String, pub cli: CliKind, pub worktrees: bool, pub prompt: Option<String> }`; `pub struct ProductInput { pub team_name: String /* default "Product" */, pub cli: CliKind, pub directories: [String; 3] /* backend, frontend, qa */, pub worktrees: bool }`; `pub struct EngineeringInput { pub cli: CliKind, pub worktrees: bool, pub teams: Vec<(String, String)> /* name, repo */ }`.
  - `pub struct OrgTree { pub teams: Vec<Team>, pub roles: Vec<Role> }`.
  - `pub fn build_tree(input: &TemplateInput, kept_names: &BTreeSet<AgentName>) -> Result<OrgTree, TemplateError>` (pure; `kept_names` are the agent names that still have worktree rows, so a clash takes the next free number).
  - `pub fn first_run_org(duo_cli: CliKind, tree: OrgTree) -> Org` (`Org::blank(duo_cli)` plus the tree; `[org]` keeps only `cli`).
  - `pub enum TemplateError { SizeOutOfRange, DuplicateTeam(String), NoTeams, Invalid(Vec<FieldError>) }`; `pub const STARTER_PROMPTS: [(&str, &str); 4]` (Generalist, Backend, Frontend, QA; wording approved by the author, P4-Q1).
- First-run plumbing: `Snapshot` (event.rs) gains `first_run: Option<Vec<(CliKind, CliPresence)>>` (`Some` with the detected inventory on a first run, `None` otherwise); `Engine::new` already takes seven parameters (clippy's `too-many-arguments-threshold` is 7), so the value is not a new parameter: `start_system` in `system.rs` calls a new `Engine::set_first_run(&mut self, inventory: Vec<(CliKind, CliPresence)>)` right after `Engine::new` and before `start_up()` (which builds the only snapshot, `engine/reconnect.rs`) and before the engine moves into `spawn_core`; `Event::OrgCreated` clears it. Every `Snapshot` struct literal gains `first_run` (`event.rs`, `engine/mod.rs:466`, the TUI `model.rs`, `app/tests.rs`, `input/pane_tests.rs`, `render/pane_tests.rs:230`). The inventory is the list `detect_all` returns at `system.rs:243`, which already holds a `Missing` entry for each CLI not found (`first_found` then falls back to Claude only to build the blank org); the TUI shows the blocking screen when every entry is `Missing`. The `None` branch at `system.rs:244` is a detection cancelled during shutdown and is left as it is.
- Command: `Command::CreateOrg { template: TemplateInput, your_name: Option<String>, duo_cli: CliKind }` (accepted only while `first_run` is `Some`; the engine builds the tree with `build_tree`, validates, replaces its in-memory org, `Store::save_org` and `mark_last_good`, saves the settings, and emits the same org-changed event an edit emits, which `system.rs` already uses to update its `Tracker`, then `Event::OrgCreated`) and, from 4.4, `Command::NewOrgFromTemplate { template: TemplateInput }`. Creating an org starts no agent: the standing agents stay stopped until the user starts them (`s`), as for any org.
- Consumes: `Org::blank(CliKind)`, `validate::validate`, `Store::save_org`, `Store::save_settings`, `Fields`, `Role`, `Team`, `AgentDef`.

- [x] **Step 1: Write the failing tests.**
  - Core, `templates.rs`: `squad_has_n_agents_under_root_role` (`size: 3` gives one Generalist role directly under Duo with `squad-1..3`); `squad_worktrees_on` (`Fields.worktree == Some(true)` on the role, not in `OrgDefaults`); `product_team_three_roles_with_prompts` (a team named by `team_name`, Backend, Frontend, QA, one agent each, a starter prompt per role); `product_team_name_required_and_unique`; `engineering_org_skips_empty_rows`; `engineering_org_needs_one_team` (`NoTeams`); `templates_validate` (every template's `first_run_org` passes `validate`); `template_agent_names_unique`; `templates_write_only_org_cli` (`OrgDefaults` equals `Org::blank(cli).defaults`); `blank_writes_org_cli_and_duo`; `squad_size_bounds` (0 and 101 refused, 1 and 100 accepted); `template_names_skip_kept_names` (`kept_names` holding `squad-1` and `backend` gives `squad-2` first and `backend-2`, `payments-backend-2`).
  - Engine, `engine/create_org/tests.rs`: `create_org_saves_the_org_and_starts_no_agent`, `create_org_sets_your_name_and_duo_cli` (`UpdateSettings` saved, `org.duo.cli` set), `create_org_refused_when_an_org_exists`, `first_run_flag_clears_after_create`, `create_org_agents_map_matches` (the engine calls `sync_agents` after swapping the org); `create_org_updates_the_tracker_org` (the org-changed event carries the new org); `create_org_probes_directories` (the org-changed path re-probes the new org's directories); `first_run_snapshot_from_setter` (`set_first_run` shows in the next snapshot).
  - TUI, `first_run/` tests with snapshots at 80×24 and 160×48: `step1_you_and_duo`, `step1_missing_cli_radio_disabled`, `step2_templates_with_preview`, `step3_agent_squad`, `step3_product_team`, `step3_engineering_org`, `step3_blank_summary`, and the error states of UI §4.3 (`squad_directory_missing`, `squad_not_git_with_worktrees_on`, `squad_size_out_of_range`, `product_team_directory_errors`, `engineering_no_rows`, `duplicate_team_name`, `write_failed_keeps_dialog`); `no_cli_blocks` (no supported CLI found: the blocking screen, `q` quits); `esc_on_step1_asks_before_quit`; `esc_on_later_steps_goes_back`; `first_run_creates_org_command` (Create org sends `Command::CreateOrg` with the built `TemplateInput`); `first_run_applies_name_and_duo_cli`; `squad_directory_kept_as_typed_and_trimmed` (core: `~/code/app ` is stored as `~/code/app`; `~` is expanded where it is today, by `expand_home` at launch). The wizard's `✓ git repo` and directory errors reuse the forms' existing directory probe (`Form::probe_dir` in `forms/mod.rs`, called from `form_action` and backed by the `DirProbe` port), so the wizard adds no new I/O.
  - `duohive` integration `tests/app.rs`: `first_run_shows_wizard_and_create_org_starts_no_agent` (no org.toml: the first snapshot has `first_run: Some(..)`; after Create org the org is on disk, `first_run` is `None`, and no tmux window exists).
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core -- templates create_org` and `cargo test -p duohive-tui -- first_run`
- [x] **Step 3: Implement `templates.rs`** (pure builders over `Org`/`Team`/`Role`), and `Command::CreateOrg` in the engine (validate, `save_org`, `save_settings`, then the normal org-changed events).
- [x] **Step 4: Replace the silent blank org** in `system.rs` (`Loaded::FirstRun`): keep detecting the CLIs once, start the engine on `Org::blank(first_found)` (or `Claude` when none) and pass the inventory in as the `first_run` value above; the TUI keeps `UiModel.first_run` from the snapshot and builds `UiModel.wizard: Option<FirstRunWizard>` (not a `Dialog`: it fills the screen) while it is `Some`; `map_key` hands every key to the wizard first, so a text field types `q`, and only the buttons and `esc` act on it. The blocking screen shows when every CLI is `Missing`, and the wizard sends `Command::CreateOrg` from Create org. Register the new `Command`, `Event` and `Input` variants at every match site (the compiler lists them).
- [x] **Step 5: The wizard.** `first_run/mod.rs` holds `FirstRun { step, you, duo_cli, template, squad, product, engineering }` and the step transitions; `steps.rs` the three step bodies; `forms.rs` the field sets (reusing `Field`/`LineEdit`; the engineering rows use a small repeating-row field). Render per UI §4.3 with `draw_box`. Template starter prompts come from `STARTER_PROMPTS`.
- [x] **Step 6: Run, see PASS.** `make check`; review snapshots against UI §4.3.
- [x] **Step 7: Edit the docs** (UI §4.3 and TD §7/§8 against what was built; the deferred D28 row of the traceability). Commit. `feat: first-run setup with templates`

### Task 4.4: New org from template

**Files:**
- Create `duohive-core/src/engine/replace.rs` and `engine/replace/tests.rs`. Modify `duohive-core/src/{command.rs, event.rs, ports.rs, testkit.rs, engine/mod.rs}`, `duohive-store/src/{org_file.rs, sqlite_store.rs, lib.rs}`, `duohive-tui/src/{first_run/*, forms/settings.rs, input/settings.rs, render/settings.rs, input.rs}`, `crates/duohive/src/system/enriching_store.rs` (forward `replace_org`); the stop-then-write sequence lives in the engine, not in `system.rs`.

**Interfaces:**
- Produces: `Command::NewOrgFromTemplate { template: TemplateInput }`; `Event::OrgReplaced`; `Event::OrgReplaceFailed { agent: Option<AgentName>, reason: String }` (banner `<agent> would not stop · org not replaced`); a new `Store` method `fn replace_org(&mut self, org: &Org, now: Millis) -> Result<(), StoreError>` (ports.rs), implemented in `sqlite_store.rs` by `org_file::replace_org(paths, org, now)` and forwarded by `EnrichingStore` (`duohive/src/system/enriching_store.rs:40`, the third `impl Store`; a method without a default breaks the build there, so add it to all three impls), which first copies the current `org.toml` to `org.toml.<stamp>.bak` (P4-Q5) and then does what `save_org` does; the fake `Store` in `testkit.rs` records the call; `org_file::backup_stamp(now: Millis) -> String` formats `YYYYMMDD-HHMMSS` in UTC with a small pure civil-from-days routine (no date dependency); `Store::save_org` keeps writing `org.toml.bak` as today. Dropped messages and cards use the existing `save_message` (`MessageStatus::Dropped` with reason `agent removed`) and the existing card outcome path, so no further `Store` method is added. The sequence is a pending-replace state in `engine/replace.rs`: `PendingReplace { template: TemplateInput, waiting: BTreeSet<AgentName>, deadline: Millis }`. A replace keeps the old org's defaults and Duo: the new org is `Org { teams, roles, ..old }` from `build_tree`, not `first_run_org` (which is for a first run only).
- Consumes: `templates::build_tree` (not `first_run_org`: a replace keeps the old defaults and Duo), `Store::{load_worktrees, replace_org, mark_last_good}`, `preview_edit`, `Command::StopAgent`, TD §5.5's grace kill, the existing messages/cards tables.

- [x] **Step 1: Write the failing tests.**
  - Engine, `engine/replace/tests.rs` (written against the existing engine harness in `engine/harness.rs` (`Setup`, `Harness`, extended here with a `first_run` option); `replace_stops_agents_then_writes` sends `NewOrgFromTemplate` with two running agents, asserts the fake `Store` has no `replace_org` call, finishes the two stops, then asserts exactly one call and `Event::OrgReplaced`): `replace_stops_agents_then_writes` (running agents get `StopAgent`; `replace_org` is called only after every one is stopped); `replace_keeps_duo_and_org_defaults` (`[duo]` and `OrgDefaults` equal before and after); `replace_keeps_duo_running`; `timestamped_backup_kept` (a file `org.toml.<stamp>.bak` with the old text); `replace_aborts_when_a_stop_fails` (a stop that never completes: no write, `OrgReplaceFailed` names the agent); `next_deadline_includes_pending_replace` (with only a pending replace, `next_deadline()` is `Some(deadline)`); `replace_refused_while_a_worktree_is_being_created` (an old-tree agent creating a worktree, in any state, refuses the replace); `replace_excludes_duo_from_the_stop_set`; `replace_refused_while_reconnecting`; `replace_store_failure_keeps_the_old_org` (`replace_org` returns an error after the stops: `OrgReplaceFailed`, the in-memory org unchanged, the pending state cleared); `commands_rejected_while_replacing` (a `GiveWork` to an idle agent, a `GiveWork` to a stopped one, an `AssignToRole`, a `NewAgent` card approval, a Duo `give_work` tool call and a start each get `the org is being replaced`); `restart_cleared_for_the_stop_set` (no spurious rejection from a pending restart); `errored_agent_is_stopped_and_leaves_waiting`; `replace_refused_while_org_file_is_broken`; `replace_apply_happens_after_dispatch` (no runtime entry exists for a removed agent); `agents_map_matches_the_new_org`; `replace_keeps_worktree_rows` (rows survive and `kept_names` feeds `build_tree`); `replace_drops_cards_and_messages_for_removed_agents` (queued messages and pending cards naming a removed agent, team or role end as `CardOutcome::NotApplied("agent removed")`); `replace_template_names_skip_kept_worktree_rows` (engine level: the new squad starts at `squad-4` when `squad-1..3` have rows); `replace_mode_back_and_cancel`.
  - Store: `backup_name_is_timestamped_and_beside_org_toml`, `backup_stamp_formats_utc` (`0` is `19700101-000000`; a known 2026 instant).
  - TUI: `settings_org_row_opens_replace_wizard`; snapshots `replace_template_step_80x24` (`step 1 of 2`, Cancel in place of Quit, no Back on the first step), `replace_form_step`, `replace_your_org_dialog` (UI §4.8, focus on Cancel); `create_org_in_replace_mode_opens_confirmation`; `esc_goes_back_then_cancels`; `replace_confirm_cancel_returns_to_wizard` (Cancel on the confirmation changes nothing; there is no engine cancel); `replace_failed_banner`.
  - `duohive` integration `tests/e2e.rs`: `replace_org_end_to_end` (a running fake agent is stopped, the new tree is on disk, the backup exists).
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core -- replace` and `cargo test -p duohive-store -- backup` and `cargo test -p duohive-tui -- replace`
- [x] **Step 3: Implement the engine sequence** in `engine/replace.rs`:
  - [x] 3a. on `NewOrgFromTemplate`, build the tree (the kept names come from the engine's worktree rows) and validate; with no running agent in the old tree, apply at once;
  - [x] 3b. otherwise refuse up front, with `OrgReplaceFailed` and `<agent> is getting a worktree ready`, if any old-tree agent has `creating_worktree` set, in any state (`engine/worktrees.rs`: a worktree is created for a stopped or errored agent, so checking only running agents would miss it and `sync_agents` would drop it mid-creation, against P2-D20); refuse too while the start-up reconnect is still pending (`Engine.reconnect`) or `org_error` is set (as `apply_edit` refuses edits). Else clear `restart_after_stop` for the stop set, issue the existing stop for each agent in it, and record `PendingReplace` (`waiting`, `deadline` = the stop grace of TD §5.5 plus a margin). The stop set is every old-tree agent whose state is not `Stopped`, Duo excluded (P4-D3), errored agents included (they still hold a window); `waiting` is computed after the stop calls (an errored agent stops synchronously, and an empty `waiting` applies at once), and an agent that becomes errored instead of stopped also leaves `waiting`. While `PendingReplace` is set, the guard sits at the top of `on_command` (`engine/mod.rs`) for `GiveWork`, `AssignToRole`, `StartAgent`, `EditOrg`, `ReloadOrg`, `ApproveCard`, a second `NewOrgFromTemplate` and the `Tool` calls that give work, assign or edit, rejecting with `the org is being replaced`, because `give_by` sends a task to an idle or finished agent straight to `type_next_turn` without reaching `launch_or_queue` (`engine/lifecycle.rs`); the same check sits in `apply_edit` (`engine/org_edit.rs`, which a `NewAgent` card approval reaches) and in `launch_or_queue` (for the internal paths: queued messages, restarts);
  - [x] 3c. in `change_with` (`engine/mod.rs`) remove an agent that reached `Stopped` (or errored) from `waiting` and set a `replace_ready` flag when it is empty; `Engine::handle` applies after its dispatch returns (applying inside `change_with` would let `sync_agents` drop an agent's runtime state while `finish_exit` and `rt_mut` still use it, re-creating a ghost entry). Apply: `Store::replace_org`, delete the removed agents' session rows, drop their queued messages and pending cards, swap the in-memory org, call `sync_agents` (`engine/org_edit.rs`), drop cards that name a removed agent, team or role, emit the org-changed event and `OrgReplaced`;
  - [x] 3d. extend `Engine::next_deadline()` and `on_tick()` (`engine/timers.rs`) with `PendingReplace.deadline`, so `run_core` schedules a `Tick` even when nothing else is due; on that tick emit `OrgReplaceFailed` naming a remaining agent, clear the state and write nothing.
- [x] **Step 4: Settings Org row and replace mode** in the TUI (an open `Dialog` gets keys before `UiModel.wizard`, so the `Replace your org?` confirmation sits on top of the wizard; its Cancel closes only the dialog and leaves the wizard where it was, with no `back` form needed; test `replace_confirm_cancel_returns_to_wizard`): add `SettingsRow::Org` (and grow `SettingsRow::ALL`, `toggled` and the settings tests and snapshots with it); it opens the wizard at the template step with `replace: true` (titles `step N of 2`, `[ Cancel ]`, Back hidden on the first step); Create org opens `Dialog::Confirm` `Replace your org?` (UI §4.8) with the agent count, and Replace sends the command.
- [x] **Step 5: Run, see PASS.** `make check`
- [x] **Step 6: Edit the docs** (TD §7, UI §4.8/§4.10), then commit. `feat: replace the org from a template`

### Task 4.5: Model and effort picker

**Files:**
- Create `duohive-cli/src/models.rs`, fixtures `duohive-cli/tests/fixtures/models/{claude-help.txt, opencode-models.txt}`, `duohive-store/src/model_cache.rs`, `duohive-tui/src/forms/dropdown.rs`, `render/dropdown.rs`.
- Create `crates/duohive/src/system/models.rs` (the listing task) with its tests.
- Modify `duohive-core/src/{adapter.rs, cli_kind.rs, event.rs, testkit.rs}` and `engine/mod.rs` (`ModelList` and `list_models` in `adapter.rs`; `PartialOrd, Ord` derived on `CliKind` so `UiModel.models` can be a `BTreeMap`; `Input::ModelsListed`, which lives with the other inputs in `engine/mod.rs`, and its forwarding arm; `Event::ModelsListed`; the stub adapter), `duohive-cli/src/{claude.rs, opencode.rs, detect.rs, fake.rs}`, `duohive-tui/src/{model.rs, forms/{field.rs, org.rs, agent.rs, role.rs, team.rs}}`, `crates/duohive/src/system.rs`, `docs/design/technical-design.md` §5.2.

**Interfaces:**
- Produces: in `adapter.rs`: `pub struct ModelList { pub models: Vec<String>, pub efforts: Vec<String>, pub variants: BTreeMap<String, Vec<String>> }` (`variants` is per model, OpenCode only) and `CliAdapter::list_models(&self) -> Result<ModelList, ModelListError>` (default `Err(ModelListError::Unsupported)`), with `pub enum ModelListError { Unsupported, Timeout, Parse(String), Io(String) }`; `Input::ModelsListed { cli: CliKind, result: Result<ModelList, String> }`, which the engine forwards unchanged as `Event::ModelsListed { cli, result }` (there is no `ListModels` command: listing is started by `system.rs`, not requested by the UI, and the UI keeps the lists in `UiModel.models: BTreeMap<CliKind, Result<ModelList, String>>`); in `models.rs`: `pub fn parse_claude_help(text: &str) -> Result<ModelList, ModelListError>` and `pub fn parse_opencode_models(text: &str) -> Result<ModelList, ModelListError>`; in `model_cache.rs`: `pub fn read(paths: &Paths, cli: CliKind, version: &str) -> Option<ModelList>` and `pub fn write(paths: &Paths, cli: CliKind, version: &str, list: &ModelList) -> Result<(), StoreError>` (`<data_dir>/model-cache.json`, keyed by CLI and version); in the TUI: `FieldKind::Dropdown { options: Vec<String>, state: DropdownState }` and `pub struct DropdownState { open: bool, filter: LineEdit, cursor: usize, loading: bool, error: Option<String> }`.
- Consumes: `CliPresence::Found { version }`, `Field`, `LineEdit`, `draw_box`, and the Model and Effort fields it replaces (free text in the agent form, `forms/agent.rs`; `Choice` in the org and role forms, `forms/org.rs`, `forms/role.rs`).

- [x] **Step 1: Spike S4 (30 minutes, no tests).** On the pinned OpenCode 2.0 (2.1.1 was the only build installed, see spike-report S4) run `opencode models --verbose`; save the output as the fixture, note the variants format, and edit TD §5.2 (the "re-checked on 2.0 in 4.5" clause) with what it shows. Also save `claude --help` as the Claude fixture. If `opencode` is not installed on the machine, say so in `bench-results.md` and keep the recorded 1.18 fixture, marking the 2.0 check open for the author.
- [x] **Step 2: Write the failing tests.**
  - `duohive-cli/src/models.rs`: `claude_help_gives_aliases_and_efforts` (from the fixture: aliases include `sonnet`/`opus`/`haiku`, efforts match `--effort`'s choices); `claude_help_parse_failure_is_an_error`; `opencode_models_verbose_gives_variants_as_efforts`; `opencode_variants_map_to_hash_effort` (a model's variant `high` becomes launch `provider/model#high`); `empty_output_is_an_error`.
  - Store: `cache_hit_per_cli_version` and `cache_miss_on_new_version`.
  - Core, `engine/tests`: `models_listed_input_becomes_an_event`.
  - `crates/duohive/src/system/models.rs` (with fake adapters and a temp `Paths`): `list_models_uses_cache_then_refreshes` (a fresh cache answers without calling the adapter; a new CLI version calls it and rewrites the cache); `list_models_failure_is_reported` (an `Err` becomes `Input::ModelsListed` with the message); `list_models_runs_after_the_first_frame` (a `run_detection` test with a fake adapter: no `list_models` call before the `first_frame` sender fires, on the detection path and, with `first_run_inventory: Some(..)`, on the first-run path); `list_models_timeout_is_reported` (a stub adapter returning `ModelListError::Timeout` becomes an `Err` message).
  - `duohive-cli`: `list_models_kills_a_hung_child` (a helper `run_listing(program: &Path, args: &[&str], timeout: Duration) -> Result<String, ModelListError>` in `detect.rs`, beside `probe`, reads stdout on a helper thread with `recv_timeout` and kills the process group on timeout, the same way `detect::probe` does, so a long output never fills the pipe and blocks the child; the test calls the helper with a stub script's own path and a 200 ms limit, with no `PATH` change, since tests never call `set_var`). The commands are `claude --help` and `opencode models [--verbose]`; neither writes any global CLI configuration (AGENTS.md).
  - TUI dropdown: `loading_then_list` (`loading models…` then the options; typing works while loading); `typeahead_filters_117_models` (a 117-model list filtered by substring, six rows visible, `… N more · ↓ scroll`); `custom_name_accepted` (the `use "<typed>" as typed` row sets the value); `inherit_row_first_and_sets_inherit`; `inherit_uses_the_inherited_clis_list` (a role under a team on OpenCode lists OpenCode's models); `effort_resets_when_model_changes`; `opencode_effort_disabled_without_model` (`OpenCode sets effort per model · choose a model`, and enabled when the inherited model is known); `claude_effort_list_from_help_flag`; `empty_list_allows_typing`; `form_stays_usable_while_a_list_is_loading`; `failure_allows_typing` (the `✕ couldn't list <cli>'s models · type a name or keep inherit` line); `esc_closes_without_change`; snapshots `dropdown_closed`, `dropdown_open_filtered`, `dropdown_loading`, `dropdown_failed` at 100×30 and 80×24.
- [x] **Step 3: Run, see FAIL.** `cargo test -p duohive-cli -- models` and `cargo test -p duohive-store -- model_cache` and `cargo test -p duohive --lib -- system::models` and `cargo test -p duohive-tui -- dropdown`
- [x] **Step 4: Implement** in this order. The wiring: `first_frame` is a single-consumer oneshot and `run_detection` (`system/detection.rs`) already awaits it (it does the directory probe after the first frame), on a first run too. So listing is called from inside `run_detection`, after that one await, on both branches: a normal start passes the inventory it has just detected; a first run passes the inventory `start_system` already holds (cloned before it moves into `spawn_core`). `run_detection` has five parameters today, so its inputs become one `DetectionInputs` struct (the current parameters plus `paths`, the standing `Arc<Adapters>` (`adapter.rs`) and `first_run_inventory: Option<Vec<(CliKind, CliPresence)>>`) to stay within clippy's limit of 7. `models::run_model_listing(inventory, adapters, paths, inputs_tx)` has no gate of its own. The lists are loaded once per session for every CLI found, whatever the org holds, so a first run needs no second pass after Create org:
  - [x] 4a. the parsers in `models.rs` (`claude_help_gives_aliases_and_efforts` and the OpenCode tests);
  - [x] 4b. `list_models` on both adapters through `run_listing` (the CLI's own read-only command, found with the same PATH rules as `detect`, a 10 s limit); `FakeCliAdapter` returns a fixed small list;
  - [x] 4c. `model_cache.rs` (the cache tests);
  - [x] 4d. `system/models.rs`: after the first frame, for each detected CLI read the cache by version, else call the adapter on a blocking task with a 10 s timeout, write the cache, and send `Input::ModelsListed` (the `list_models_*` tests), the same way `run_detection` is gated on `first_frame`.
- [x] **Step 5: The widget.** `FieldKind::Dropdown` replaces the free-text Model and Effort fields (`FieldId::Model`, `FieldId::Effort`) in the org, team, role and agent forms; `enter` or a click opens the popup under the field (UI §4.4.1), `↑↓` move, typing filters, `enter` chooses, `esc` closes; the popup is drawn by `render/dropdown.rs` and clips to the screen.
- [x] **Step 6: Run, see PASS.** `make check`; then repeat the bench (Task 4.2, step 4) for the start-up cost of the lists.
- [x] **Step 7: Edit the docs** (TD §5.2/§7, UI §4.4.1), then commit. `feat: model and effort picker`

### Task 4.8: Move to…

**Files:** Create `duohive-tui/src/forms/move_to.rs`, `render/move_to.rs`. Modify `duohive-tui/src/{model.rs, input.rs}`, `input/panes.rs` (the handler for `Action::Move`, registered in 4.6).

**Interfaces:**
- The inherited-values diff reuses the engine's public `preview_edit(org: &Org, edit: &OrgEdit) -> Result<Org, Vec<FieldError>>` (`engine/org_edit.rs`, re-exported from `engine/mod.rs`): the picker previews `OrgEdit::Move { node, to }` for each target and compares `inherit::resolve` of the affected agents before and after. Core changes nothing.
- Consumes: `OrgEdit::Move { node: NodeKey, to: MoveTarget }` (P3-D20), `MoveTarget::{Into(NodeKey), Before(NodeName), End}`, `inherit::resolve`, `Keymap` (`ActionId::Move`).
- Produces: `pub struct MoveTo { node: NodeKey, targets: Vec<MoveOption>, cursor: usize }`; `pub struct MoveOption { pub label: String, pub to: MoveTarget, pub current: bool, pub changes: Vec<InheritedChange> }`; `pub struct InheritedChange { pub field: &'static str, pub from: String, pub to: String }`; `pub fn valid_targets(org: &Org, node: &NodeKey) -> Vec<MoveOption>`; `Dialog::MoveTo(MoveTo)`.

- [x] **Step 1: Write the failing tests** (`forms/move_to.rs` inline, `input/move_tests.rs`, `render/move_tests.rs`): `move_lists_inherited_changes` (`worktrees off → on`, `role prompt QA → Generalist` (taken from the prompt level names in `inherit.rs`, not from the stacked prompt string) for `pay-qa` into Generalist); `invalid_targets_absent` (an agent has no team as a target; a team has positions only); `move_sends_org_edit_move` (`enter` sends `Command::EditOrg(OrgEdit::Move { .. })`); `move_disabled_when_no_target` (`m` sets the footer `no other place to move <name>` and opens nothing); `move_refused_on_role_name_collision` (`<team> already has a role <role>`); `move_on_team_lists_positions` (`before <team>` rows and `at the end`); `current_target_marked_and_skipped`; `m_on_duo_does_nothing`; snapshot `move_to_100x30` and `move_to_80x24`.
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui -- move_to`
- [x] **Step 3: Implement `valid_targets`** over the org tree and `resolve` (a pure function), the dialog input (`↑↓`, `enter`, `esc`) and the render per UI §4.4.2.
- [x] **Step 4: Run, see PASS.** `make check`
- [x] **Step 5: Commit.** `feat: move to…`

### Task 4.9: Filter and the accessibility re-check

**Files:** Create `duohive-tui/src/filter.rs`. Modify `model.rs` (`UiModel.filter: Filter`), `tree.rs` (`tree_rows` reads `m.filter`), `render/{header.rs, org_pane.rs}`, `input/panes.rs`, `theme.rs`; tests in `filter.rs`, `render/filter_tests.rs`, `render/a11y_tests.rs`.

**Interfaces:**
- Produces: `pub enum Filter { Off, NeedYou, Working, Finished }` with `next(self) -> Filter`, `matches(&self, &AgentView) -> bool` (`NeedYou` = waiting, quiet or errored; `Working` = working and not quiet; `Finished` = finished), `chip(&self, compact: bool) -> Option<String>` (`filtered: ◆ · esc clears`, `filter ◆` below 100 columns); `tree_rows` keeps its signature and reads `m.filter` (`UiModel.filter`); `ActionId::Filter` handler.
- Consumes: `roll_up_counts`, `AgentView`, `Keymap`, the header counts (`needs_you.len()` includes card rows; the chip shows no count).

- [x] **Step 1: Write the failing tests.**
  - Filter: `filter_keeps_matches_and_parents`; `filter_need_you_includes_quiet_and_errored`; `filter_never_hides_cards` (the Needs you list and the chat are unchanged); `filter_empty_result_message` (`no agent is <state> · esc clears the filter`); `filter_clears_hidden_selection_to_first_match`; `esc_in_org_pane_clears_filter`; `esc_elsewhere_keeps_filter`; `f_cycles_need_you_working_finished_off`; snapshots `filter_chip_100x30` (99 columns) and `filter_chip_80x24` (`DuoHive  ● 2 · ◆ 2   filter ◆  … ? help`).
  - Accessibility: `contrast_ratios_meet_4_5` (a new test over `Palette::dark()` and `light()` for every text/background pair in UI §6's table, computed from the RGB values; `auto` is excluded because it uses the terminal's colours); `state_never_colour_only` (in `mono()` every state's row still differs by glyph and word); `no_color_new_screens_readable` and `reduce_motion_new_screens` (render the first-run steps, replace dialog, dropdown, key map, key editor, Move to… and the filter chip with `NO_COLOR` and with `reduce_motion`: no panic, glyph and word present, no spinner); `every_action_has_key` (every `ActionId` has a default key and appears in the key map text).
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui -- filter a11y contrast`
- [x] **Step 3: Implement** `Filter` (filling in the `Action::Filter` handler registered in 4.6), the `tree_rows` filtering (matches plus their parents), the chip in the header, and the empty and hidden-selection rules; add whatever the accessibility tests find (a missing glyph, a failing ratio) as fixes in `theme.rs`/`render`.
- [x] **Step 4: Run, see PASS.** `make check`
- [x] **Step 5: Commit.** `feat: filter by state and accessibility re-check`

### Task 4.7: Mouse parity on every screen

**Files:** Create `duohive-tui/src/input/{menu.rs, drag.rs}`, `render/menu.rs`. Modify `input/mouse.rs`, `model.rs` (`UiModel.menu: Option<Menu>`, `UiModel.drag: Option<Drag>`, `UiModel.org_scroll: usize`), `render/{org_pane.rs, needs_you.rs, mod.rs}`, `app/terminal.rs` (stop dropping `Drag` mouse events; `Moved` stays dropped), and the dialog input modules for the per-screen clicks; tests in `input/mouse_tests.rs`, `input/menu_tests.rs`, `render/menu_tests.rs`.

**Interfaces:**
- Consumes: `on_mouse`, `pane_areas`, `org_key_at`, `needs_you_item_at`, `Keymap`, `OrgEdit::Move`, `valid_targets` (4.8), `Filter` (4.9), the dialogs and wizard of 4.3–4.6.
- Produces: `pub struct Menu { pub node: NodeKey, pub at: Position, pub rows: Vec<MenuRow> }`, `pub struct MenuRow { pub action: ActionId, pub label: String, pub key: String, pub disabled: Option<String> }`; `pub struct Drag { pub node: NodeKey, pub over: Option<DropTarget> }`; `UiModel.org_scroll` (a scroll offset apart from the selection).

- [x] **Step 1: Replace three phase 2 tests** in `input/mouse_tests.rs`, change `motion_events_dropped` (the test at `app/terminal.rs`, about line 336, which asserts `Moved` and `Drag` events map to `None`) to `drag_events_are_forwarded_and_moved_stays_dropped`, and write the new ones:
  - `wheel_ignored_in_dialogs_confirmations_settings_worktrees` and `mouse_ignored_on_too_small_screen` (wheel included) replace `mouse_ignored_in_dialogs_confirmations_settings_worktrees_and_too_small`;
  - `wheel_scrolls_org_and_needs_you_without_moving_selection` replaces `wheel_ignored_over_org_and_needs_you`;
  - `right_click_opens_menu_and_drag_moves_node` replaces `right_middle_click_and_drag_ignored` (middle-click stays ignored: `middle_click_ignored`);
  - kept: `click_ignored_while_zoomed`.
  - New gestures: `right_click_menu_lists_every_action_with_its_key`, `menu_disabled_row_gives_the_reason`, `menu_stays_on_screen_at_the_edges`, `menu_closes_on_esc_click_elsewhere_and_action`, `team_role_duo_menus_differ` (Duo has Edit org; team and role have New here, Edit, Move to, Delete, and Give work for roles), `drag_highlights_valid_targets`, `invalid_drop_refused_with_reason` (footer text), `drop_opens_move_to_when_inherited_values_change`, `drag_chat_edge_resizes`, `header_count_click_sets_filter_for_two_states` (working and need you; finished by `f` only), `typing_mode_right_click_and_drag_ignored` (P4-Q4); `mouse_outside_screen_ignored` (events past the last row or column, or arriving during a resize, change nothing).
  - One click test per screen: `click_dialog_buttons`, `click_form_buttons`, `click_worktree_rows`, `click_settings_rows`, `click_move_to_rows`, `click_picker_rows` (the dropdown), `click_key_binding_rows`, `click_first_run_steps`, `click_replace_dialog_buttons`.
  - Accessibility and parity: `every_action_has_a_click_path` (each `ActionId` is reachable from the menu or a screen control), `no_color_menu_and_drag_readable`, `reduce_motion_menu_and_drag`.
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui -- mouse menu`
- [x] **Step 3: Implement** the menu (built from the node's valid actions through the keymap), the drag state machine in `drag.rs` (press on a row, `Drag(Left)` updates `over`, `Up(Left)` drops; the terminal now forwards `Drag` events only, because `Moved` fires on every pointer motion and is dropped on purpose so it cannot fill the input queue, `app/terminal.rs`), the org and Needs you scroll offsets, and the per-screen click handlers (each does what its key does, by sending the same `Action` the key would).
- [x] **Step 4: Run, see PASS.** `make check`; review snapshots against UI §4.4.2 and §5.
- [x] **Step 5: Commit.** `feat: mouse parity on every screen`

### Task 4.10: Acceptance

**Files:** `docs/plans/bench-results.md`, `docs/plans/v1-roadmap.md`, `docs/product/prd.md`, `docs/README.md`, `README.md`, `docs/plans/v1-traceability.md`, `docs/design/technical-design.md` §12. No code under test.

- [ ] **Step 1: Gate run.** On Apple M-series, plugged in, with the 1-minute load at most 2: `make release && make bench-gate` (10 and 100 agents, three counted runs each, `--view` on). Record each run in `bench-results.md` "Phase 4 acceptance" in the format of Phase 3 (the Item 4 table), with the figures without the view beside them. Every target must pass in all three runs. A miss goes back to Task 4.2.
- [ ] **Step 2: A one-hour run with real agents** (Claude Code and OpenCode, a few agents each, normal work): idle CPU and memory stay within the 10-agent targets; record it.
- [ ] **Step 3: Walk the screens** once by hand in iTerm2 at 80×24 and 160×48: first run, replace, dropdown, key map and editor, filter, Move to…, the menu and a drag. File any failure as a fix task here.
- [ ] **Step 4: Update the docs.** TD §12 row 4 checked against what was built; roadmap row 4 `done <date>` and row 5 next; PRD Delivery Phases row 4; `docs/README.md` and `README.md` status; the traceability matrix; this plan's header `Status: Done <date>`.
- [ ] **Step 5: `make check`, then commit.** `docs: phase 4 acceptance and status`

**Order (P4-D4):** 4.1, then 4.2 (it repeats after any later task that adds start-up or render work). Then 4.6 (the keymap registry, which already holds `m` and `f`, and the editor), since 4.8 and 4.9 read their keys from it. Then 4.3 → 4.4, and 4.5 and 4.8 in any order, then 4.9 (its accessibility renders cover the screens of 4.3, 4.4, 4.5, 4.6 and 4.8). 4.7 comes after 4.3, 4.4, 4.5, 4.6, 4.8 and 4.9, since each click test needs its screen (4.4 supplies the replace wizard, its dialog and the Settings Org row). After 4.5, repeat the bench (4.2) to measure the model-list load. 4.10 is last.

## TODO checklist

Tick each box when the task is done. The steps inside each task above are ticked as they are done; this list is the overview. A task is done when `make check` passes and its commit is on a branch named `feat/phase-4-<topic>` (`perf/phase-4-<topic>` in 4.2), never `main`. Branches are stacked: each later task is cut from the branch of the task it depends on until that is merged. Commit messages are conventional commits, with no `Co-Authored-By` line. Nothing is pushed by an agent. Tasks 4.2 (tuning), 4.10 (acceptance) and the 4.5 spike are not test-first.

**Before 4.1**

- [ ] The author confirms the defaults drafted on 2026-10-19: the measurement method (Constraints), P4-D4's 3–4 week estimate and order, one binding per action with context-scoped conflicts (UI §4.10), and the gate running with `--view` on
- [ ] Expand this outline into the full task-by-task plan, in the format of [phase-3-plan.md](phase-3-plan.md) (files, test code, commands, expected output), and have it reviewed
- [ ] The author approves the template starter prompts (P4-Q1) and the bench's realistic-output profile (P4-Q2)

**Tasks, in order**

- [x] 4.1 Full benchmark
- [ ] 4.2 Tuning at 100 agents (repeat after 4.5)
- [x] 4.6 Keymap registry, the `?` key map and the binding editor
- [x] 4.3 First-run setup and templates
- [x] 4.4 New org from template
- [x] 4.5 Model and effort picker (spike S4 first)
- [x] 4.8 Move to…
- [x] 4.9 Filter and the accessibility re-check
- [x] 4.7 Mouse parity on every screen (after 4.3, 4.4, 4.5, 4.6, 4.8 and 4.9)
- [ ] 4.10 Acceptance

## Open items carried to the end of the phase

Decided 2026-10-22: the build is complete, and what could not be done in the build sessions is kept here and finished at the end of the phase, before `Status: Done`. None of them is a code task.

- [ ] **4.10 step 1, gate run.** *Evidenced 2026-10-25 by the full gate on a GitHub macOS runner (see [bench-results](bench-results.md#phase-4-acceptance)): at 100 agents every target passes in all three runs; at 10 agents everything passes except first frame (167.7 ms against 100 ms, two of three runs), on a shared 3-CPU VM with the load check off. The method asks for the M-series machine at load 2 or less, so this step stays open for that run.* `make release && make bench-gate` with the 1-minute load at most 2, three counted runs at 10 and 100 agents, recorded in `bench-results.md` "Phase 4 acceptance". Attempted 2026-10-22: the machine's baseline load was about 3, and every attempt was discarded for load (the gate exited 3). The figures in "Phase 4" were taken with the load gate off and are a first reading only.
- [ ] **4.2 tuning, steps 1–4.** Only if the gate run misses. Two targets have little headroom in the first reading: `~` p95 1983.65 ms against 2000 ms at 10 agents, and 10-agent memory 18.08 MB against 20 MB. The second pass for the model-list start-up cost (after 4.5) goes with the gate run.
- [ ] **4.10 step 2, one-hour run** with real Claude Code and OpenCode agents.
- [ ] **4.10 step 3, walk the screens** in iTerm2 at 80×24 and 160×48.
- [ ] **Before 4.1 items.** The author confirms the defaults, the starter prompts (P4-Q1) and the bench profile (P4-Q2); the code already uses them.
- [ ] **P4-Q7.** Re-check OpenCode's effort list on a build that lists variants.
- [ ] **Sign-off.** On the above: TD §12 row 4, roadmap row 4 `done <date>`, PRD row 4, the READMEs and this header to `Done`, then `docs: phase 4 acceptance and status`.

## Risks

| Risk | Response |
|---|---|
| A target misses at 100 agents (CPU or `~` freshness) | 4.2 tunes with numbers; the S0 fallback (pause unselected panes harder, read on settle only) stays available (TD §13) |
| `claude --help` changes its format | Recorded fixtures per pinned version; on a parse failure the field falls back to typing a name |
| The estimate: tuning at 100 is open-ended, and 4.1's bench rework (end-of-window memory, the reopen procedure, the helpers, the three-run script) is itself a small project | 4.1 and 4.2 go first so a miss shows early; if a target still misses after the S0 fallback, the author decides whether to move the target or the date |
| The bench's fake agents don't run long real-agent output (bench-results.md) | The realistic-output profile (P4-Q2) is replayed output; a one-hour run with real agents is a manual check in 4.10 |
| Binary size: 9,829,520 B today, about 170 KB under the target, with no release profile yet; this phase adds templates, help, the editor, pickers and the mouse | 4.1 sets `[profile.release]` first and records the size; every task that adds a dependency or a screen re-checks it; the P3-D8 fallback (hand-written code over a crate) applies |
| The keymap refactor touches about five input modules and the footers | 4.6 comes first, behind the registry; the existing key tests must pass unchanged with the default bindings |
| OpenCode 2.0's `models --verbose` differs from 1.18 (happened: 2.1.1 has no `--verbose`) | Spike S4 before 4.5; recorded fixtures for the pinned version; typing a name stays available |
| Replace-flow data: worktree rows, messages and cards that name removed agents | The rules in 4.4 and their tests; nothing is deleted on disk |
| 10-agent memory with the live view is within about 2 MB of 20 MB | The gate runs with the view on (Constraints); 4.2 tunes against it |
| Mouse capture breaks text selection in some terminals | The terminal's bypass modifier (`⌥`-drag in iTerm2) keeps its own selection, since P2-D25; noted in the key map |
| Templates' starter prompts are opinionated | Wording approved by the author (P4-Q1); everything stays editable |

## Open questions (decide when the phase starts)

| # | Question | Default if not decided |
|---|---|---|
| P4-Q1 | The wording of the template starter prompts (Generalist, Backend, Frontend, QA) | Drafted in 4.3; approved by the author |
| P4-Q2 | The bench's "realistic output" profile | Replayed recorded CLI output at its recorded pace, plus a spinner burst every 2 s |
| P4-Q4 | Right-click and drags while the agent pane is in Typing mode (the wheel and clicks are settled by P2-D25) | Ignored |
| P4-Q7 | OpenCode's effort list: 2.1.1 cannot list variants, so Effort is typed text for OpenCode (TD §5.2) | Typed text; re-check on a build that lists variants |
| P4-Q8 | `~` freshness has about 1–3 % headroom at 10 agents (structural: 1 s poll of whole-second activity plus a 1.5 s settle). Keep as is, lower `SETTLE_AFTER`, or change the target? | Keep as is for v1; it is a guessed (`~`) state and passes; re-measure in the full gate |
| P4-Q5 | The timestamped backup's name (TD §7 says only "timestamped") | `org.toml.<YYYYMMDD-HHMMSS>.bak` beside `org.toml` |
| P4-Q6 | The names of template agents (names are unique across the org) | `<team-slug>-backend`, `<team-slug>-qa` for Engineering Org; `backend`, `frontend`, `qa` for Product Team; `squad-N` for Agent Squad. These are the template defaults; the New role form's `<team>-<role>-N` (UI §4.4.1) applies only to agents made there |

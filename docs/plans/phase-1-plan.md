# DuoHive Phase 1 Implementation Plan — Org and agents

**Status:** Done 2026-10-04 · every task built and reviewed (results in [bench-results.md](bench-results.md)); decisions made during execution are folded into the tasks and decision log.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** You can define an org (teams, roles, agents), start and stop standing agents on Claude Code and OpenCode in `tmux -L duohive`, see their status live in a minimal org tree, edit the org through forms, and run a basic `duohive bench` at 10 agents.

**Companion plan:** [phase-0-plan.md](phase-0-plan.md) (Tasks 1–5: workspace and spikes S0–S2). Task numbers run across both plans. "Global Constraints" in this plan means [phase-0-plan.md § Global Constraints](phase-0-plan.md#global-constraints), which applies here in full.

**Architecture:** Seven crates with inward-only dependencies (TD §3.4). `duohive-core` holds the domain, the port traits and a sans-I/O engine. The engine is one task that owns all state, takes `Command`s and adapter `Input`s, and publishes `Event`s (TD §3.2–3.3). Adapters (tmux runtime, CLI adapters, SQLite/TOML store, hook listener) run as their own tasks and reach the core only through channels or the port traits. The TUI renders events and sends commands.

**Tech Stack:** Rust 1.99.0 (edition 2024), tokio 1.53, ratatui =0.30.2, crossterm =0.29.0, rusqlite 0.40 (bundled SQLite, WAL), toml 1.1, serde 1, serde_json 1, thiserror 2, tracing 0.1 + tracing-subscriber 0.3 + tracing-appender 0.2, shell-words 1.1, getrandom 0.4, libc 0.2, insta 1.49, tempfile 3. tmux 3.2 or newer. Versions checked on the crates.io index (2026-10-03): every pin is the latest stable release (patch updates come through `Cargo.lock` and Dependabot); the highest MSRVs are ratatui 0.30.2 (1.88), toml 1.1 and getrandom 0.4 (1.85), all below the 1.99 toolchain.

**Spec:** [docs/product/prd.md](../product/prd.md) (what), [docs/ux/ui-spec.md](../ux/ui-spec.md) (how it looks), [docs/design/technical-design.md](../design/technical-design.md) (how it is built), [docs/plans/v1-roadmap.md](v1-roadmap.md) (order). When they disagree, the PRD decides *what*, the UI/UX spec decides *how it looks*, and the TD decides *how it's built*. Section references below are "TD §4.2", "UI §2.1", "PRD Org model".

## Review Focus

1. **A hook must never disturb the agent's CLI.** When DuoHive is closed, the socket is stale, the token is missing, stdin is garbage or the data dir is unwritable, `duohive hook` still exits 0 within 1 s and prints nothing (Claude Code turns hook stdout into context). Worst case: stdin read deadline 300 ms (a CLI that leaves stdin open and silent), then socket connect 200 ms (a helper thread and `recv_timeout`) + write 150 ms, then the database fallback's SQLite `busy_timeout` 200 ms, then one append to `logs/hook.log` — 850 ms plus process start, opening the database and the log append, under 1 s. Tests: Task 22 `hook_never_fails_the_cli`, `hook_survives_silent_open_stdin`, `hook_survives_an_unreachable_socket`.
2. **A broken hand edit of `org.toml` is never overwritten.** The banner shows the line and error, the last good org stays in use, and in-app org edits are refused until the file parses, so the user's hand edit survives. Tests: Task 14 `edits_refused_while_org_file_invalid`, Task 16 `unknown_cli_reports_line`.
3. **Restarting DuoHive while agents run keeps the truth.** Live windows keep their state, hooks fired while DuoHive was closed are replayed in order before a dead pane applies the exit rules, and an agent whose window vanished (reboot) is stopped, not errored. Tests: Task 15 `reconnect_*`, Task 29 `e2e_restart_reconnects`.
4. **Task and prompt text reaches the CLI verbatim.** Quotes, newlines, `$(…)`, backticks, a leading `-`, and non-ASCII text are passed literally, never shell-interpreted or split. Tests: Task 19 `launch_script_passes_args_verbatim`, Tasks 20–21 `launch_snapshot_hostile_text`.
5. **Two DuoHive instances, or a stale `run.sock` after a crash.** A second instance refuses to start with a clear message instead of stealing hooks, and a stale socket file from a crash is replaced. Tests: Task 22 `second_instance_refused`, `stale_socket_replaced`.

---

## File Structure

```
Cargo.toml                      workspace, shared deps and lints
rust-toolchain.toml  rustfmt.toml  clippy.toml  LICENSE  .gitignore
Makefile  deny.toml  _typos.toml  .editorconfig   make check = the local gate (Task 1b)
README.md  CONTRIBUTING.md  CODE_OF_CONDUCT.md  SECURITY.md  CHANGELOG.md  AGENTS.md  CLAUDE.md
.github/ISSUE_TEMPLATE/  pull_request_template.md  dependabot.yml
.github/workflows/ci.yml        fmt, clippy, tests on macOS + Linux × tmux 3.2a + latest
.github/scripts/install-tmux.sh builds a given tmux version from source
.github/tmux-latest             one line: the tmux version CI calls "latest"
spikes/                         throwaway spike code (not in the workspace)
docs/plans/spike-report.md      spike findings (Tasks 3–5)
docs/plans/bench-results.md     bench numbers per phase (Task 30)
crates/duohive-core/src/
  lib.rs  names.rs  cli_kind.rs  org.rs  inherit.rs  validate.rs  status.rs
  settings.rs  time.rs  hook.rs  launch.rs  adapter.rs  ports.rs  command.rs  event.rs
  engine/mod.rs  engine/lifecycle.rs  engine/signals.rs  engine/timers.rs
  engine/org_edit.rs  engine/reconnect.rs  engine/driver.rs
  testkit.rs                    cfg(test) or feature "testkit": stub ports for tests in any crate
crates/duohive-store/src/       lib.rs  paths.rs  org_file.rs  settings_file.rs  db.rs  sqlite_store.rs
crates/duohive-tmux/src/        lib.rs  control.rs  quote.rs  version.rs  server.rs  connection.rs  activity.rs  runtime.rs
crates/duohive-tmux/tests/      runtime.rs  support/mod.rs
crates/duohive-cli/src/         lib.rs  detect.rs  hooks_json.rs  claude.rs  opencode.rs  fake.rs  assets/opencode-plugin.js
crates/duohive-cli/tests/fixtures/<cli>/<version>/{hooks,screens,replies}/
crates/duohive-tui/src/         lib.rs  theme.rs  format.rs  model.rs  tree.rs  render/{mod,header,org_pane,needs_you,side_pane,screens,dialog}.rs
                                input.rs  app.rs  forms/{mod,field,line_edit,agent,role,team,org,chooser}.rs
crates/duohive-mcp/src/lib.rs   empty until phase 3
crates/duohive/src/             main.rs  lib.rs  args.rs  logging.rs  startup.rs  system.rs  real_ports.rs
                                hook_cmd.rs  hook_listener.rs  fake_agent.rs  bench/{mod,measure}.rs
crates/duohive/tests/           dependency_rule.rs  hook.rs  system.rs  e2e.rs  bench.rs
```

## Decisions this plan makes where the specs are silent

Each is pinned here once; tasks refer to it. Each is also listed for the author in the hand-off.

| # | Decision |
|---|---|
| D1 | **Per-launch tokens are generated in phase 1.** Hooks authenticate with them (TD §4.1, §6.1), even though the roadmap lists tokens under phase 3. Phase 3 adds tool rights on top. Token = 32 random bytes, lowercase hex. |
| D2 | **Three transitions the TD §4.2 table lacked** (added to it 2026-10-05): `starting → waiting` (trust or login prompt at launch), `waiting → finished/idle` (the turn ends after the user declines), `finished/idle → working` (a new turn the user typed straight into the pane). A hook takes each where the CLI has one. As TD §4.2 allows for any transition, a settled screen read takes each only when the adapter has no hook for it (`needs_input`, `turn_end`, `busy` respectively), marked `~`: screen needs-input while starting → waiting, screen at-rest while waiting → finished/idle, screen busy while finished/idle → working. Exception for waiting: while an agent is waiting, a settled screen read applies whatever the hook coverage (at-rest → finished/idle, busy → working), because Claude Code hooks no answer to an approval prompt (spike S1: a declined prompt fires no hook at all, and an accepted long-running tool fires its next hook only when it ends). OpenCode reports the answer (`permission.replied`, spike S2), where the exception is harmless. Another exception: a start-up prompt (Claude Code's workspace trust, a login) appears before the CLI can fire any hook, so a needs-input screen while starting moves the agent to waiting whatever the adapter's hook coverage. A starting agent whose CLI can show a start-up prompt (the adapter's `startup_prompts()`, from Sn.8) is read once before the start timeout fires (Task 13), whatever its hook coverage, so a trust or login prompt shows as `~◆ waiting` rather than a timeout error. |
| D3 | **Prompt stacking format:** levels joined by a blank line, each under a Markdown heading: `# Org`, `# Team: <name>`, `# Role: <name>`, `# Agent: <name>`. Empty or whitespace-only levels are left out. |
| D4 | **Names:** agent names match `^[A-Za-z0-9][A-Za-z0-9_-]{0,39}$`, are unique across the org, and may not equal Duo's name. Team names are unique; role names are unique within their parent (team, or the root). Team and role names: 1–40 characters, trimmed, no control characters, no `›`. |
| D5 | **Duo's directory** is the user's home directory. Phase 1 starts Duo as an ordinary agent with the org prompt only; Duo's fixed prompt and MCP tools arrive in phase 3. |
| D6 | **Timers:** a start must be ready within 120 s (how this meets quiet: D20); stop grace 5 s; settle read 1.5 s after the last activity change; a starting agent's pre-timeout screen read 5 s before the start timeout (D2). |
| D7 | **Phase 1 layout:** header, org pane at the UI §3 widths (50 columns at ≥ 100 wide, 32 in the compact layout), and a side pane with the selected node's summary. No chat pane (phase 3), no live terminal (phase 2). A running agent's side pane shows how to attach: `tmux -L duohive attach -t duohive:<agent>`. Needs you is shown but is not a `tab` stop until phase 2. Phase-1 text adaptations (no Give work dialog until phase 2, no Duo tools until phase 3): a stopped agent's side pane reads `○ stopped · s to start` (UI §4.1.2: `space to give work`); the empty org reads `Your org is empty · n to add a team, role or agent` without `, or ask Duo to set one up`; an empty role reads `no agents yet · n to add one` without ` or ask Duo` (UI §4.12). Task 23. |
| D8 | **Phase 1 keys:** `↑↓` move, `s` start/stop (on an errored agent, stop: D16), `n` new, `e` edit, `d` delete, `v` last reply on/off, `o` open `org.toml` in `$EDITOR` (then reload), `q`/`ctrl+c` quit. `space` (Give work dialog), `?`, `m`, `f`, `w`, `z`, `,` (Settings), folding and the mouse arrive in phases 2–4. Give work exists as an engine command now. |
| D9 | **Agent windows vanished while DuoHive was closed** (reboot) → stopped, without an error. tmux windows with no session row are left running and logged, never killed. |
| D10 | **`org.toml` is rewritten from the model** on in-app edits, so hand-written comments are lost; the previous file is kept as `org.toml.bak`. Hand edits are re-read at start and after `o`. The last good org is copied to `<data_dir>/org.last-good.toml` after every save and every load the engine accepts (`Store::mark_last_good`: at start-up and on reload, never while `org.toml` is broken or leaves out a running agent). |
| D11 | **Role form "Agents [n]"** names new agents `<team-slug>-<role-slug>-<k>` for a role in a team, or `<role-slug>-<k>` for a role directly under Duo, so names stay unique across teams (`payments-frontend-1`, `growth-frontend-1`). A slug is the name lowercased, spaces → `-`, other characters dropped, runs of `-` collapsed to one, and leading and trailing `-` stripped (`+Growth  Ops` → `growth-ops`); when the name would pass D4's 40 characters, each slug is cut to its first 16 and stripped of a trailing `-` again. A slug that ends up empty is left out with its separator. `k` counts from 1, skipping names that exist. UI §4.4.1's New role mock was edited to match (`payments-frontend-1 …`). Team and role forms carry CLI, Directory, Model, Effort, Flags, Worktrees and Prompt (the PRD lets any field be a default; UI §4.4.1 draws only some). |
| D12 | **Roll-ups** list `● working`, `◆ waiting`, `◷ quiet`, `✕ errored` in that order, non-zero only, with a leading `~` when any counted agent's state was read from the screen. Errored counts as an attention state; UI §4.1's roll-up rule was edited to name it. |
| D13 | **Quiet time** shown on a row is the time since the silence began (last activity or hook), which is also the Needs you ordering key. Ages under a minute show `<1m`. |
| D14 | **Model and effort** are free-text fields with `inherit:` placeholders in phase 1; the picker, live model lists and "loading models…" arrive in phase 4. |
| D15 | **Desktop alerts and the `Alert` event** arrive with notifications in phase 2. |
| D16 | **`s` on an errored agent stops it** (→ stopped, leaves Needs you), matching the PRD ("until you give it work or stop it") and UI §4.12's `· s stop` hint; `s` never restarts an errored agent. Retrying is Give work (the engine's `GiveWork` relaunches now; the dialog arrives in phase 2). TD §4.2's start row was edited to drop `s` for errored. Tasks 23–24. |
| D17 | **Exit 127 at start** (the shell's "command not found") reads `<binary> not found (code 127)`, not "exited at start". On Duo's row the error line ends with ` · e Edit org` (Duo's CLI is set there); UI §4.12's Duo row was edited to `opencode not found (code 127) · e Edit org`. Tasks 12, 23. |
| D18 | **Directory overrides:** `$XDG_CONFIG_HOME` and `$XDG_DATA_HOME` are honoured (TD §7 names only the `~/.config` and `~/.local/share` defaults), and `DUOHIVE_CONFIG_DIR` / `DUOHIVE_DATA_DIR` override both, so tests and the bench never touch the user's files. |
| D19 | **The quiet threshold is at least 1 minute:** `quiet_after_minutes` below 1 (including 0) is treated as 1. The specs give only the 5-minute default. |
| D20 | **Start timeout vs quiet:** a starting agent errors at the start timeout (120 s, D6) whatever the quiet threshold. Quiet applies to a starting agent only when the threshold is below the timeout, which with whole minutes means only a 1-minute threshold; at 2 minutes or more the timeout comes first. |
| D21 | **Exit with no end hook:** for an adapter whose coverage has no `session_end` hook, a pane that dies with status 0 is stopped and a non-zero status is errored. Where the adapter has an end hook, status 0 without it is still errored, after a 500 ms wait for an end hook that races the pane's death (Task 12). TD §4.2's unexpected-exit row was edited to match. |
| D22 | **Replayed hooks are enriched:** hooks queued in `pending_hooks` while DuoHive was closed pass through the adapter's `enrich_hook` when replayed (for Claude Code, the transcript is read only when a `Stop` payload lacks `last_assistant_message`; spike S1), exactly as live ones do (Task 27's `EnrichingStore`). |
| D23 | **The state cell's word is padded to 9 cells** (`working` + 2), so `● working    2m` has four spaces before a two-character age. UI §2.1's "padded to 8 cells" was edited to 9. Task 23. |
| D24 | **A missing CLI goes through the state machine:** stopped (or errored) → starting (`Launch`) → errored (`LaunchFailed`, `<binary> not found · install it or change its CLI (e)`), as TD §4.2's "Launch fails: CLI missing" row says; nothing is sent to tmux. There is no stopped → errored edge. Tasks 9, 11. |
| D25 | **Unselected panes are silenced by the method spike S0 picks:** `refresh-client -f no-output` or paused flow control (`pause-after` plus `refresh-client -A '%<pane>:pause'`). S0 measures both (activity reports keep arriving, at what rate, and CPU at 100 windows) and records the choice in S0.5; `duohive-tmux` builds only that one, and the bench measures it. TD §4.1, §10 and §13 were edited from "paused by flow control" to "silenced (no-output or paused flow control, chosen by spike S0)". **S0.5 chose `no-output`, with `poll` activity** (one `list-windows` a second; no subscriptions). TD updated to match. Tasks 3, 19, 28. |
| D26 | **Directory probes run after the first frame** (TD §4.1 Start-up). `Engine::new` and `start_up` never call the `DirProbe`; until `Input::DirInventory` arrives (Task 27's detection task sends it after the first frame, with `CliInventory`), an agent's directory status is pending: no directory problem on its row, and nothing waits for it. A launch probes its own agent's directory when it runs (always after the first frame: it follows a key, a command or the inventory), and org edits probe the directories they touch. Tasks 8, 11, 14, 15, 27. |
| D27 | **An agent needs a directory to launch.** No directory resolved (none on the agent, role, team or org; Duo always has `~`, D5) refuses Start and Give work with `no directory set · e to fix`, which is also the row's problem line. This needs no probe, so it shows from the first frame. Tasks 8, 11. |
| D28 | **TD §8's "No agent CLI installed → full-screen message at first run" is deferred to phase 4's first-run setup.** In phase 1 a missing CLI shows only on the rows of agents that use it (D24), and a first run with no CLI found starts a blank org on Claude Code. |
| D29 | **Broken files at start-up:** an `org.toml` that doesn't parse starts the engine on `last_good_org()` (or `Org::blank(CliKind::Claude)` when there is none; edits are refused anyway, Review Focus 2) with `org_error` set, so the banner shows. An invalid `settings.toml` starts on `Settings::default()` with `settings_error` set and the banner ` ✕ settings.toml line N: <error> · using default settings`; while it is set, `UpdateSettings` applies in memory but is not saved, so the hand edit survives. Tasks 11, 23, 27. |
| D30 | **Launches before the first `CliInventory` are queued.** `StartAgent` and `GiveWork` launches that arrive before detection finishes wait, in order, in the engine and run when `CliInventory` arrives; the agent's row is unchanged until then. Detection takes at most one `--version` timeout (2 s) after the first frame, so the wait is short and nothing is lost. Task 11. |
| D31 | **tmux socket and session:** the server is `tmux -L duohive` unless `DUOHIVE_TMUX_SOCKET` names another socket (tests, the bench and the smoke run use their own, so they never touch the user's agents); the session is always `duohive`. The attach hint (D7) uses the socket in use. Tasks 16, 19, 23. |
| D32 | **Give work to a starting agent is refused** (`GiveWorkRefused { state: Starting }`, nothing sent), like working and waiting: a starting agent already has its launch task, and a paste sent before the CLI's input box exists is lost (spike S2). TD §5.1 and §6.2 were edited to list starting. Task 11. |

---

# Phase 1 — Org and agents

### Task 6: Core domain — names, CLI kinds, the org tree

**Files:**
- Create: `crates/duohive-core/src/{names.rs, cli_kind.rs, org.rs, time.rs, testkit.rs}`; Modify: `src/lib.rs` (declares `testkit` under `cfg(any(test, feature = "testkit"))` with the clippy allowance from Global Constraints)
- `testkit.rs` starts here with `example_org()` and grows in Tasks 8 (`FakeDirs`), 10 (fake ports, `StubAdapter`) and 15 (`TokioClock`).

**Interfaces:**
- Produces:

```rust
// names.rs
pub struct AgentName(String);           // D4 rule; Display, Ord, Hash, Serialize/Deserialize via parse
impl AgentName { pub fn parse(s: &str) -> Result<Self, NameError>; pub fn as_str(&self) -> &str; }
pub struct NodeName(String);            // team/role names, D4 rule
impl NodeName { pub fn parse(s: &str) -> Result<Self, NameError>; pub fn as_str(&self) -> &str; }
#[derive(thiserror::Error)] pub enum NameError { Empty, TooLong, BadChars(String) }
// cli_kind.rs  — TOML values "claude", "opencode"
pub enum CliKind { Claude, Opencode }
impl CliKind { pub const ALL: [CliKind; 2]; pub fn binary(self) -> &'static str;   // "claude" | "opencode"
               pub fn display(self) -> &'static str; }                            // "Claude Code" | "OpenCode"
impl FromStr for CliKind { type Err = UnknownCli; }   // UnknownCli Display: unknown CLI "claud"
// org.rs
pub struct Fields { pub cli: Option<CliKind>, pub flags: Option<String>, pub directory: Option<String>,
                    pub model: Option<String>, pub effort: Option<String>, pub worktree: Option<bool>,
                    pub prompt: Option<String> }
pub struct OrgDefaults { pub cli: CliKind, pub model: Option<String>, pub effort: Option<String>,
                         pub worktree: bool, pub prompt: Option<String> }
pub struct DuoConfig { pub name: AgentName, pub cli: CliKind }       // name default "Duo"
pub struct AgentDef { pub name: AgentName, pub fields: Fields }
pub struct Role { pub name: NodeName, pub defaults: Fields, pub agents: Vec<AgentDef> }
pub struct Team { pub name: NodeName, pub defaults: Fields, pub roles: Vec<Role> }
pub struct Org { pub defaults: OrgDefaults, pub duo: DuoConfig, pub teams: Vec<Team>, pub roles: Vec<Role> }
pub struct RoleKey { pub team: Option<NodeName>, pub role: NodeName }
pub enum NodeKey { Duo, Team(NodeName), Role(RoleKey), Agent(AgentName) }
pub struct AgentLoc<'a> { pub team: Option<&'a Team>, pub role: &'a Role, pub agent: &'a AgentDef }
impl Org {
    pub fn blank(cli: CliKind) -> Org;                       // Duo "Duo" on cli, no teams/roles
    pub fn find_agent(&self, name: &AgentName) -> Option<AgentLoc<'_>>;
    pub fn agents(&self) -> Vec<AgentLoc<'_>>;               // tree order: teams first, then root roles
    pub fn is_duo(&self, name: &AgentName) -> bool;
    pub fn breadcrumb(&self, key: &NodeKey) -> String;       // "Payments › Backend › billing-api"
}
// time.rs
pub struct Millis(pub u64);   // unix ms; Add<Duration>, Sub -> Duration (saturating)
```
All domain types derive `Clone, Debug, PartialEq, Eq, Serialize, Deserialize`; `CliKind` also derives `Copy, Hash` (it keys `Adapters` and is passed by value). `AgentLoc<'a>` is exempt: it is a borrowed view, never serialised, and derives only `Clone, Debug`.

- [x] **Step 1: Write the failing tests** (in each module):

```rust
#[test] fn agent_names_follow_d4() {
    for ok in ["auth-service", "squad-1", "Pay_QA", "a"] { assert!(AgentName::parse(ok).is_ok(), "{ok}"); }
    for bad in ["", "-x", "has space", "é", &"a".repeat(41), "a/b"] { assert!(AgentName::parse(bad).is_err(), "{bad}"); }
}
#[test] fn node_names_reject_breadcrumb_separator() { assert!(NodeName::parse("A › B").is_err()); assert!(NodeName::parse("  ").is_err()); }
#[test] fn unknown_cli_message() { assert_eq!("claud".parse::<CliKind>().unwrap_err().to_string(), "unknown CLI \"claud\""); }
#[test] fn cli_display_and_binary() { assert_eq!(CliKind::Opencode.display(), "OpenCode"); assert_eq!(CliKind::Claude.binary(), "claude"); }
#[test] fn find_agent_in_team_and_root_role() {
    let org = example_org();
    let a = org.find_agent(&n("auth-service")).unwrap();
    assert_eq!((a.team.map(|t| t.name.as_str()), a.role.name.as_str()), (Some("Payments"), "Backend"));
    let s = org.find_agent(&n("squad-1")).unwrap();
    assert_eq!((s.team.is_none(), s.role.name.as_str()), (true, "Generalist"));
    assert!(org.find_agent(&n("nobody")).is_none());
}
#[test] fn breadcrumb_formats() { assert_eq!(org.breadcrumb(&NodeKey::Agent(n("billing-api"))), "Payments › Backend › billing-api"); }
```
Put the PRD's example org (Payments with Backend [auth-service, billing-api] and QA [pay-qa]; root role Generalist [squad-1, squad-2, squad-3]) in `testkit::example_org()` (`cfg(test)` or feature `testkit`) — Tasks 7–15 and 23–25 reuse it.

- [x] **Step 2: Run them and see them fail.** `cargo test -p duohive-core --features testkit` → FAIL (types missing).
- [x] **Step 3: Implement the types.**
- [x] **Step 4: Run them and see them pass.** Same command → PASS.
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(core): org domain, names and CLI kinds"`

### Task 7: Core — inheritance and prompt stacking

**Files:**
- Create: `crates/duohive-core/src/inherit.rs`, `crates/duohive-core/src/validate.rs` (only `pub struct FieldError { pub path: String, pub message: String }` in this task)

**Interfaces:**
- Consumes: Task 6 types.
- Produces:

```rust
pub struct ResolvedAgent { pub name: AgentName, pub is_duo: bool, pub cli: CliKind, pub flags: Vec<String>,
    pub directory: Option<String>, pub model: Option<String>, pub effort: Option<String>,
    pub worktree: bool, pub prompt: String }
pub fn resolve(org: &Org, agent: &AgentName) -> Result<ResolvedAgent, FieldError>;   // FieldError: created in this task in validate.rs (Task 8 adds the rest of that module)
pub enum PromptLevel { Org, Team(NodeName), Role(NodeName), Agent(AgentName) }
pub fn stack_prompt(levels: &[(PromptLevel, Option<&str>)]) -> String;               // D3
pub struct InheritedValue<T> { pub value: T, pub from: String }                      // from = "Org" | team name | role name
pub struct Inherited { pub cli: InheritedValue<CliKind>, pub directory: Option<InheritedValue<String>>,
    pub model: Option<InheritedValue<String>>, pub effort: Option<InheritedValue<String>>,
    pub flags: Option<InheritedValue<String>>, pub worktree: InheritedValue<bool>,
    pub prompts: Vec<(String, Option<String>)> }      // ("Org", Some(..)), ("Payments", None), ("Backend", Some(..))
pub fn inherited_for(org: &Org, parent: &NodeKey) -> Inherited;   // what a child of `parent` inherits
pub fn expand_home(dir: &str, home: &Path) -> PathBuf;            // "~" and "~/x" only
```
Rules (PRD Inheritance, TD §5.1): prompt stacks org → team → role → agent; cli, flags, directory, model, effort, worktree take the nearest value up the tree, ending at `OrgDefaults` (which has no directory or flags). Duo resolves from `OrgDefaults` plus `duo.cli`, directory `~` (D5), prompt = org level only. Flags split with `shell_words::split`; a split error is a `FieldError` on `<path>.flags`.

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn prompts_stack_in_order_with_headings() {
    // org "Ask before you delete anything.", Payments none, Backend "API URLs use hyphens.", agent "Owns billing."
    assert_eq!(resolve(&org, &n("billing-api")).unwrap().prompt,
      "# Org\nAsk before you delete anything.\n\n# Role: Backend\nAPI URLs use hyphens.\n\n# Agent: billing-api\nOwns billing.");
}
#[test] fn nearest_value_wins() { /* role model "opus", team model "sonnet", org model "haiku" → "opus"; agent override → agent's */ }
#[test] fn org_defaults_are_the_last_resort() { /* nothing set → cli = org cli, worktree = org worktree, directory None */ }
#[test] fn root_role_skips_team_level() { /* squad-1: levels Org, Role: Generalist, Agent */ }
#[test] fn duo_resolves_from_org_defaults_and_home() { let r = resolve(&org, &n("Duo")).unwrap(); assert!(r.is_duo); assert_eq!(r.cli, org.duo.cli); assert_eq!(r.directory.as_deref(), Some("~")); }
#[test] fn flags_split_like_a_shell() { /* "--verbose --name 'a b'" → ["--verbose","--name","a b"] */ }
#[test] fn bad_flags_name_the_field() { /* "--x 'unterminated" → FieldError{ path: "team[Payments].role[Backend].agent[billing-api].flags", .. } */ }
#[test] fn inherited_for_role_in_team_reports_sources() { /* inherited_for(Role Backend) → cli from "Org", prompts [("Org",Some),("Payments",None),("Backend",Some)] */ }
#[test] fn expand_home_only_leading_tilde() { assert_eq!(expand_home("~/code", Path::new("/h")), PathBuf::from("/h/code")); assert_eq!(expand_home("/a/~b", h), PathBuf::from("/a/~b")); }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --features testkit inherit`
- [x] **Step 3: Implement** `resolve`, `stack_prompt`, `inherited_for`, `expand_home`.
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(core): inheritance and prompt stacking"`

### Task 8: Core — validation

**Files:**
- Modify: `crates/duohive-core/src/validate.rs` (created in Task 7 with `FieldError`)

**Interfaces:**
- Consumes: Tasks 6–7.
- Produces:

```rust
// FieldError { path, message } from Task 7; path like "team[Payments].role[Backend].agent[x].name"
pub fn validate(org: &Org) -> Vec<FieldError>;                    // blocking: the org is not used if non-empty
pub enum DirStatus { GitRepo, Found, Missing }
pub trait DirProbe: Send + Sync { fn probe(&self, dir: &Path) -> DirStatus; }
pub fn agent_problem(r: &ResolvedAgent, home: &Path, dir: Option<DirStatus>) -> Option<String>;   // non-blocking, per agent;
    // dir = the caller's probe of expand_home(directory); None = not probed yet (pending, D26): only "no directory set" can be reported
pub fn next_agent_names(org: &Org, team: Option<&NodeName>, role: &NodeName, count: u8) -> Vec<AgentName>;   // D11: <team-slug>-<role-slug>-<k>
```
Blocking rules (TD §7, D4): agent names unique including Duo's name — message `an agent named <x> already exists`; team names unique — `a team named <x> already exists`; role names unique within their parent — `a role named <x> already exists in <parent>`; flags parse (from `resolve`). Three levels and supported CLIs are enforced by the types and the file schema (Task 16). Non-blocking (TD §7 "Missing directory"): no directory resolved → `no directory set · e to fix` (D27; Duo resolves `~`); directory missing → `✕ <dir as written> not found · e to fix` (UI §4.12); directory pending (`dir` None) → no problem (D26).

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn duplicate_agent_names_rejected() { /* two "auth-service" → [FieldError{ path: "...agent[auth-service].name", message: "an agent named auth-service already exists" }] */ }
#[test] fn agent_may_not_take_duos_name() { /* agent "Duo" → error */ }
#[test] fn duplicate_team_and_sibling_role_names_rejected() {
    /* two teams "Payments" → [FieldError{ path: "team[Payments].name", message: "a team named Payments already exists" }];
       two roles "QA" in Payments → [FieldError{ path: "team[Payments].role[QA].name", message: "a role named QA already exists in Payments" }] */
}
#[test] fn same_role_name_in_different_teams_is_fine() { /* Backend in Payments and Growth (PRD example) → [] */ }
#[test] fn example_org_is_valid() { assert!(validate(&example_org()).is_empty()); }
#[test] fn missing_directory_invalidates_only_that_agent() { /* each agent probed with FakeDirs (~/code/billing Missing) → billing-api Some("✕ ~/code/billing not found · e to fix"); others None */ }
#[test] fn no_directory_reported() { assert_eq!(agent_problem(&r_without_dir, h, None), Some("no directory set · e to fix".into())); }
#[test] fn pending_directory_reports_no_problem() { /* D26: billing-api with directory "~/code/billing", dir None → None */ }
#[test] fn next_agent_names_skip_existing() {
    /* role "Frontend" in Payments, existing payments-frontend-1 → next_agent_names(org, Some(Payments), Frontend, 2) == [payments-frontend-2, payments-frontend-3];
       the same role name in Growth → [growth-frontend-1]; root role "Frontend" → [frontend-1] */
}
#[test] fn next_agent_names_fit_forty_characters() {
    /* team "Customer Success Operations", role "Platform Reliability Engineering" → ["customer-success-platform-reliabi-1"] (each slug cut to 16) */
}
#[test] fn slugs_strip_and_collapse_hyphens() {
    /* D11: team "+Growth  Ops" (starts with a symbol, two spaces), role "Front - End" → ["growth-ops-front-end-1"] */
}
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --features testkit validate`
- [x] **Step 3: Implement.** Add `pub struct FakeDirs(pub HashMap<PathBuf, DirStatus>, pub Arc<AtomicUsize>)` to `testkit`: the statuses (a path not in the map is `Found`) and a count of `probe` calls, which Task 15's `start_up_does_not_probe_directories` reads; `FakeDirs::new(map)` starts the count at 0. This is the one definition; Tasks 10–15 use it as is.
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(core): org validation and per-agent problems"`

### Task 9: Core — the status state machine

**Files:**
- Create: `crates/duohive-core/src/status.rs`

**Interfaces:**
- Produces:

```rust
pub enum AgentState { Stopped, Starting, Working, Waiting, Finished, Idle, Errored }
impl AgentState {
    pub fn is_running(self) -> bool;   // Starting | Working | Waiting | Finished | Idle
    pub fn is_at_rest(self) -> bool;   // Finished | Idle
    pub fn is_free(self) -> bool;      // Stopped | Finished | Idle  (PRD "Free")
    pub fn word(self) -> &'static str; // "stopped" "starting" "working" "waiting" "finished" "idle" "errored"
    pub fn glyph(self) -> char;        // ○ ◌ ● ◆ ✓ ◦ ✕
}
pub struct Status { pub state: AgentState, pub quiet: bool, pub quiet_since: Option<Millis>,
                    pub from_screen: bool, pub since: Millis, pub error: Option<String> }
pub enum Trigger { Launch, Ready { has_task: bool }, LaunchFailed, NeedsInput, Answered,
                   TurnEnded { is_duo: bool }, Viewed, NewWork, UnexpectedExit, Stop }
pub fn transition(from: AgentState, t: &Trigger) -> Option<AgentState>;   // None = not allowed
```
The table (TD §4.2 plus D2, marked †):

| From | Trigger | To |
|---|---|---|
| Stopped, Errored | Launch | Starting |
| Starting | Ready{has_task: true} | Working |
| Starting | Ready{has_task: false} | Idle |
| Starting | LaunchFailed | Errored |
| Working, Starting† | NeedsInput | Waiting |
| Waiting | Answered | Working |
| Working, Waiting† | TurnEnded{is_duo: false} | Finished |
| Working, Waiting† | TurnEnded{is_duo: true} | Idle |
| Finished | Viewed | Idle |
| Finished, Idle | NewWork | Working |
| Starting, Working, Waiting, Finished, Idle | UnexpectedExit | Errored |
| Starting, Working, Waiting, Finished, Idle, Errored | Stop | Stopped |

(D2's "user typed into the pane" case is `NewWork` from a hook; Task 12 maps it. A missing CLI is `Launch` then `LaunchFailed` (D24, Task 11); there is no stopped → errored edge.)

- [x] **Step 1: Write the failing tests.**

```rust
const TABLE: &[(AgentState, Trigger, AgentState)] = &[ /* every row above, expanded per From state */ ];
#[test] fn every_row_of_the_table_moves() { for (f, t, to) in TABLE { assert_eq!(transition(*f, t), Some(*to), "{f:?} {t:?}"); } }
#[test] fn nothing_else_moves() {
    for f in ALL_STATES { for t in ALL_TRIGGERS { if !TABLE.iter().any(|(a, b, _)| *a == f && b == &t) {
        assert_eq!(transition(f, &t), None, "{f:?} {t:?}"); } } }
}
#[test] fn duo_never_finishes() { assert_eq!(transition(Working, &TurnEnded { is_duo: true }), Some(Idle)); }
#[test] fn missing_cli_goes_through_starting() {
    /* D24 */ assert_eq!(transition(Stopped, &LaunchFailed), None);
    assert_eq!(transition(Stopped, &Launch), Some(Starting)); assert_eq!(transition(Starting, &LaunchFailed), Some(Errored));
}
#[test] fn words_and_glyphs_match_ui_2_1() { assert_eq!(Waiting.word(), "waiting"); assert_eq!(Errored.glyph(), '✕'); /* all seven */ }
#[test] fn free_means_stopped_finished_idle() {
    let free: Vec<_> = ALL_STATES.into_iter().filter(|s| s.is_free()).collect();
    assert_eq!(free, [Stopped, Finished, Idle]);
}
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core status`
- [x] **Step 3: Implement** `transition` as one `match` on `(from, trigger)`.
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(core): agent state machine"`

### Task 10: Core — ports, commands, events, settings and the testkit

**Files:**
- Create: `crates/duohive-core/src/{settings.rs, hook.rs, launch.rs, adapter.rs, ports.rs, command.rs, event.rs}`; Modify: `testkit.rs` (from Task 6)

**Interfaces:**
- Produces (all serialisable enums/structs; TD §3.2 "plain serialisable enums"):

```rust
// settings.rs  (TD §7; UI §4.10)
pub struct Settings { pub your_name: Option<String>, pub theme: Theme, pub quiet_after_minutes: u32,
    pub alerts: AlertSettings, pub show_last_reply: bool, pub reduce_motion: bool,
    pub keys: BTreeMap<String, String> }               // defaults: Auto, 5, {true,true}, true, false, {}
pub enum Theme { Auto, Dark, Light }   pub struct AlertSettings { pub needs_you: bool, pub finished: bool }
impl Settings { pub fn quiet_after(&self) -> Duration; }   // minutes, min 1
// hook.rs — one line on run.sock / one pending_hooks row
pub struct HookMessage { pub token: String, pub agent: String, pub event: String, pub payload: serde_json::Value }
// launch.rs
pub struct Token(pub String);   pub struct WindowId(pub String);       // "@12"
pub enum Key { Enter, Escape, CtrlC, CtrlD, Text(String) }
pub struct LaunchSpec { pub agent: ResolvedAgent, pub task: Option<String>, pub run_dir: PathBuf,
                        pub hook_exe: PathBuf, pub home: PathBuf }
pub struct LaunchFile { pub path: PathBuf, pub contents: String }      // always under run_dir, mode 0600
pub struct LaunchCommand { pub program: String, pub args: Vec<String>, pub env: Vec<(String, String)>,
                           pub cwd: PathBuf, pub files: Vec<LaunchFile> }
pub fn sh_quote(arg: &str) -> String;   // POSIX single quotes: ' → '\''; shared by duohive-tmux and duohive-cli
// adapter.rs
pub enum HookSignal { Ready, Busy, NeedsInput { prompt: Option<String> }, TurnEnded { last_reply: Option<String> }, SessionEnded }
pub enum ScreenState { Busy, NeedsInput, AtRest }
pub struct ScreenSignal { pub state: Option<ScreenState>, pub last_reply: Option<String> }
pub struct HookCoverage { pub ready: bool, pub busy: bool, pub needs_input: bool, pub turn_end: bool,
                         pub session_end: bool, pub last_reply: bool }   // the five Sn.3 events, plus where the reply comes from
impl HookCoverage { pub fn complete(&self) -> bool; }   // all five events hooked and last_reply from a hook
pub enum CliPresence { Missing, Found { version: String } }
#[derive(thiserror::Error)] pub enum LaunchError { #[error("{0}")] Invalid(String) }
pub trait CliAdapter: Send + Sync {
    fn kind(&self) -> CliKind;
    fn detect(&self) -> CliPresence;                              // may run `<binary> --version`; called off the core task, never by the engine
    fn tested_versions(&self) -> &'static [&'static str];         // major.minor, oldest first
    fn coverage(&self) -> HookCoverage;
    fn launch(&self, spec: &LaunchSpec) -> Result<LaunchCommand, LaunchError>;
    fn parse_hook(&self, event: &str, payload: &serde_json::Value) -> Option<HookSignal>;
    fn read_screen(&self, screen: &str) -> ScreenSignal;
    fn quit_keys(&self) -> Vec<Key>;
    fn submit_keys(&self) -> Vec<Key>;                             // after a paste
    fn startup_prompts(&self) -> bool;                             // Sn.8 lists a start-up prompt (trust, login, update notice): D2, Task 13
    fn enrich_hook(&self, _event: &str, _payload: &mut serde_json::Value) {}   // default no-op; may read files; called by the hook listener, never the engine
}
pub struct Adapters(HashMap<CliKind, Box<dyn CliAdapter>>);    // not Clone: shared as Arc<Adapters> by the engine, the hook listener and start_system
impl Adapters { pub fn new(list: Vec<Box<dyn CliAdapter>>) -> Self; pub fn get(&self, k: CliKind) -> Option<&dyn CliAdapter>;
                pub fn detect_all(&self) -> Vec<(CliKind, CliPresence)>; }   // each adapter's detect(), in CliKind::ALL order
pub fn untested_notice(binary: &str, version: &str, tested: &[&str]) -> Option<String>;
// "notice: claude 2.3 untested (tested up to 2.1)" when version's major.minor is not in `tested`
// ports.rs
pub trait Clock: Send { fn now(&self) -> Millis; }
pub trait Ids: Send { fn new_token(&mut self) -> Token; }
pub trait Runtime: Send {                   // fire-and-forget; results return as RuntimeEvent inputs
    fn start(&mut self, agent: &AgentName, cmd: LaunchCommand);   // creates <run_root>/<agent>/ (0700; run_root from TmuxConfig, Task 19),
                                                                   // writes cmd.files and launch.sh there, opens the window
    fn stop(&mut self, agent: &AgentName, quit: Vec<Key>, grace: Duration);
    fn kill(&mut self, agent: &AgentName, window: &WindowId);     // that window only, never a newer one for the same agent
    fn paste(&mut self, agent: &AgentName, text: String, submit: Vec<Key>);
    fn capture(&mut self, agent: &AgentName);
    fn inventory(&mut self);
}
pub enum RuntimeEvent { Started { agent: AgentName, window: WindowId }, StartFailed { agent: AgentName, reason: String },
    Activity { agent: AgentName, at_secs: u64 }, PaneDied { agent: AgentName, window: WindowId, status: Option<i32> },
    WindowGone { agent: AgentName, window: WindowId }, Screen { agent: AgentName, text: String },
    PasteFailed { agent: AgentName, reason: String }, Inventory(Vec<LiveWindow>), ConnectionLost { reason: String } }
pub struct LiveWindow { pub agent: AgentName, pub window: WindowId, pub dead: Option<Option<i32>>, pub activity_secs: u64 }
pub struct OrgFileError { pub line: Option<usize>, pub message: String }
pub struct SessionRow { pub agent: AgentName, pub token: Option<Token>, pub window: Option<WindowId>, pub status: Status,
    pub last_reply: Option<String>, pub task_at_launch: bool, pub saw_end_hook: bool, pub last_task: Option<String> }
#[derive(thiserror::Error)] #[error("{0}")] pub struct StoreError(pub String);
pub trait Store: Send {
    fn load_org(&mut self) -> Result<Option<Org>, OrgFileError>;   // None = no file yet
    fn last_good_org(&mut self) -> Option<Org>;
    fn save_org(&mut self, org: &Org) -> Result<(), StoreError>;   // also refreshes last-good (the engine saves only validated orgs)
    fn mark_last_good(&mut self, org: &Org) -> Result<(), StoreError>;   // D10: an org the engine accepted at start-up or on reload
    fn save_settings(&mut self, s: &Settings) -> Result<(), StoreError>;
    fn load_sessions(&mut self) -> Result<Vec<SessionRow>, StoreError>;
    fn save_session(&mut self, row: &SessionRow) -> Result<(), StoreError>;
    fn delete_session(&mut self, agent: &AgentName) -> Result<(), StoreError>;
    fn take_pending_hooks(&mut self) -> Result<Vec<HookMessage>, StoreError>;   // oldest first, deletes them
}
// command.rs  (TD §3.2, phase 1 subset)
pub enum Command { StartAgent { agent: AgentName }, StopAgent { agent: AgentName },
    GiveWork { agent: AgentName, task: String }, ViewAgent { agent: AgentName },
    EditOrg(OrgEdit), ReloadOrg, UpdateSettings(Settings), Shutdown }
pub struct TeamDraft { pub name: NodeName, pub defaults: Fields }
pub struct RoleDraft { pub name: NodeName, pub defaults: Fields }
pub struct AgentDraft { pub name: AgentName, pub fields: Fields }
pub enum OrgEdit { AddTeam(TeamDraft), AddRole { team: Option<NodeName>, role: RoleDraft, agents: u8 },
    AddAgent { role: RoleKey, agent: AgentDraft }, EditTeam { key: NodeName, draft: TeamDraft },
    EditRole { key: RoleKey, draft: RoleDraft }, EditAgent { key: AgentName, draft: AgentDraft },
    EditOrgDefaults { defaults: OrgDefaults, duo: DuoConfig }, Delete(NodeKey) }
// event.rs
pub struct AgentView { pub status: Status, pub last_reply: Option<String>, pub notice: Option<String>, pub problem: Option<String> }
pub struct NeedsYouItem { pub agent: AgentName, pub state: AgentState, pub quiet: bool, pub since: Millis, pub excerpt: Option<String> }
pub struct Snapshot { pub org: Org, pub agents: BTreeMap<AgentName, AgentView>, pub needs_you: Vec<NeedsYouItem>, pub org_error: Option<OrgFileError>,
                     pub settings_error: Option<OrgFileError>,   // D29: settings.toml invalid at start-up, defaults in use
                     pub reconnecting: u32 }   // running session rows awaiting the tmux inventory (UI §4.1.2 "Reconnecting to N agents…"); 0 = none
pub enum Event { Snapshot(Box<Snapshot>), OrgChanged { org: Box<Org>, problems: BTreeMap<AgentName, String> },
    AgentStatusChanged { agent: AgentName, status: Status }, LastReplyChanged { agent: AgentName, reply: Option<String> },
    AgentNoticeChanged { agent: AgentName, notice: Option<String> }, NeedsYouChanged(Vec<NeedsYouItem>),
    OrgFileError(Option<OrgFileError>), CommandRejected { reason: String },
    GiveWorkRefused { agent: AgentName, state: AgentState }, Reconnected }   // Reconnected: the start-up inventory has been applied
// testkit.rs (cfg(test) or feature "testkit")
pub fn example_org() -> Org; pub struct FakeClock(Arc<AtomicU64>);  /* advance(Duration) */
pub struct FakeIds; /* tokens "tok-1", "tok-2", … */
pub struct FakeDirs(pub HashMap<PathBuf, DirStatus>, pub Arc<AtomicUsize>);   // from Task 8: statuses (default Found) and the probe count
pub struct FakeRuntime { pub calls: Arc<Mutex<Vec<RuntimeCall>>> }   pub enum RuntimeCall { Start(AgentName, LaunchCommand), Stop(AgentName, Vec<Key>, Duration), Kill(AgentName, WindowId), Paste(AgentName, String, Vec<Key>), Capture(AgentName), Inventory }
pub struct FakeStore { /* in-memory; `fail_next_save` switch; `org_result` for load_org */ }
pub struct StubAdapter { pub kind: CliKind, pub coverage: HookCoverage, pub presence: CliPresence, pub startup_prompts: bool }   // StubAdapter::new(kind): complete coverage, presence Found { "2.1.0" }, tested ["2.1"], startup_prompts false; detect() returns presence
// StubAdapter::launch → program = binary(), args = ["--task", task] when a task is given;
// parse_hook maps event names "session-start" "busy" "needs-input" "turn-end" "session-end" 1:1,
// turn-end takes last_reply from payload["last_reply"]; read_screen maps a screen whose last non-empty line, trailing whitespace trimmed, is
// "BUSY" | "ASK" | "REST" to Busy | NeedsInput | AtRest; quit_keys = [Text("/exit"), Enter]; submit = [Enter].
```

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn commands_and_events_round_trip_json() { /* one value of every Command and Event variant → serde_json → equal */ }
#[test] fn settings_defaults() { let s = Settings::default(); assert_eq!(s.quiet_after_minutes, 5); assert_eq!(s.quiet_after(), Duration::from_secs(300)); assert!(s.show_last_reply); }
#[test] fn quiet_minimum_is_one_minute() { assert_eq!(Settings { quiet_after_minutes: 0, ..Default::default() }.quiet_after(), Duration::from_secs(60)); }
#[test] fn sh_quote_handles_hostile_text() { /* $(rm -rf ~), `x`, "-x", "a\nb", "é", "it's" round-trip through `sh -c 'printf %s <quoted>'` (std::process here is test-only) */ }
#[test] fn untested_notice_matches_ui_4_12() {
    assert_eq!(untested_notice("claude", "2.3.0", &["2.0", "2.1"]).as_deref(), Some("notice: claude 2.3 untested (tested up to 2.1)"));
    assert_eq!(untested_notice("claude", "2.1.288", &["2.1"]), None);
}
#[test] fn coverage_complete_needs_all_five_events_and_reply() {
    let all = HookCoverage { ready: true, busy: true, needs_input: true, turn_end: true, session_end: true, last_reply: true };
    assert!(all.complete());
    assert!(!HookCoverage { session_end: false, ..all }.complete());
    assert!(!HookCoverage { busy: false, ..all }.complete());
    assert!(!HookCoverage { last_reply: false, ..all }.complete());
}
#[test] fn stub_adapter_detect_reports_its_presence() {
    let a = StubAdapter { presence: CliPresence::Missing, ..StubAdapter::new(CliKind::Opencode) };
    assert_eq!(Adapters::new(vec![Box::new(a)]).detect_all(), vec![(CliKind::Opencode, CliPresence::Missing)]);
}
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --features testkit`
- [x] **Step 3: Implement the types and the testkit.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(core): ports, commands, events and testkit"`

### Task 11: Engine — lifecycle, launch path and Give work by state

**Files:**
- Create: `crates/duohive-core/src/engine/{mod.rs, lifecycle.rs}`

**Interfaces:**
- Consumes: Tasks 6–10.
- Produces:

```rust
pub struct EngineConfig { pub home: PathBuf, pub data_dir: PathBuf, pub hook_exe: PathBuf,
    pub start_timeout: Duration /* 120 s, D6 */, pub stop_grace: Duration /* 5 s */, pub settle_after: Duration /* 1.5 s */,
    pub end_hook_grace: Duration /* 500 ms, Task 12 */ }
pub struct Ports { pub runtime: Box<dyn Runtime>, pub store: Box<dyn Store>, pub clock: Box<dyn Clock>,
                   pub ids: Box<dyn Ids>, pub dirs: Arc<dyn DirProbe> }
pub enum Input { Command(Command), Runtime(RuntimeEvent), Hook(HookMessage), CliInventory(Vec<(CliKind, CliPresence)>),
    DirInventory(Vec<(PathBuf, DirStatus)>) /* D26: every resolved agent directory, probed after the first frame */, Tick }
pub struct Engine { /* org, org_error, settings, settings_error, agents: BTreeMap<AgentName, AgentRt>, cli: Option<HashMap<CliKind, CliPresence>> (None until the first CliInventory),
                      dirs: Option<HashMap<PathBuf, DirStatus>> (None = pending, D26), queued_launches (D30), cfg, ports, adapters */ }
impl Engine {
    pub fn new(org: Org, org_error: Option<OrgFileError>, settings: Settings, settings_error: Option<OrgFileError>,
               cfg: EngineConfig, ports: Ports, adapters: Arc<Adapters>) -> Engine;   // D29
    pub fn handle(&mut self, input: Input) -> Vec<Event>;
    pub fn snapshot(&self) -> Snapshot;
    pub fn next_deadline(&self) -> Option<Millis>;   // Task 13
}
```
`AgentRt` (private): `status`, `token`, `window` (the agent's current window: set by `Started`, cleared on a relaunch until the next `Started`; Task 12 ignores exits of any other window), `task_at_launch`, `stopping`, `saw_end_hook`, `last_signal_at`, `last_activity_secs`, `settle_due`, `start_deadline`, `last_reply`, `notice`, `last_task`, `restart_after_stop`, `waiting_prompt` (the `NeedsInput { prompt }` text while waiting, cleared when it leaves waiting; Task 13's Needs you excerpt), `pre_read_done` (Task 13's pre-timeout read was sent for this launch), `exit_pending` (Task 12's short wait for an end hook). Every status change saves a `SessionRow`; a save failure emits `CommandRejected { reason: "couldn't save state: <err>" }` and the engine carries on.

Behaviour:
- **Fresh launch** (`StartAgent` on stopped/errored with no task; `GiveWork` on stopped/errored with the task; TD §5.1): before the first `CliInventory`, queue it and do nothing else (D30; queued launches run in order when the inventory arrives); resolve (Task 7); probe the agent's directory through `ports.dirs` (D26) and `agent_problem(r, home, Some(status))` (Task 8) → refuse with `CommandRejected { reason: problem }` (D27 covers no directory); CLI `Missing` → `Launch` → starting, then at once `LaunchFailed` → errored `"<binary> not found · install it or change its CLI (e)"`, both status changes emitted and nothing sent to the runtime (D24); else, if the agent still has a recorded window (as after `ConnectionLost`, which errors agents without killing their windows), `runtime.kill(agent, old_window)` first and clear `window`; new token, `run_dir = data_dir/run/<agent>`, `adapter.launch(spec)`, append env `DUOHIVE_TOKEN`, `DUOHIVE_AGENT`, `DUOHIVE_DATA_DIR`, then `runtime.start`; state `Launch` → starting; `start_deadline = now + start_timeout`; `last_task = task`; the previous session's per-launch flags are reset: `saw_end_hook = false`, `waiting_prompt = None`, quiet cleared (`quiet_since = None`), `pre_read_done = false`, `exit_pending = None`, and `last_signal_at = now`. An untested version sets the notice (`AgentNoticeChanged`).
- **Give work on finished/idle:** `runtime.paste(agent, task, adapter.submit_keys())`, trigger `NewWork` → working, `from_screen = false`.
- **Give work on working/waiting:** `GiveWorkRefused { agent, state }`, nothing sent (TD §5.1; the "send as message" offer is phase 3's inbox).
- **StartAgent on a running agent:** `CommandRejected { reason: "<agent> is already running" }`.
- **StopAgent** (TD §5.5, stop keys measured 2026-10-04): running → `stopping = true`. A waiting agent gets no keys, since a key could answer its open prompt: `runtime.kill(agent, window)` at once. Any other running state → `runtime.stop(agent, [Escape] + adapter.quit_keys(), stop_grace)`: `Escape` first declines a prompt that opened meanwhile. The state changes when the window goes (Task 12). Errored → `Stop` → stopped, `runtime.kill(agent, window)` when it still has a window. Stopped → no-op.
- **ViewAgent:** finished → `Viewed` → idle; otherwise no-op.
- `StartFailed { reason }` → `LaunchFailed` → errored with `reason`; `last_task` kept for retry.
- `CliInventory` → record presence, then run the queued launches in order (D30). `DirInventory` → cache the statuses and emit `OrgChanged { org, problems }` with each agent's directory problem (D26).

- [x] **Step 1: Write the failing tests** in `engine/lifecycle.rs` with `FakeRuntime`, `FakeStore`, `FakeClock`, `FakeIds`, `FakeDirs`, `StubAdapter` (complete coverage) over `example_org()` with directories set. The harness sends `CliInventory` (every kind `Found`) right after `Engine::new` unless a test says otherwise.

```rust
#[test] fn start_stopped_agent_launches_with_no_task() { /* calls == [Start(pay-qa, cmd)], cmd.args has no "--task"; status Starting; session row saved with token "tok-1" */ }
#[test] fn launch_env_carries_token_agent_and_data_dir() { /* cmd.env ⊇ {DUOHIVE_TOKEN=tok-1, DUOHIVE_AGENT=pay-qa, DUOHIVE_DATA_DIR=<cfg.data_dir>} */ }
#[test] fn launch_spec_has_stacked_prompt_and_run_dir() { /* StubAdapter records the spec: prompt == resolve().prompt; run_dir == data_dir/run/pay-qa */ }
#[test] fn give_work_stopped_launches_with_task() { /* args contain ["--task", "Add retry cases"]; last_task kept */ }
#[test] fn give_work_errored_relaunches() { /* pay-qa errored "exited unexpectedly (code 1)"; GiveWork "Retry it" → calls == [Start(pay-qa, cmd)] with args ["--task", "Retry it"]; status Starting, error None */ }
#[test] fn give_work_finished_types_next_turn() { /* calls == [Paste(squad-2, "Now make…", [Enter])]; state Working */ }
#[test] fn give_work_idle_types_next_turn() { /* squad-1 idle; GiveWork "Add a test" → calls == [Paste(squad-1, "Add a test", [Enter])]; state Working */ }
#[test] fn give_work_working_is_refused() { /* events contain GiveWorkRefused{state: Working}; no runtime call */ }
#[test] fn give_work_waiting_is_refused() { /* billing-api waiting → events contain GiveWorkRefused{ agent: billing-api, state: Waiting }; calls empty; still Waiting */ }
#[test] fn start_running_agent_rejected() { /* CommandRejected "pay-qa is already running" */ }
#[test] fn missing_directory_refuses_start() { /* FakeDirs Missing → CommandRejected "✕ ~/code/pay-qa not found · e to fix"; no Start call */ }
#[test] fn missing_cli_errors_through_starting() { /* D24: CliInventory Opencode Missing; StartAgent on an opencode agent → events AgentStatusChanged Starting then AgentStatusChanged Errored "opencode not found · install it or change its CLI (e)"; no Start call */ }
#[test] fn launch_before_cli_inventory_is_queued() { /* D30: no CliInventory yet; StartAgent pay-qa → no events, no runtime call; CliInventory → calls == [Start(pay-qa, cmd)], Starting */ }
#[test] fn no_directory_refuses_start() { /* D27: agent with no directory anywhere up the tree → CommandRejected "no directory set · e to fix"; no Start call */ }
#[test] fn relaunch_resets_per_launch_flags() { /* pay-qa errored after a session-end hook, a waiting prompt and quiet; GiveWork "Retry" at t → saw_end_hook false, waiting_prompt None, quiet false, pre-timeout read due again, last_signal_at == t */ }
#[test] fn relaunch_kills_the_old_window_first() { /* pay-qa Started "@7", then errored by ConnectionLost (window still recorded); GiveWork "Retry" → calls == [Kill(pay-qa, "@7"), Start(pay-qa, cmd)] */ }
#[test] fn untested_version_sets_notice() { /* Found "2.3.0", tested ["2.1"] → AgentNoticeChanged "notice: claude 2.3 untested (tested up to 2.1)"; still launches */ }
#[test] fn start_failed_errors_and_keeps_task() { /* StartFailed "branch exists" → Errored with that reason; last_task == Some(task) */ }
#[test] fn dir_inventory_reports_missing_directories() { /* D26: before DirInventory, problems has no billing-api entry; DirInventory [(~/code/billing expanded, Missing)] → OrgChanged problems[billing-api] == "✕ ~/code/billing not found · e to fix" */ }
#[test] fn stop_running_sends_quit_keys_with_grace() { /* working agent → calls == [Stop(a, [Escape, Text("/exit"), Enter], 5s)] with the stub's quit keys; state unchanged until window goes */ }
#[test] fn stop_waiting_kills_without_keys() { /* waiting agent → calls == [Kill(a, window)]; no Stop, no keys */ }
#[test] fn stop_errored_goes_stopped() { /* Stopped; error cleared; Kill(agent, its window) called when it has one */ }
#[test] fn view_finished_goes_idle() { /* squad-2 finished; ViewAgent → AgentStatusChanged{ squad-2, Idle }; ViewAgent on idle squad-1 → no events */ }
#[test] fn store_failure_is_reported_not_fatal() { /* FakeStore fail_next_save → CommandRejected "couldn't save state: …"; status still Starting */ }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --features testkit engine::lifecycle`
- [x] **Step 3: Implement** `Engine::new`, `handle` dispatch and the lifecycle functions.
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(core): engine lifecycle and give work by state"`

### Task 12: Engine — hook signals, screen reads and exits

**Files:**
- Create: `crates/duohive-core/src/engine/signals.rs`

**Interfaces:**
- Consumes: `Engine`, `HookMessage`, `HookSignal`, `ScreenSignal`, `RuntimeEvent`.
- Produces: handling of `Input::Hook`, `RuntimeEvent::{Screen, PaneDied, WindowGone, Started, ConnectionLost}`.

Rules:
- **Authentication (TD §6.1):** a hook is accepted only if `message.agent` names a known agent and `message.token` equals its current token (constant-time compare). Anything else is dropped with a `tracing::warn!` and no event.
- **Hook → trigger** (any accepted hook sets `last_signal_at = now`, clears quiet, and sets `from_screen = false` on a move):
  - `Ready` in starting → `Ready { has_task: task_at_launch }`.
  - `Busy` in starting → `Ready { has_task: true }`; in waiting → `Answered`; in finished/idle → `NewWork` (D2); in working → no move.
  - `NeedsInput { prompt }` in working or starting → `NeedsInput`; `waiting_prompt = prompt`.
  - `TurnEnded { last_reply }` in working or waiting → `TurnEnded { is_duo }`; a `Some` reply that differs emits `LastReplyChanged`.
  - `SessionEnded` → `saw_end_hook = true`; no move.
- **Screen read** (TD §4.2, D2): `adapter.read_screen(text)`; a `ScreenState` moves the agent only when the adapter's `coverage()` has no hook for the trigger it implies, except while starting (needs-input) and while waiting (D2), where it applies whatever the coverage:
  - `Busy`: starting → `Ready { has_task: true }`, finished/idle → `NewWork` (D2) — when `busy` is false; waiting → `Answered` whatever the coverage (D2: no hook reports the user's answer);
  - `NeedsInput`: starting → `NeedsInput` whatever the coverage (D2: a trust or login prompt appears before any hook can fire); working → `NeedsInput` — when `needs_input` is false;
  - `AtRest`: working → `TurnEnded { is_duo }` — when `turn_end` is false; waiting → `TurnEnded { is_duo }` whatever the coverage (D2: a declined prompt fires no hook, spike S1); starting → `Ready { has_task: task_at_launch }` — when `ready` is false.
  A move made this way sets `from_screen = true`. `last_reply` from the screen is used only when `coverage.last_reply` is false.
- **Window identity:** `PaneDied { window, .. }` and `WindowGone { window, .. }` count only when `window` is the agent's current window (`AgentRt.window`). An event for any other window — the old one, killed after a crash and a relaunch, or the one a Duo CLI change stopped — or arriving while a relaunch waits for `Started` (window `None`) is ignored with a `tracing::debug!`, so it can never error or stop the new session.
- **Exits** (TD §5.5): on `PaneDied { status }` or `WindowGone` for the current window:
  1. `stopping` → `Stop` → stopped (if `restart_after_stop`, launch again with no task);
  2. else state starting → errored: status 127 → `"<binary> not found (code 127)"` (D17), otherwise `"<binary> exited at start (code N)"`;
  3. else status `Some(0)` and (`saw_end_hook`, or the adapter's `coverage.session_end` is false: D21) → `Stop` → stopped;
  4. else status `Some(0)`, `coverage.session_end` true and no end hook yet: the hook may still be in flight, so set `exit_pending = { window, status, due: now + end_hook_grace }` and wait (`next_deadline` includes `due`); a `SessionEnded` hook before `due` applies rule 3 at once, and a Tick at `due` applies rule 5;
  5. else `UnexpectedExit` → errored `"exited unexpectedly (code N)"` (or `"exited unexpectedly"` when there is no status).
  Then `runtime.kill(agent, window)` for a dead pane; stopped deletes the session row; errored keeps it with `window = None`.
- `Started { window }` records the window. `ConnectionLost { reason }` (sent only after the runtime failed to reattach to the tmux server, Task 19) → every running agent → errored `"tmux server exited"`; their `window` stays recorded, and a relaunch kills it first (Task 11).

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn hook_with_wrong_token_is_ignored() { /* token "nope" → no events, state Starting */ }
#[test] fn hook_for_unknown_agent_is_ignored() { /* HookMessage{ agent: "ghost", token: "tok-1" } → no events; every agent unchanged */ }
#[test] fn ready_without_task_goes_idle() { /* StartAgent pay-qa, hook session-start → AgentStatusChanged{ pay-qa, Idle }, from_screen false */ }
#[test] fn busy_while_starting_goes_working() { /* started with no task, hook busy → Working */ }
#[test] fn needs_input_then_busy_returns_to_working() { /* Working; needs-input → Waiting; busy → Working */ }
#[test] fn needs_input_while_starting_goes_waiting() { /* D2: Starting; hook needs-input → Waiting */ }
#[test] fn turn_end_after_declined_prompt_finishes() { /* D2: Waiting; hook turn-end → Finished (Duo → Idle) */ }
#[test] fn hook_busy_at_rest_goes_working() { /* D2: Finished; hook busy → Working; same from Idle */ }
#[test] fn screen_busy_at_rest_goes_working_without_busy_hook() {
    /* D2: coverage busy=false; squad-2 Finished; Screen "…BUSY" → Working, from_screen = true.
       coverage busy=true: same screen → no move */
}
#[test] fn screen_needs_input_while_starting_goes_waiting_without_hook() {
    /* D2: coverage needs_input=false; Starting; Screen "…ASK" → Waiting, from_screen = true */
}
#[test] fn screen_needs_input_while_starting_goes_waiting_even_with_hook() {
    /* D2: complete coverage (needs_input hooked); Starting; Screen "…ASK" (a trust prompt before any hook) → Waiting, from_screen = true;
       the same screen while Working → no move */
}
#[test] fn turn_end_finishes_and_sets_last_reply() { /* Finished; LastReplyChanged Some("Ready for review") */ }
#[test] fn duo_turn_end_goes_idle() { /* Duo Working; hook turn-end → AgentStatusChanged{ Duo, Idle }; never Finished */ }
#[test] fn screen_read_moves_only_unhooked_transitions_and_marks_tilde() {
    /* coverage needs_input=false: Screen "…ASK" while working → Waiting, from_screen=true;
       coverage turn_end=true: Screen "…REST" while working → no move;
       D2 waiting exception, complete coverage: Screen "…REST" while waiting → Finished (Duo → Idle), from_screen=true;
       Screen "…BUSY" while waiting → Working, from_screen=true */
}
#[test] fn hook_move_clears_tilde() { /* coverage needs_input=false; Screen "…ASK" → Waiting with from_screen true; hook busy → Working with from_screen false */ }
#[test] fn screen_last_reply_used_only_without_hook_reply() {
    /* screen "Done: 3 files\nREST" with ScreenSignal.last_reply Some("Done: 3 files"): coverage last_reply=false → LastReplyChanged Some("Done: 3 files");
       last_reply=true → no LastReplyChanged */
}
#[test] fn normal_exit_with_end_hook_and_zero_is_stopped() { /* session row deleted */ }
#[test] fn exit_without_end_hook_is_errored() { /* coverage session_end=true, no session-end hook seen, PaneDied Some(0) → no event; Tick at +500 ms → Errored "exited unexpectedly (code 0)" */ }
#[test] fn zero_exit_waits_briefly_for_the_end_hook() {
    /* coverage session_end=true; auth-service Working; PaneDied{ current window, Some(0) } at t → no event, next_deadline == t + 500 ms;
       session-end hook at t + 200 ms → Stopped, error None, row deleted; a Tick at t + 500 ms → no further event */
}
#[test] fn zero_exit_is_stopped_when_cli_has_no_end_hook() { /* D21: coverage session_end=false; PaneDied Some(0) → Stopped, row deleted; PaneDied Some(1) → Errored "exited unexpectedly (code 1)" */ }
#[test] fn nonzero_exit_is_errored_with_code() { /* "exited unexpectedly (code 1)"; Kill called; row kept */ }
#[test] fn exit_127_while_starting_says_not_found() { /* Starting on claude, PaneDied Some(127) → Errored "claude not found (code 127)" (D17) */ }
#[test] fn exit_while_starting_says_exited_at_start() { /* Duo on opencode, status 2 → "opencode exited at start (code 2)" */ }
#[test] fn window_gone_after_stop_is_stopped() { /* StopAgent auth-service (Working); WindowGone → Stopped, error None, session row deleted */ }
#[test] fn exits_of_an_old_window_are_ignored() {
    /* pay-qa Started "@3"; PaneDied{ "@3", Some(1) } → Errored, Kill(pay-qa, "@3"); GiveWork "Retry" → Starting (window None);
       WindowGone{ pay-qa, "@3" } → no events, still Starting; Started "@4"; WindowGone{ pay-qa, "@3" } and PaneDied{ pay-qa, "@3", Some(1) } → no events */
}
#[test] fn connection_lost_errors_running_agents() { /* "tmux server exited"; stopped agents unchanged */ }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --features testkit engine::signals`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(core): hook signals, screen reads and exit rules"`

### Task 13: Engine — quiet, settle reads, start timeout, Needs you

**Files:**
- Create: `crates/duohive-core/src/engine/timers.rs`

**Interfaces:**
- Produces: `Input::Tick` and `RuntimeEvent::Activity` handling; `Engine::next_deadline()`; `needs_you(&self) -> Vec<NeedsYouItem>`.

Rules:
- **Activity:** an `at_secs` different from the last one sets `last_activity_secs`, `last_signal_at = max(last_signal_at, at_secs*1000)`, clears quiet, and sets `settle_due = now + settle_after`.
- **Quiet** (TD §4.2): on Tick, a working or starting agent with `now − last_signal_at ≥ settings.quiet_after()` becomes quiet with `quiet_since = last_signal_at` (D13). Any state change clears quiet. Waiting never goes quiet.
- **Settle:** on Tick with `settle_due ≤ now`: if the agent is running and either the adapter's coverage is not complete or the agent is waiting (D2's waiting exception, whatever the coverage) → `runtime.capture(agent)` once; `settle_due = None`.
- **Pre-timeout read** (D2, D6): on Tick, a starting agent whose adapter's `startup_prompts()` is true (Sn.8: the CLI can show a trust, login or update prompt), whatever its hook coverage, and whose `start_deadline − 5 s ≤ now` gets one `runtime.capture` (once per launch), so a trust or login prompt can move it to `~◆ waiting` (Task 12) before the timeout. Claude Code's workspace-trust prompt appears before any hook, so complete coverage is no reason to skip it.
- **Start timeout:** on Tick, starting with `start_deadline ≤ now` → `runtime.kill(agent, window)` when its window is known, `LaunchFailed` → errored `"not ready within 120s"` (the configured value), whatever the quiet threshold (D20).
- **Needs you:** waiting, quiet and errored agents; `since` = `quiet_since` for quiet, else `status.since`; sorted by `since`, then name; excerpt = the waiting prompt (`AgentRt.waiting_prompt`, Task 11, set from `NeedsInput { prompt }`) or the error text. `NeedsYouChanged` is emitted only when the list changes.
- **`next_deadline`:** the earliest of every `settle_due`, `start_deadline`, pending pre-timeout read (`start_deadline − 5 s`), `exit_pending.due` (Task 12), and `last_signal_at + quiet_after` for non-quiet working/starting agents.

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn quiet_after_threshold_for_working() { /* advance 4m59s + Tick → not quiet; 5m0s → quiet, quiet_since = start of silence; in needs_you */ }
#[test] fn activity_clears_quiet() { /* auth-service quiet; Activity{ at_secs: new } → AgentStatusChanged with quiet false, quiet_since None; out of needs_you */ }
#[test] fn hook_clears_quiet() { /* auth-service quiet; hook busy (valid token) → quiet false; out of needs_you */ }
#[test] fn waiting_never_goes_quiet() { /* billing-api Waiting; advance 10 min + Tick → quiet false; needs_you item state Waiting, quiet false */ }
#[test] fn starting_goes_quiet_with_one_minute_threshold() { /* settings 1 min; at 60 s quiet, before the 120 s timeout */ }
#[test] fn settle_reads_screen_once_after_activity_stops() { /* incomplete coverage; Activity at t; Tick at t+1.4 s → no Capture; t+1.5 s → one Capture; later Tick → none */ }
#[test] fn complete_coverage_never_reads_screen() { /* complete coverage; Activity at t; Ticks at t+1.5 s and t+10 s → no Capture call */ }
#[test] fn starting_agent_screen_read_once_before_timeout() {
    /* StubAdapter with startup_prompts true and complete coverage; StartAgent at 0; Tick at 114 s → no Capture; 115 s → one Capture(pay-qa); 118 s → none;
       Screen "…ASK" → Waiting (from_screen); Tick at 120 s → no timeout error */
}
#[test] fn no_pre_read_without_startup_prompts() { /* startup_prompts false, coverage incomplete; StartAgent at 0; Ticks at 115 s and 118 s → no Capture */ }
#[test] fn start_timeout_errors_and_kills() { /* 120 s → Errored "not ready within 120s"; Kill(agent, its window) called */ }
#[test] fn needs_you_holds_waiting_quiet_errored_oldest_first() { /* billing-api waiting 4m, squad-3 quiet 6m → [squad-3, billing-api] */ }
#[test] fn needs_you_changed_only_on_change() { /* billing-api enters Waiting → one NeedsYouChanged; a Tick with nothing new → no NeedsYouChanged; Activity on auth-service (not quiet) → none */ }
#[test] fn next_deadline_is_earliest_timer() {
    /* now 0; auth-service Working with last_signal_at 0 (quiet at 300 s); pay-qa Starting at 10 s on an adapter with startup_prompts true (pre-read at 125 s, timeout 130 s);
       Activity on squad-1 at 20 s (settle at 21.5 s) → next_deadline == Millis(21_500); after that settle read → Millis(125_000) */
}
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --features testkit engine::timers`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(core): quiet, settle reads, start timeout and Needs you"`

### Task 14: Engine — org edits, reload and settings

**Files:**
- Create: `crates/duohive-core/src/engine/org_edit.rs`

**Interfaces:**
- Produces: handling of `Command::EditOrg`, `Command::ReloadOrg`, `Command::UpdateSettings`.

Rules (PRD Lifecycle, UI §4.4): apply the edit to a copy of the org, run `validate`, reject with the first `FieldError.message` if any, else `store.save_org`, replace the org, emit `OrgChanged { org, problems }` (problems from `agent_problem` for every agent: once `DirInventory` has arrived, with the cached statuses plus a `ports.dirs` probe of each directory this edit adds or changes; before it, pending, D26).
- Refused while `org_error` is set: `"org.toml has an error · fix it first (o)"` (Review Focus 2).
- Delete agent: only when stopped, else `"stop <agent> first"`. Delete team or role: only when every agent in it is stopped, else `"stop <agent> first"` naming the first running one. Duo can't be deleted: `"Duo can't be deleted"`.
- Rename (EditAgent with a new name): only while stopped (phase 1 has no worktrees), else `"rename only while stopped with no worktrees"`. Session rows move to the new name.
- Rename Duo (`EditOrgDefaults` whose `duo.name` differs): Duo is an ordinary agent (TD §6.4), so the same rule applies. Unless Duo is stopped (running or errored), the whole edit is refused with `"rename only while stopped with no worktrees"` and nothing is saved; when Duo is stopped, the org is saved and Duo's session row (if any), its token entry and its `AgentRt` move to the new name (keyed by name in `agents`), so hooks and `find_agent` use the new name from then on.
- Edits to a running agent are saved and apply on its next start (no restart).
- `AddRole { agents: n }` adds `next_agent_names` agents with empty fields.
- `EditOrgDefaults` with a new `duo.cli` while Duo runs: stop Duo with `restart_after_stop = true` (the TUI asked first).
- `ReloadOrg`: `store.load_org()`; `Err(e)` → keep the current org, set `org_error`, emit `OrgFileError(Some(e))`; `Ok(Some(org))` and valid → replace, clear error, `OrgChanged` + `OrgFileError(None)`; valid parse but `validate` errors → `OrgFileError(Some({ line: None, message: "<path>: <message>" }))`, keep the current org.
- `UpdateSettings(s)`: `store.save_settings`, replace settings (the quiet threshold takes effect on the next Tick). While `settings_error` is set (D29), replace settings without saving, so the broken hand edit survives.

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn add_agent_saves_org_and_emits_org_changed() { /* AddAgent{ Payments›QA, "pay-qa-2" } → FakeStore saved org has pay-qa-2 under QA; OrgChanged org.find_agent(pay-qa-2).is_some(); problems has no pay-qa-2 entry (FakeDirs Found) */ }
#[test] fn duplicate_agent_name_rejected() { /* "an agent named auth-service already exists"; store untouched */ }
#[test] fn rename_running_agent_rejected() { /* auth-service Working; EditAgent{ key: auth-service, name "auth-svc" } → CommandRejected "rename only while stopped with no worktrees"; org unchanged */ }
#[test] fn rename_stopped_agent_moves_session_row() { /* pay-qa stopped with a session row; rename to "pay-qa-old" → FakeStore sessions has pay-qa-old and no pay-qa */ }
#[test] fn rename_running_duo_rejected() { /* Duo Idle; EditOrgDefaults{ duo.name "Boss", same cli } → CommandRejected "rename only while stopped with no worktrees"; save_org not called; org.duo.name still "Duo" */ }
#[test] fn rename_stopped_duo_moves_its_runtime_state() {
    /* Duo stopped with a session row (as in rename_stopped_agent_moves_session_row); EditOrgDefaults{ duo.name "Boss" } → OrgChanged with org.duo.name "Boss";
       FakeStore sessions has Boss and no Duo; snapshot().agents has Boss (Stopped) and no Duo; StartAgent Boss launches with DUOHIVE_AGENT=Boss;
       Duo errored instead → CommandRejected "rename only while stopped with no worktrees" */
}
#[test] fn delete_running_agent_rejected() { /* "stop auth-service first" */ }
#[test] fn delete_role_with_running_agent_rejected() { /* auth-service stopped, billing-api Waiting; Delete(Role Payments›Backend) → CommandRejected "stop billing-api first"; org unchanged */ }
#[test] fn delete_stopped_agent_removes_session() { /* pay-qa stopped; Delete(Agent pay-qa) → OrgChanged without pay-qa; FakeStore delete_session(pay-qa) called */ }
#[test] fn cannot_delete_duo() { /* Delete(Agent Duo) → CommandRejected "Duo can't be deleted"; save_org not called */ }
#[test] fn edit_running_agent_applies_next_start() { /* edit model while working: no runtime call; after stop+start the launch spec has the new model */ }
#[test] fn add_role_with_agent_count_creates_named_agents() { /* AddRole{ team: Some(Payments), "Frontend", agents: 2 } → payments-frontend-1, payments-frontend-2 (D11); under the root → frontend-1 */ }
#[test] fn edits_refused_while_org_file_invalid() { /* org_error set → CommandRejected "org.toml has an error · fix it first (o)"; save_org not called */ }
#[test] fn reload_invalid_keeps_last_good_and_reports_line() { /* load_org Err{line 42, "unknown CLI \"claud\""} → OrgFileError event; org unchanged */ }
#[test] fn reload_valid_clears_banner() { /* org_error set; load_org Ok(Some(example_org())) → events OrgChanged and OrgFileError(None); snapshot().org_error None */ }
#[test] fn reload_with_duplicate_names_reports_path() { /* load_org Ok(org with two "squad-1") → OrgFileError(Some{ line: None, message: "role[Generalist].agent[squad-1].name: an agent named squad-1 already exists" }); org unchanged */ }
#[test] fn change_duo_cli_restarts_running_duo() { /* Duo Started "@2"; EditOrgDefaults with a new duo.cli → Stop(Duo); WindowGone{ Duo, "@2" } → Starting again on the new CLI */ }
#[test] fn update_settings_saves_and_changes_threshold() { /* UpdateSettings{ quiet_after_minutes: 2 } → FakeStore saved settings has 2; auth-service Working silent 2m + Tick → quiet */ }
#[test] fn update_settings_not_saved_while_settings_file_invalid() { /* D29: settings_error set; UpdateSettings{ quiet_after_minutes: 2 } → save_settings not called; auth-service Working silent 2m + Tick → quiet */ }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --features testkit engine::org_edit`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(core): org edits, reload and settings"`

### Task 15: Engine — reconnect at start-up and the driver loop

**Files:**
- Create: `crates/duohive-core/src/engine/{reconnect.rs, driver.rs}`

**Interfaces:**
- Produces:

```rust
impl Engine { pub fn start_up(&mut self) -> Vec<Event>; }   // loads sessions, asks runtime.inventory(), emits Snapshot
pub async fn run_core(engine: Engine, commands: tokio::sync::mpsc::Receiver<Command>,
    inputs: tokio::sync::mpsc::Receiver<Input>, events: tokio::sync::mpsc::UnboundedSender<Event>);
```
`start_up`: `load_sessions` → restore each row's status, token, window, last reply, with `last_signal_at = now` (the time DuoHive was closed is not silence it saw, so no restored agent goes quiet before a full threshold has passed) (stopped rows are deleted; rows for agents the org no longer names are held for the Inventory, below); then `runtime.inventory()`; emit `Snapshot` with `reconnecting` = the number of restored rows in a running state (the TUI shows UI §4.1.2's "Reconnecting to N agents…" while it is non-zero). `start_up` never calls the `DirProbe` (D26). On `RuntimeEvent::Inventory(windows)`, in this order (D9):
  0. orphan rows (agents `org.toml` no longer names, edited while DuoHive was closed): a gone or dead window → row deleted (a dead pane is left, D9); while a live one remains the loaded org is never accepted or marked last good: `last_good_org()` naming every live agent → start on it with `org_error` "<agent> is running · stop it, then press o to reload", else keep the loaded org, keep the rows held, `org_error` "<agent> is running but missing from org.toml · add it back, or stop it, then press o to reload" (a reload naming it adopts the row; a later inventory with its window gone drops it). `ConnectionLost` before the Inventory ends the reconnect: orphan rows deleted, queued then buffered hooks replayed, then the loss applied, then step 4.
  1. for each session row: live window → keep the restored state and set `last_activity_secs`; dead pane (`dead: Some(status)`) → keep the restored state for now; no window and state running → stopped, no error, row deleted. Windows with no row → `tracing::info!`, left alone.
  2. replay `store.take_pending_hooks()` in order through the normal hook path (in the binary the `Store` is Task 27's `EnrichingStore`, so replayed hooks are enriched like live ones: D22). Hooks for agents stopped in step 1 no longer match a token and are dropped.
  3. apply Task 12's exit rules to each dead pane, so a session-end replayed in step 2 counts (`saw_end_hook`) and a clean exit while DuoHive was closed is stopped, not errored.
  4. `NeedsYouChanged` and `Reconnected`.

  Live hooks wait for the replay (Review Focus 3): from `start_up` until step 2 ends, every `Input::Hook` the listener sends is buffered in the engine in arrival order, with no event; right after the replayed hooks, and before step 3, the buffered ones go through the normal hook path in that order. After `Reconnected` hooks are handled as they arrive.

`run_core`: `tokio::select!` over `commands.recv()`, `inputs.recv()`, and `sleep_until(next_deadline)` (→ `Tick`); sends every returned `Event`; ends on `Command::Shutdown` or when both channels close. The engine is the only owner of state (TD §3.3).

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn reconnect_restores_live_sessions() { /* row working 2m ago + live window → still Working, since unchanged */ }
#[test] fn reconnect_dead_pane_applies_exit_rules() { /* dead Some(Some(1)) → Errored "exited unexpectedly (code 1)" */ }
#[test] fn reconnect_missing_window_marks_stopped() { /* D9: no error, row deleted */ }
#[test] fn reconnect_replays_pending_hooks_in_order() { /* busy then turn-end queued → Finished */ }
#[test] fn live_hooks_wait_for_the_replay() {
    /* row squad-1 Idle with a live window; pending_hooks [busy]; after start_up, a live turn-end for squad-1 arrives before Inventory → no events;
       Inventory → the replayed busy (Working) and then the live turn-end (Finished) apply in that order: squad-1 ends Finished */
}
#[test] fn reconnect_clean_exit_while_closed_is_stopped() {
    /* coverage session_end=true; row auth-service Working; pending turn-end then session-end; Inventory dead Some(Some(0)) →
       Stopped, error None, row deleted (hooks replayed before the exit rules) */
}
#[test] fn reconnect_does_not_mark_quiet_immediately() {
    /* row auth-service Working whose last hook was 10 min before the restart; live window; first Tick after Inventory → not quiet;
       advance quiet_after (5 min) + Tick → quiet */
}
#[test] fn start_up_does_not_probe_directories() { /* D26: FakeDirs::new(map); Engine::new + start_up + Inventory → its probe count (the Arc<AtomicUsize>) is 0; problems pending (no "not found" lines) */ }
#[test] fn reconnect_keeps_errored_agents_errored() { /* errored row, no window → still Errored, in needs_you */ }
#[test] fn session_for_deleted_agent_is_dropped() { /* row for "gone-agent" (not in the org) → FakeStore delete_session(gone-agent) called; Snapshot.agents has no gone-agent */ }
#[test] fn reconnecting_count_then_reconnected() { /* rows: auth-service Working, squad-2 Finished, pay-qa Stopped → Snapshot.reconnecting == 2; after Inventory → events end with Reconnected */ }
#[test] fn unknown_windows_are_left_alone() { /* no Kill call */ }
#[tokio::test(start_paused = true)] async fn driver_ticks_at_next_deadline() { /* quiet fires without any input after 5 min of paused time */ }
#[tokio::test] async fn driver_stops_on_shutdown() { /* spawn run_core; send Command::Shutdown → the JoinHandle completes within 1 s; FakeRuntime got no Stop or Kill call (agents keep running) */ }
```
For the paused-time test, the driver's `FakeClock` follows `tokio::time::Instant` (add `TokioClock` to the testkit). `start_paused` needs tokio's `test-util` feature: add `tokio = { workspace = true, features = ["macros", "rt", "test-util"] }` to `duohive-core`'s `[dev-dependencies]`.
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --features testkit -- engine::reconnect engine::driver`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.** Then the core gate: `cargo test -p duohive-core --features testkit && cargo clippy -p duohive-core --all-targets --features testkit -- -D warnings`.
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(core): reconnect at start-up and the core driver loop"`

### Task 16: Store — paths, `org.toml` and `settings.toml`

**Files:**
- Create: `crates/duohive-store/src/{paths.rs, org_file.rs, settings_file.rs}`; test fixtures `crates/duohive-store/tests/fixtures/{org-example.toml, org-bad-cli.toml, org-nested-agent.toml}`
- Modify: `docs/design/technical-design.md` §7 (add the `org.toml` schema below)

**Interfaces:**
- Produces:

```rust
pub struct Paths { pub config_dir: PathBuf, pub data_dir: PathBuf, pub tmux_socket: String }
impl Paths {
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Paths, PathsError>;   // Global Constraints; tmux_socket from DUOHIVE_TMUX_SOCKET else "duohive" (D31)
    pub fn from_env() -> Result<Paths, PathsError>;   // collects std::env::vars() and delegates to from_vars
    pub fn org_toml(&self) -> PathBuf; pub fn org_backup(&self) -> PathBuf;      // org.toml.bak
    pub fn org_last_good(&self) -> PathBuf;                                       // data_dir/org.last-good.toml
    pub fn settings_toml(&self) -> PathBuf; pub fn state_db(&self) -> PathBuf;
    pub fn run_sock(&self) -> PathBuf; pub fn run_dir(&self, agent: &AgentName) -> PathBuf; pub fn logs_dir(&self) -> PathBuf;
}
pub fn load_org(path: &Path) -> Result<Option<Org>, OrgFileError>;
pub fn save_org(paths: &Paths, org: &Org) -> Result<(), StoreError>;   // atomic, keeps .bak, refreshes last-good (the engine saves only validated orgs; loads never copy, D10)
pub fn load_settings(path: &Path) -> Result<Settings, OrgFileError>;   // missing file → defaults
pub fn save_settings(path: &Path, s: &Settings) -> Result<(), StoreError>;
```
`org.toml` schema (D10; nested arrays of tables, `deny_unknown_fields` everywhere so a fourth level fails to parse):

```toml
[org]                       # OrgDefaults
cli = "claude"              # required; "claude" | "opencode"
model = "sonnet"            # optional
effort = "high"             # optional
worktree = false            # default false
prompt = "Ask before you delete anything."

[duo]
name = "Duo"
cli = "opencode"

[[team]]
name = "Payments"
# optional defaults: cli, flags, directory, model, effort, worktree, prompt
[[team.role]]
name = "Backend"
prompt = "API URLs use hyphens."
[[team.role.agent]]
name = "auth-service"
directory = "~/code/auth"

[[role]]                    # roles directly under you
name = "Generalist"
[[role.agent]]
name = "squad-1"
```
Atomic save: write `org.toml.tmp` in the same directory, `fsync`, copy the current file to `org.toml.bak`, `rename` the tmp over `org.toml`, then copy to `org.last-good.toml`. Parse errors map the `toml` error span to a 1-based line; the message is the error's own text (for a bad CLI, `unknown CLI "claud"` from `CliKind::from_str`).

- [x] **Step 1: Write the failing tests** (`tests/org_file.rs`, `tests/settings_file.rs`, unit tests in `paths.rs`).

```rust
#[test] fn parses_prd_example_org() { assert_eq!(load_org(&fixture("org-example.toml")).unwrap().unwrap(), example_org()); }
#[test] fn save_then_load_round_trips() { let p = temp_paths(); save_org(&p, &example_org()).unwrap(); assert_eq!(load_org(&p.org_toml()).unwrap(), Some(example_org())); }
#[test] fn unknown_cli_reports_line() { assert_eq!(load_org(&fixture("org-bad-cli.toml")).unwrap_err(), OrgFileError { line: Some(42), message: "unknown CLI \"claud\"".into() }); }
#[test] fn agent_under_agent_is_rejected() { /* org-nested-agent.toml → Err, line of the [[team.role.agent.agent]] header */ }
#[test] fn missing_org_file_is_none() { assert_eq!(load_org(&tmp.path().join("org.toml")).unwrap(), None); }
#[test] fn save_keeps_previous_version_as_backup() { /* save A, save B → org.toml == B, org.toml.bak == A */ }
#[test] fn save_refreshes_last_good() { /* save example_org → load_org(&p.org_last_good()) == Some(example_org()) */ }
#[test] fn missing_settings_file_gives_defaults() { assert_eq!(load_settings(&missing).unwrap().quiet_after_minutes, 5); }
#[test] fn settings_round_trip() { let s = Settings { quiet_after_minutes: 7, theme: Theme::Light, ..Default::default() }; save_settings(&path, &s).unwrap(); assert_eq!(load_settings(&path).unwrap(), s); }
#[test] fn paths_follow_xdg_then_home() { /* Paths::from_vars only, never std::env::set_var: {HOME: /h, XDG_CONFIG_HOME: /x} → /x/duohive; {HOME: /h} → /h/.config/duohive; DUOHIVE_DATA_DIR wins; DUOHIVE_TMUX_SOCKET sets tmux_socket, else "duohive" */ }
```
(`org-bad-cli.toml` is a valid example org padded so that `cli = "claud"` sits on line 42.)
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-store`
- [x] **Step 3: Implement**, with serde "file" structs mapped to the core types (core types stay format-free). Add the schema above to TD §7.
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(store): org.toml and settings.toml with atomic saves"`

### Task 17: Store — `state.db` and the `Store` implementation

**Files:**
- Create: `crates/duohive-store/src/{db.rs, sqlite_store.rs}`

**Interfaces:**
- Produces:

```rust
pub fn open_db(path: &Path, busy_timeout: Duration) -> Result<rusqlite::Connection, StoreError>;   // WAL, migrations; the app passes 1000 ms, `duohive hook` 200 ms (Review Focus 1)
pub fn insert_pending_hook(conn: &rusqlite::Connection, m: &HookMessage, received_at: Millis) -> Result<(), StoreError>;  // used by `duohive hook`
pub struct SqliteStore { /* paths, conn */ }
impl SqliteStore { pub fn open(paths: &Paths) -> Result<SqliteStore, StoreError>; }
impl duohive_core::ports::Store for SqliteStore {}
```
Migration 1 (append-only list `MIGRATIONS: &[&str]`, applied in a transaction; `PRAGMA user_version` = count):

```sql
CREATE TABLE sessions (agent TEXT PRIMARY KEY, token TEXT, window TEXT, state TEXT NOT NULL,
  quiet INTEGER NOT NULL, quiet_since INTEGER, from_screen INTEGER NOT NULL, since INTEGER NOT NULL,
  error TEXT, last_reply TEXT, task_at_launch INTEGER NOT NULL, saw_end_hook INTEGER NOT NULL, last_task TEXT);
CREATE TABLE pending_hooks (id INTEGER PRIMARY KEY AUTOINCREMENT, token TEXT NOT NULL, agent TEXT NOT NULL,
  event TEXT NOT NULL, payload TEXT NOT NULL, received_at INTEGER NOT NULL);
```
The database file and its directory are created owner-only (0600 / 0700); tokens live here (TD §6.1).

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn migrations_apply_once_and_set_user_version() { /* open twice → user_version 1, no error */ }
#[test] fn wal_mode_enabled() { /* PRAGMA journal_mode == "wal" */ }
#[test] fn session_round_trip() { /* save_session(row: auth-service Working, quiet, from_screen, token "tok-1", last_reply Some("hi")) → load_sessions() == [row] */ }
#[test] fn save_session_upserts() { /* save auth-service Working, then Finished → load_sessions().len() == 1, state Finished */ }
#[test] fn pending_hooks_are_fifo_and_taken_once() { /* insert busy, turn-end, session-end → take_pending_hooks() events == [busy, turn-end, session-end]; a second take → [] */ }
#[test] fn a_second_connection_can_insert_while_store_is_open() { /* insert_pending_hook from another Connection → take_pending_hooks sees it */ }
#[test] fn db_file_is_owner_only() { /* mode & 0o077 == 0 */ }
#[test] fn store_load_org_uses_org_file_and_last_good() { /* save_org(example_org) → load_org() == Ok(Some(example_org())); then write "cli = \"claud\"" garbage to org.toml → load_org() Err with a line; last_good_org() == Some(example_org()) */ }
#[test] fn mark_last_good_writes_last_good_and_load_does_not() { /* hand-write a valid org.toml → load_org() leaves org.last-good.toml absent; mark_last_good(&org) → last_good_org() == Some(org) (D10) */ }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-store`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(store): state.db sessions and pending hooks"`

### Task 18: tmux — control-mode protocol, quoting and version

**Files:**
- Create: `crates/duohive-tmux/src/{control.rs, quote.rs, version.rs}`

**Interfaces:**
- Consumes: spike report **S0.4 Formats** (the exact notification lines).
- Produces:

```rust
pub enum ControlLine { Begin { cmd: u64 }, End { cmd: u64 }, Error { cmd: u64 }, Body(String),
    WindowClose { window: String },   // from "%unlinked-window-close @N" (what tmux 3.2a and 3.7c send, S0.4) or "%window-close @N"
    Exit { reason: Option<String> }, Other(String) }
pub struct ControlParser { /* tracks whether inside a %begin block */ }
impl ControlParser { pub fn feed(&mut self, line: &str) -> ControlLine; }
pub fn tmux_quote(arg: &str) -> Result<String, QuoteError>;   // single-quoted for a control-mode command line; '\n' → Err(QuoteError::Newline)
#[derive(thiserror::Error)] pub enum QuoteError { #[error("a tmux command argument can't contain a newline")] Newline }
pub struct TmuxVersion { pub major: u32, pub minor: u32, pub raw: String }
pub fn parse_version(out: &str) -> Option<TmuxVersion>;   // "tmux 3.2a" "tmux 3.7c" "tmux next-3.6" "tmux master"→None
impl TmuxVersion { pub fn supported(&self) -> bool; }     // ≥ 3.2
```
- [x] **Step 1: Write the failing tests** using the S0.4 lines verbatim.

```rust
#[test] fn parses_begin_body_end() {
    let mut p = ControlParser::default();
    assert_eq!(p.feed("%begin 1767225600 12 1"), ControlLine::Begin { cmd: 12 });
    assert_eq!(p.feed("@3"), ControlLine::Body("@3".into()));
    assert_eq!(p.feed("%end 1767225600 12 1"), ControlLine::End { cmd: 12 });
}
#[test] fn parses_error_block() { /* "%begin 1 13 1", "can't find window: @99", "%error 1 13 1" → Begin{13}, Body("can't find window: @99"), Error{13} */ }
#[test] fn lines_inside_a_block_are_body_even_if_they_start_with_percent() { /* inside %begin…%end, "%output %1 x" → Body("%output %1 x"); after %end the same line → Other */ }
#[test] fn parses_window_close_and_exit() { /* "%unlinked-window-close @4" → WindowClose{ window: "@4" }; "%window-close @4" → WindowClose{ window: "@4" }; "%exit" → Exit{ reason: None }; "%exit server exited" → Exit{ reason: Some("server exited") } */ }
#[test] fn unknown_notifications_are_other() { assert_eq!(ControlParser::default().feed("%session-renamed $1 x"), ControlLine::Other("%session-renamed $1 x".into())); }
#[test] fn tmux_quote_escapes_single_quotes() { assert_eq!(tmux_quote("it's").unwrap(), r"'it'\''s'"); }
#[test] fn tmux_quote_rejects_newlines() { assert_eq!(tmux_quote("a\nb"), Err(QuoteError::Newline)); }
#[test] fn versions_parse() { assert_eq!(parse_version("tmux 3.2a").map(|v| (v.major, v.minor)), Some((3, 2))); assert!(!parse_version("tmux 3.1c").unwrap().supported()); assert_eq!(parse_version("tmux next-3.6").map(|v| v.minor), Some(6)); }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-tmux --lib`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(tmux): control-mode parser, quoting and version check"`

### Task 19: tmux — the runtime

**Files:**
- Create: `crates/duohive-tmux/src/{server.rs, connection.rs, activity.rs, runtime.rs}`; tests `crates/duohive-tmux/tests/{runtime.rs, support/mod.rs}`

**Interfaces:**
- Consumes: Task 18; `Runtime`, `RuntimeEvent`, `LaunchCommand`, `Key`, `LiveWindow`, `sh_quote` (Task 10); spike report **S0.5 Decision** (`poll` with `no-output` silencing) and **S0.4** lines.
- Produces:

```rust
pub struct TmuxConfig { pub tmux_bin: PathBuf, pub socket: String /* D31 */, pub session: String /* "duohive" */, pub buffer_name: String /* "duohive-<pid>" */,
                        pub run_root: PathBuf /* <data_dir>/run: start creates run_root/<agent>/ and its launch.sh */ }
pub fn tmux_version(cfg: &TmuxConfig) -> Result<Option<TmuxVersion>, TmuxError>;   // None = tmux not found
pub struct TmuxRuntime { /* sends requests to the connection task */ }
impl TmuxRuntime {
    pub async fn connect(cfg: TmuxConfig, events: tokio::sync::mpsc::Sender<duohive_core::engine::Input>) -> Result<TmuxRuntime, TmuxError>;
}
impl duohive_core::ports::Runtime for TmuxRuntime {}
```
Mechanics:
- **Server set-up** (`server.rs`, idempotent): if `has-session -t =duohive` fails, `new-session -d -s duohive -n duohive-home -x 160 -y 48 'tail -f /dev/null'` (the keeper window keeps the session when no agent runs). Then `set-option -g remain-on-exit on`, `set-option -g default-shell /bin/sh` (launch strings are always parsed by POSIX sh, Review Focus 4), `set-option -g default-size 160x48`. Every tmux invocation passes `-f /dev/null`: DuoHive's server never loads the user's tmux config, so their hooks and options can't interfere (a user attaching via the hint gets stock key bindings). One-shot tmux processes are bounded (5 s; `tmux -V` 2 s).
- **Connection** (`connection.rs`): one child `tmux -L <socket> -C attach-session -t =duohive`; commands written one per line; replies matched FIFO to the `%begin/%end` blocks of commands this client sent (flags 1; flags 0 blocks, such as the attach's own and hook commands, are dropped); a command unanswered after 5 s fails with a timeout and, since FIFO matching can't be trusted after that, the client is dropped and reattached; notifications routed to `activity.rs`. On start: the S0.5 silencing (D25), `refresh-client -f no-output,ignore-size` (it covers windows created later), then the size command `refresh-client -C 160x48` (tmux 3.2a otherwise sizes windows from the attaching control client, 80x24), then the activity poll. On EOF or `%exit`, it first tries to reattach: if `tmux -L <socket> has-session -t =duohive` succeeds, it starts a new control client, re-sends the silencing and activity commands, and diffs `list-windows` against its window table, emitting `PaneDied` / `WindowGone` for windows that died or vanished meanwhile; no `ConnectionLost`. Only when the server is gone or the reattach fails → `ConnectionLost { reason }`.
- **Activity** (`activity.rs`), the S0.5 `poll`: `list-windows -t =duohive -F "#{window_id}\t#{window_activity}\t#{pane_dead}\t#{pane_dead_status}"` once a second, diffed against the previous reply. Tabs keep a live pane's empty `pane_dead_status` an empty last field (S0.5). The format is double-quoted on purpose: tmux turns `\t` into a real tab only inside double quotes, and inside single quotes it passes a literal backslash-t (checked on 3.2a and 3.7c). It emits `RuntimeEvent::Activity` on a changed activity time and `PaneDied { agent, window, status }` when a pane turns dead, mapping window → agent through the runtime's table. A table entry lives until its window closes, even after a newer window for the same agent exists, so a late exit still names its own window. A table window missing from a poll reply is gone: `WindowGone { agent, window }`, unless that window was already reported gone.
- **start:** `run_dir = cfg.run_root/<agent>`; create it (0700), check every `cmd.files` path is inside it (else `StartFailed`), write `cmd.files` (0600) and `run_dir/launch.sh` (0700):
  ```sh
  #!/bin/sh
  export PATH=<sh_quote(current PATH)>      # Review Focus 4; DuoHive's PATH, not the tmux server's
  export <K>=<sh_quote(V)> …               # cmd.env
  cd <sh_quote(cwd)> || exit 126
  exec <sh_quote(program)> <sh_quote(arg)> …
  ```
  then `new-window -d -t =duohive: -n <agent> -P -F '#{window_id}' <tmux_quote(&format!("sh {}", sh_quote(launch.sh path)))?>` (the inner `sh_quote` keeps a data dir with spaces one word; a `QuoteError` → `StartFailed`), then `set-option -w -t <id> @duohive_agent <agent>` → `Started` (no per-window silencing command: `no-output` covers new windows, S0.2). Any failure → `StartFailed { reason }`.
- **stop:** `send-keys -t <id>` each key (`Text` with `-l`), pausing 150 ms between two keys (`KEY_PAUSE`; sent back to back, a TUI reads them as one burst, TD §5.5), then after `grace`, if the window still exists, `kill-window -t <id>`. `WindowClose` (`%unlinked-window-close` or `%window-close`, treated the same; S0.4) → `WindowGone { agent, window }`.
- **kill:** `kill-window -t <window>` for exactly the window given, never by agent name, so killing an old window can't touch a relaunched one.
- **paste:** `tmux -L <socket> load-buffer -b <buffer_name> -` as a separate process with the text on stdin (no command-line length or newline limits), then `paste-buffer -d -p -b <buffer_name> -t <id>`, then the submit keys. Failure → `PasteFailed`.
- **capture:** `capture-pane -p -J -t <id>` → `Screen { text }` (body lines joined with `\n`).
- **inventory:** `list-windows -t =duohive -F "#{window_id}\t#{@duohive_agent}\t#{pane_dead}\t#{pane_dead_status}\t#{window_activity}"` (double-quoted so `\t` is a real tab, as in Activity) → `Inventory`; untagged windows (the keeper) skipped.

- [x] **Step 1: Write the failing integration tests.** `support::TestServer::new()` picks socket `duohive-test-<random hex>` and a temp `run_root`, uses `tmux` from `PATH`, and on `Drop` runs `kill-server`. Programs are `/bin/sh` one-liners; no CLI needed.

```rust
#[tokio::test] async fn start_reports_started_and_tags_the_window() { /* Started{window "@N"}; list-windows shows @duohive_agent == agent */ }
#[tokio::test] async fn launch_script_passes_args_verbatim() {
    /* program "/bin/sh", args ["-c", "printf '%s\\n' \"$@\" > \"$OUT\"", "x", "it's", "$(touch pwned)", "`id`", "-rf", "a\nb", "é ✓"];
       OUT file equals the six args, one per line (a\nb as two lines), and no "pwned" file exists */
}
#[tokio::test] async fn env_carries_token_and_path() { /* env [(DUOHIVE_TOKEN, "tok-1")], program sh -c 'printf "%s|%s" "$DUOHIVE_TOKEN" "$PATH" > "$OUT"' → OUT == "tok-1|" + the test process's PATH */ }
#[tokio::test] async fn activity_reported_for_window_without_output() { /* loop printing every 100 ms → ≥ 2 Activity events within 4 s; no %output lines read */ }
#[tokio::test] async fn idle_windows_send_nothing() { /* `cat` windows → no Activity for 3 s after the first */ }
#[tokio::test] async fn start_works_with_a_space_in_the_data_dir() { /* run_root "<tmp>/My Data/run" → Started; the program runs (writes its OUT file); launch.sh is under "<tmp>/My Data/run/<agent>/" */ }
#[tokio::test] async fn pane_died_reports_exit_status() { /* sh -c 'exit 3' → PaneDied{ window: the Started @id, status: Some(3) } */ }
#[tokio::test] async fn kill_targets_only_the_given_window() { /* agent a started as "@1" (`cat`), killed, started again as "@2"; kill(a, "@1") again → "@2" still listed; WindowGone events name "@1" only */ }
#[tokio::test] async fn stop_sends_quit_keys() { /* program exits on "q\n" → PaneDied{Some(0)} before grace */ }
#[tokio::test] async fn stop_kills_after_grace() { /* trap '' INT; sleep 100 → WindowGone after ~grace (1 s in test) */ }
#[tokio::test] async fn capture_returns_screen_text() { /* window runs `printf 'hello\nworld\n'; cat` → capture → Screen{ text } whose non-empty lines start with ["hello", "world"] */ }
#[tokio::test] async fn paste_types_multiline_text() { /* `cat > out` window; paste "line one\nline two" + Enter; out has both lines */ }
#[tokio::test] async fn inventory_lists_tagged_windows_with_dead_status() { /* agents a (`cat`) and b (`exit 2`) → Inventory has a with dead None and b with dead Some(Some(2)); the keeper window is absent */ }
#[tokio::test] async fn connection_lost_when_server_killed() { /* `tmux -L <socket> kill-server` → ConnectionLost within 2 s */ }
#[tokio::test] async fn control_client_loss_reattaches() { /* `tmux -L <socket> detach-client -s duohive` drops the control client while the server lives → no ConnectionLost; a noisy window keeps producing Activity within 3 s; an `exit 4` window that died meanwhile → PaneDied{ Some(4) } */ }
#[tokio::test] async fn reconnect_finds_windows_started_by_a_previous_connection() { /* start a (`cat`), drop the runtime, connect again, inventory → [LiveWindow{ agent: a, window: the same @id, dead: None, .. }] */ }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-tmux --test runtime`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS** on the local tmux and on 3.2a: `PATH=$HOME/.local/tmux-3.2a/bin:$PATH cargo test -p duohive-tmux --test runtime`.
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(tmux): runtime over tmux control mode"`

### Tasks 20–21: CLI adapters — Claude Code, OpenCode

Each task builds one adapter in `crates/duohive-cli/src/<cli>.rs` against its spike section; Task 20 also adds the shared pieces.

**Files:**
- Task 20 creates: `src/lib.rs` (`pub fn real_adapters() -> Adapters`), `src/detect.rs`, `src/hooks_json.rs`, `src/claude.rs`
- Task 21 creates: `src/opencode.rs`, `src/assets/opencode-plugin.js`
- Tests: beside the code, reading `tests/fixtures/<cli>/<version>/`; snapshots in `src/snapshots/`
- Task 20 modifies `crates/duohive-cli/Cargo.toml`: `[dev-dependencies] insta = { workspace = true, features = ["yaml"] }` (`assert_yaml_snapshot!` needs the `yaml` feature; `LaunchCommand` derives `Serialize`, Task 10)

**Interfaces:**
- Consumes: `CliAdapter` (including `enrich_hook`), `LaunchSpec`, `LaunchCommand`, `HookSignal`, `ScreenSignal`, `HookCoverage`, `Key`, `sh_quote` (Task 10); spike sections **Sn.1, .2, .3, .4, .7, .8, .9, .10, .11, .13**. MCP (Sn.5) and model listing (Sn.6) are not built here (phases 3 and 4).
- Produces:

```rust
pub struct ClaudeAdapter; pub struct OpencodeAdapter;   // each impl CliAdapter
pub fn real_adapters() -> Adapters;
pub fn probe_version(kind: CliKind) -> CliPresence;          // detect.rs: runs `<binary> --version`, 2 s timeout, Sn.1 regex;
                                                              // each real adapter's CliAdapter::detect calls it
pub fn parse_cli_version(kind: CliKind, out: &str) -> Option<String>;
pub fn hook_command(spec: &LaunchSpec, event: &str) -> Result<String, LaunchError>;   // "'<hook_exe>' hook '<agent>' <event>", sh-quoted; a non-UTF-8 hook_exe → LaunchError::Invalid
```
Each adapter:
- `launch`: instructions per **Sn.2** (`spec.agent.prompt`); hook configuration per **Sn.3**, written as a `LaunchFile` under `spec.run_dir`, each hook running `hook_command(spec, <duohive event>)`; model and effort per **Sn.6**'s flags when set; `spec.agent.flags` appended; the task last per **Sn.8**, with a `--` separator before it when the CLI supports one (Review Focus 4); `cwd = expand_home(directory, home)`.
  - OpenCode (S2): one config `LaunchFile` (`opencode.json`: `instructions`, `plugin`, `mcp` from phase 3, and effort as `agent.build.variant`; **no** `permission` key, S2.11) named by `OPENCODE_CONFIG` in `env`, plus the plugin asset written as a second `LaunchFile` and loaded as `file://<run_dir>/opencode-plugin.js`. Model as `--model=<m>`; `--agent=build` unless `spec.agent.flags` already contain `--agent` (the user's choice; only agents the plugin's `config` hook composes are guarded); the task as one `--prompt=<task>` argument (no `--`: the positional argument is the project directory, S2.8). The plugin runs `duohive hook` as an argv (no shell), so instead of `hook_command`'s string it gets `spec.hook_exe` and the agent name through `env`. The plugin (written with `node:child_process` and `node:fs` only, which Bun also provides, so it runs under Bun in OpenCode and under Node in tests):
    - **Events:** maps S2.3's rows; sends `session-start` once, on the first `catalog.updated` after it loads, or on the first prompt's event (`session.created` or `chat.message`) if that comes first, or 3 s after load if neither comes (S2.8: a paste at the plugin's load is lost; the 3 s is read from an optional env var so tests can shorten it); sends `busy` only on change; adds `duohive_last_reply` to `turn-end` by S2.4's rule.
    - **Approvals (TD §2, S2.11):** its `config` hook makes `edit` and `bash` `ask` where the user's resolved value is `allow` or absent at the top level, or `allow` in an `agent.<name>.permission`; `ask`, `deny` and every pattern other than `*` stay as the user wrote them; a string shorthand `"permission": "allow"` is read as `{"*": "allow"}`. A static `permission` key is not used: it replaces the user's value and can turn their `deny` into `ask`.
    - **Never disturb OpenCode (Review Focus 1):** every hook returns at once; deliveries go on one internal promise chain, in event order, never awaited by OpenCode's hooks; a spawn that throws (a missing exe throws synchronously), rejects or exits non-zero is caught and dropped, and nothing is ever thrown into OpenCode; the child's stdout and stderr are ignored; a delivery still running after 10 s is killed and the chain goes on (`DELIVERY_LIMIT_MS`; a loaded machine can take seconds to start one, so a shorter limit dropped real hooks, TD §5.2); `dispose` waits for the chain for at most 1 s, then sends `session-end` with a synchronous spawn bounded at 1 s (S2.3: an async child started in `dispose` may not run).
- `coverage`: exactly the Sn.3 table (a DuoHive event with no hook → `false`); `last_reply` per Sn.4.
- `parse_hook`: DuoHive event name → `HookSignal` (Sn.3); `turn-end` extracts the reply per Sn.4 (for Claude Code, from the `Stop` payload's `last_assistant_message`, S1.4; when that field is missing, from `"duohive_last_reply"`, which `enrich_hook` fills from the transcript; for OpenCode, from `"duohive_last_reply"`, which the plugin adds to the `session.idle` payload, S2.4, so its `enrich_hook` stays the no-op).
- `read_screen`: the Sn.7 distinguishing strings → `ScreenState`, matched after the same normalisation as the fake adapter's (Task 27): trailing blank rows dropped, each line's trailing whitespace trimmed, rules read from the last non-empty line upward; the last reply line where Sn.4 says there is no hook.
- `startup_prompts`: true when Sn.8 lists any start-up prompt (Claude Code's workspace trust is one).
- `detect`: `probe_version(self.kind())`.
- `quit_keys` per Sn.9 and the 2026-10-04 stop-key measurements (TD §5.5): Claude Code `[CtrlC, Text("/exit"), Enter]` (the `C-c` clears a prompt that `Escape` put back in the input box), OpenCode `[Text("/exit"), Enter]`; `submit_keys` per Sn.10.
- `tested_versions`: the Sn.1 major.minor.

> Reading a transcript is file I/O, so `parse_hook` stays pure: the adapter's `enrich_hook` (Task 10 trait, default no-op) runs in the hook listener before forwarding; where Sn.4 says the reply lives in a file, it reads it and inserts `"duohive_last_reply"` into the payload, and `parse_hook` reads that field. For Claude Code the reply is in the payload (S1.4), so `enrich_hook` reads the transcript (S1.4's extraction rule) only for a `turn-end` payload with no `last_assistant_message`; this fallback is the one adapter call that reads a file, and it runs in the hook listener task, not the engine (Task 22).

- [x] **Step 1: Write the failing tests** for the adapter:

```rust
#[test] fn launch_snapshot_no_task() { insta::assert_yaml_snapshot!(adapter.launch(&spec(None)).unwrap()); /* OpenCode: the config LaunchFile has no "permission" key, and the plugin LaunchFile's config hook composes it (S2.11, S2.13) */ }
#[test] fn launch_snapshot_with_task_model_effort_flags() { /* spec task "Add retry cases", model "opus", effort "high", flags ["--verbose"] → snapshot; args contain the Sn.6 model and effort flags with those values, "--verbose", and the task as the last arg (OpenCode: the last arg is "--prompt=Add retry cases", and effort is `agent.build.variant` = "high" in the config LaunchFile, S2.6) */ }
#[test] fn launch_snapshot_hostile_text() { /* task "-rf $(id) `x` it's\nline2 é" appears as one untouched arg (OpenCode: "--prompt=" followed by the untouched text, S2.8) */ }
#[test] fn launch_files_stay_in_run_dir() { /* every LaunchFile.path starts with spec.run_dir */ }
#[test] fn no_auto_approve_flags() { /* args contain none of: --dangerously-skip-permissions, --permission-mode,
    nor any approval-bypass flag that Sn.11 names for the CLI (OpenCode: --auto; and its config LaunchFile has no "permission" key, so nothing the user set is replaced) */ }
#[test] fn hook_config_points_at_duohive_hook() { /* the hooks file names hook_command(spec, e) for each Sn.3 row (OpenCode: the config file's "plugin" entry is file://<run_dir>/opencode-plugin.js, that file is a LaunchFile, env names spec.hook_exe and the agent, and the plugin's event map has each S2.3 row) */ }
#[test] fn parses_recorded_hooks() { /* every fixture under hooks/ → the HookSignal its Sn.3 row names.
    OpenCode (fixtures are named by OpenCode event; the test passes the DuoHive event S2.3 maps each to): plugin.init-* and review-plugin.init-* → "session-start" → Ready;
    session.status-* with status.type "busy", hook.chat.message-*, hook.tool.execute.before-*/after-*, permission.replied-* (and review- ones) → "busy" → Busy;
    permission.asked-* → "needs-input" → NeedsInput { prompt: Some("<permission> <metadata.filepath>") }, e.g. "edit /private/tmp/duohive-spike/work-oc/yes.txt", falling back to "<permission> <patterns joined by ', '>" when metadata.filepath is absent;
    session.idle.reply-* → "turn-end" → TurnEnded { last_reply: duohive_last_reply }; session.idle-1/-2 (first plugin version, no duohive_last_reply) → TurnEnded { last_reply: None };
    hook.dispose-* → "session-end" → SessionEnded. Unmapped fixtures (session.created-*, session.updated-*, message.updated.completed-*, session.status-* with "idle"),
    passed with their OpenCode name as the event, → None */ }
#[test] fn extracts_last_reply_from_fixture() { /* Sn.4 fixture → expected text */ }
#[test] fn screen_rules_classify_recorded_screens() { /* at-rest → AtRest, working → Busy, approval → NeedsInput, start-up prompt → NeedsInput; each also with three blank rows and trailing spaces appended → the same state */ }
#[test] fn startup_prompts_match_spike_report() { /* assert_eq!(adapter.startup_prompts(), <Sn.8 lists a start-up prompt>) */ }
#[test] fn coverage_matches_spike_report() { /* assert_eq!(adapter.coverage(), HookCoverage { .. }) with each of the six fields copied from the Sn.3/Sn.4 table */ }
#[test] fn version_output_parses() { /* Sn.1 sample → "2.1.288"; garbage → None */ }
#[test] fn plugin_replays_recorded_events() { /* Task 21 only. Runs src/assets/opencode-plugin.js under `node` (or `bun`; the test prints a skip notice and passes when neither is on PATH)
    with a small JS driver in tests/: env names a fake hook exe (a script that appends argv and stdin as one JSON line to a file) and an agent; the driver calls the plugin with a stub ctx,
    calls its config hook with a sample user config, then feeds tests/fixtures/opencode/1.18.34/hooks/events.jsonl to its event hook in order, one plugin instance per segment between
    {"plugin":"init"} marker lines (a segment with no session events, like the `opencode mcp list` run, yields only session-start, by the shortened timer, and session-end), and calls dispose at each segment's end. Checks: per segment the DuoHive events are session-start first, busy only on change, needs-input at each
    permission.asked, busy after each permission.replied, turn-end at each session.idle with duohive_last_reply equal to replies/a1-turn-<n>.from-events.txt and
    replies/session.idle.reply-<n>.duohive_last_reply.txt in order ("(null)" → null), and session-end last; the delivered order equals the event order;
    the sample config's edit/bash come back composed per the launch rules above (a deny and a pattern kept, an allow made ask); and with the hook exe missing, nothing throws */ }
```
Task 20 adds `#[test] fn hook_command_quotes_paths_with_spaces()` (`hook_exe = "/Applications/My Tools/duohive"`).
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-cli <cli>`
- [x] **Step 3: Implement** the adapter (and in Task 20, `probe_version`, `hook_command`, `real_adapters`). Review the new snapshots with `cargo insta review`; accept only after checking each argument against the spike section.
- [x] **Step 4: Run, see PASS.** `cargo test -p duohive-cli`
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(cli): <cli> adapter"`

### Task 22: `duohive hook` and the hook listener

**Files:**
- Create: `crates/duohive/src/{args.rs, hook_cmd.rs, hook_listener.rs}`; Modify: `lib.rs` (from Task 1), `main.rs`; tests `crates/duohive/tests/hook.rs`

**Interfaces:**
- Consumes: `Paths`, `open_db`, `insert_pending_hook` (Tasks 16–17); `HookMessage`, `Input`, `Adapters`, `enrich_hook` (Tasks 10, 20).
- Produces:

```rust
// args.rs — Tasks 27 and 28 add variants and their parsing; Task 29 adds App's options and --help
pub enum Cli { App, Hook { agent: String, event: String } }   // Task 27 adds BenchAgent { hooks: bool }, Task 28 Bench(BenchArgs)
pub fn parse_args(args: &[String]) -> Result<Cli, String>;     // hand-rolled; args without the program name; [] → App
// hook_cmd.rs — `duohive hook <agent> <event>`; std only (no tokio), always exit code 0
pub fn run_hook(args: &[String], stdin: impl Read + Send + 'static, stdin_is_terminal: bool, env: &HashMap<String, String>) -> std::process::ExitCode;
// main.rs passes std::io::IsTerminal::is_terminal(&std::io::stdin()) before handing stdin over
// hook_listener.rs
pub enum ListenerError { AlreadyRunning, Io(std::io::Error) }
pub struct HookListener { /* bound UnixListener */ }
impl HookListener {
    pub fn bind(sock: &Path) -> Result<HookListener, ListenerError>;
    pub async fn serve(self, agents: AgentClis, adapters: Arc<Adapters>, to_core: mpsc::Sender<Input>, tap: Option<mpsc::UnboundedSender<HookMessage>>);
}
pub type AgentClis = Arc<std::sync::RwLock<HashMap<String, CliKind>>>;   // agent → its CLI, refreshed by the binary on OrgChanged
```
`run_hook`: token from `DUOHIVE_TOKEN` in `env` (missing → exit 0); paths from `Paths::from_vars(env)` (never the process environment directly, so tests pass a map). Payload, when `stdin_is_terminal` is false: a helper thread reads stdin to EOF (cap 4 MiB) and sends the bytes over a channel; the main thread waits with `recv_timeout(300 ms)`; on timeout (a CLI that leaves stdin open and silent) it proceeds with the bytes read so far, which the helper shares through an `Arc<Mutex<Vec<u8>>>`, and never joins the blocked thread (the process exits). Empty → `null`; `stdin_is_terminal` → `null` (Claude Code hooks and the OpenCode plugin both write JSON to stdin); non-JSON → wrapped as `{"raw": "<text>"}`. Connect to `run_sock()` with a 200 ms deadline: std's `UnixStream::connect` has no timeout, so a helper thread connects and sends the stream over a channel, and the main thread `recv_timeout(200 ms)`s (std only, no `unsafe`); then `set_write_timeout(150 ms)` and write one JSON line. If that fails, `open_db(state_db, 200 ms)` and `insert_pending_hook`. Worst case 300 + 200 + 150 + 200 = 850 ms plus process start, under 1 s (Review Focus 1). If that fails, append one line to `logs/hook.log` and stop. Nothing is ever written to stdout or stderr; the exit code is always 0.

`bind`: take the exclusive instance lock on `run.lock` beside the socket (`take_lock`, `File::try_lock`, opened close-on-exec); held by another instance → `AlreadyRunning` ("another DuoHive is already running (run.lock is held)"). With the lock held, any socket file at the path is stale (a crash) and is removed; bind, then `chmod 0600`. A connect probe is not used: it would race (TD §7). `serve`: per connection, read lines (max 1 MiB; longer lines are dropped with a warning), parse `HookMessage`, look up the agent's CLI, call `enrich_hook`, send `Input::Hook` to the core (and to `tap` when set — the bench uses it).

- [x] **Step 1: Write the failing tests.**

```rust
#[tokio::test] async fn hook_forwards_one_line_to_the_socket() { /* listener on a temp run.sock with a tap; run_hook(["hook", "pay-qa", "busy"], b"{\"a\":1}", false, {DUOHIVE_TOKEN: "tok-1", DUOHIVE_DATA_DIR: tmp}) → tap receives HookMessage{ token: "tok-1", agent: "pay-qa", event: "busy", payload: {"a":1} }; pending_hooks empty */ }
#[test] fn terminal_stdin_is_not_read() { /* stdin_is_terminal = true with a reader that panics if read → payload null, exit 0 */ }
#[test] fn hook_falls_back_to_the_database_when_no_listener() { /* pending_hooks has the row */ }
#[test] fn hook_survives_silent_open_stdin() {
    /* run the built binary with stdin a pipe the test keeps open and never writes; listener on run.sock with a tap →
       exit 0 in < 1 s, stdout and stderr empty; the tap receives the hook with payload null */
}
#[test] fn hook_survives_an_unreachable_socket() {
    /* bind a UnixListener at run.sock that never accepts and fill its backlog with non-blocking connects until one would block
       (where the OS refuses at once instead, the test still holds); run the built binary → exit 0 in < 1 s, stdout and stderr empty,
       pending_hooks has the row */
}
#[test] fn hook_never_fails_the_cli() {
    /* run the built binary (env!("CARGO_BIN_EXE_duohive")) with: no token; garbage stdin; data dir unwritable (chmod 0500);
       socket path is a regular file. Each: exit status 0, stdout and stderr empty, finished in < 1 s */
}
#[test] fn second_instance_refused() { /* bind twice → AlreadyRunning (the first holds run.lock) */ }
#[test] fn stale_socket_replaced() { /* leave a socket file and no lock holder → bind succeeds */ }
#[test] fn socket_is_owner_only() { /* mode == 0o600 */ }
#[tokio::test] async fn oversized_line_dropped() { /* write a 2 MiB line then a valid line → the core channel receives exactly one Input::Hook (the valid one) */ }
#[tokio::test] async fn listener_enriches_before_forwarding() { /* StubAdapter enrich inserts a field; core receives it */ }
```
The CLI entry for `hook` goes in `main.rs` now (`parse_args` returns `Cli::Hook` for `hook <agent> <event>`; `bench-agent` comes in Task 27, `bench` in Task 28, the app in Task 29). Add `#[test] fn args_parse_hook() { assert_eq!(parse_args(&s(["hook", "a", "busy"])), Ok(Cli::Hook { agent: "a".into(), event: "busy".into() })); assert!(parse_args(&s(["frob"])).is_err()); }` to `args.rs`.
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive --test hook && cargo test -p duohive --lib args`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat: duohive hook subcommand and hook listener"`

### Task 23: TUI — theme, tree rows and the main screen

**Files:**
- Create: `crates/duohive-tui/src/{theme.rs, format.rs, model.rs, tree.rs, render/mod.rs, render/header.rs, render/org_pane.rs, render/needs_you.rs, render/side_pane.rs, render/screens.rs}`

**Interfaces:**
- Consumes: `Event`, `Snapshot`, `AgentView`, `NeedsYouItem`, `Status`, `Org`, `Settings`, `Command` (Task 10).
- Produces:

```rust
pub struct Palette { /* every UI §2.3 token */ }   impl Palette { pub fn dark() -> Self; pub fn light() -> Self; pub fn mono() -> Self; }
pub fn palette_for(theme: Theme, no_color: bool) -> Palette;          // Auto → dark in phase 1; NO_COLOR → mono
pub fn format_age(d: Duration) -> String;                             // "<1m" "2m" "4h" "3d" (D13)
pub fn state_cell(s: &Status, now: Millis, spinner: Option<usize>) -> String;   // 16 cells: "~" or " ", glyph, " ", word padded to 9 (D23), age right-aligned in 4
pub fn truncate_middle(name: &str, width: usize) -> String;           // "billing-…-worker"
pub struct UiModel { pub org: Org, pub agents: BTreeMap<AgentName, AgentView>, pub needs_you: Vec<NeedsYouItem>,
    pub org_error: Option<OrgFileError>, pub settings_error: Option<OrgFileError>, pub selected: NodeKey, pub footer_note: Option<String>, pub dialog: Option<Dialog>,
    pub reconnecting: u32, /* … */ }
pub enum Dialog { Confirm(Confirm) }   // defined here because UiModel holds it; Task 24 renders it, Task 25 adds Form(Box<Form>) and Chooser(Chooser)
pub struct Confirm { pub title: String, pub lines: Vec<String>, pub yes_label: String, pub focus_yes: bool, pub on_yes: Command }
impl UiModel { pub fn apply(&mut self, e: Event) -> bool; }           // true = needs redraw
pub enum TreeRowKind { Duo, Team, Role, Agent, Reply, Problem, EmptyRole, EmptyOrg }
pub struct TreeRow { pub kind: TreeRowKind, pub key: Option<NodeKey>, pub prefix: String, pub label: String, pub right: String }
pub fn tree_rows(m: &UiModel, compact: bool, show_replies: bool, now: Millis) -> Vec<TreeRow>;
pub struct RenderCtx { pub now: Millis, pub spinner: usize, pub palette: Palette, pub settings: Settings, pub tmux_socket: String }
pub fn render(frame: &mut ratatui::Frame, m: &UiModel, ctx: &RenderCtx);
pub enum BlockingScreen { TmuxMissing { found: Option<String> } }
pub fn render_blocking(frame: &mut ratatui::Frame, s: &BlockingScreen);
```
Look (exact from the UI spec):
- **State cell, glyphs, colours:** UI §2.1, §2.3. Quiet replaces the working glyph with `◷` and the word with `quiet`. Spinner `◐◓◑◒` at 4 fps on on-screen working rows only; `●` with Reduce motion.
- **Tree:** UI §2.2 and §4.1 — Duo row `⬢ Duo · your manager agent`, teams `▣`, roles `◇`, connectors `├─ └─ │`, roll-ups per D12, last reply in quotes on the next line (cut at the pane edge with `…`), the selection bar `▌` in the padding cell plus the `selection` background. A row's second line shows, in priority: the error (`status.error` plus a hint: on Duo's row ` · e Edit org` (D17); on other rows ` · s stop` (D16; UI §4.12's ` · space retry · s stop` minus the phase 2 Give work hint), or no hint when the error already carries one, i.e. contains ` · `, as `opencode not found · install it or change its CLI (e)` does), the problem (`✕ … · e to fix`), the notice (`fg.muted`), else the last reply. Empty role: `no agents yet · n to add one` (UI §4.12 minus "or ask Duo"). Empty org: `Your org is empty · n to add a team, role or agent` in the side pane.
- **Layout** (UI §3, D7): ≥ 100 wide → org pane 50 columns with connectors and replies; 80–99 → 32 columns, compact (no connectors, no replies, `▾` before groups, state cell = glyph + age); below 80×24 → only `Make the window at least 80×24` and `◆ <n> need you`. The org pane has focus (heavy border `┏━┓`); Needs you is pinned at its bottom with `┣━ Needs you (n) ━┫` (UI §4.1.3 row format). Header (UI §4.1) without `? help` (D8): ` DuoHive   6 agents · ● 2 working · ◆ 2 need you`; compact ` DuoHive  ● 2 · ◆ 2`. Footer: ` ↑↓ move · s start/stop · n new · e edit · d delete · v replies · q quit`, or the footer note when set.
- **Side pane** (light border, D7): team or role summary (UI §4.1.2, without the `space` hint: `e edit`); stopped agent `○ stopped · s to start`; running agent title `<agent> · <CLI display>` and body `attach: tmux -L <socket> attach -t duohive:<agent>`; empty org message; while `reconnecting > 0` (from `Snapshot`, cleared by `Reconnected`), the side pane shows only UI §4.1.2's start-up state `Reconnecting to <n> agents…` (`agent` when n is 1).
- **Banner** (UI §4.12): ` ✕ org.toml line 42: unknown CLI "claud" · using your last good org · o open in $EDITOR` (no `line N` part when `line` is `None`). An invalid `settings.toml` (D29): ` ✕ settings.toml line N: <error> · using default settings`, on the next line when both are set.
- **Blocking screen:** UI §4.12 "DuoHive needs tmux", `Found: none.` or `Found: 3.1c.`

- [x] **Step 1: Write the failing tests.** Render with `ratatui::backend::TestBackend` and compare buffers as strings.

```rust
#[test] fn state_cells() { assert_eq!(state_cell(&working(2m), now, None), " ● working    2m"); assert_eq!(state_cell(&working_screen(1m), ..), "~● working    1m"); assert_eq!(state_cell(&quiet(6m), ..), " ◷ quiet      6m"); }
#[test] fn ages() { assert_eq!(format_age(secs(59)), "<1m"); assert_eq!(format_age(mins(4*60)), "4h"); assert_eq!(format_age(days(3)), "3d"); }
#[test] fn names_truncate_in_the_middle() { assert_eq!(truncate_middle("billing-invoice-worker", 16), "billing-…-worker"); }
#[test] fn org_pane_matches_spec_4_1() {
    /* model = example_org with UI §4.1's states, ages and replies; render at 100×30;
       row 0 is the header (UI §4.1, docs/ux/ui-spec.md line 114) and row 1 the `┏━ Org` border (line 115);
       rows 2..=14, columns 0..50 (Duo through squad-3) equal ui-spec.md lines 116–128, columns 0..50, with "✉ 1" replaced by spaces */
}
#[test] fn needs_you_rows_match_spec() { /* "┣━ Needs you (2) ━…┫", "◷ squad-3      quiet      6m", "◆ billing-api  waiting    4m  \"Allow edit to…\"" */ }
#[test] fn compact_org_pane_matches_spec() { /* 80×24; UI §4.1 compact lines with "▸ QA (1)" shown unfolded as "▾ QA" + its agent (no folding in phase 1, D8) and no "✉1" */ }
#[test] fn header_counts_match_needs_you() { /* UI §4.1 model → header line starts " DuoHive   6 agents · ● 2 working · ◆ 2 need you"; the "need you" count == needs_you.len() */ }
#[test] fn errors_on_rows_match_spec_4_12() {
    /* Duo errored "opencode not found (code 127)" → second line "opencode not found (code 127) · e Edit org" (UI §4.12 exactly);
       auth-service "opencode not found · install it or change its CLI (e)" (no hint added); pay-api "exited unexpectedly (code 1)" → "exited unexpectedly (code 1) · s stop";
       billing-api "✕ ~/code/billing not found · e to fix"; pay-qa notice "notice: claude 2.3 untested (tested up to 2.1)" */
}
#[test] fn reconnecting_state_in_side_pane() { /* Snapshot reconnecting 6 → side pane line "Reconnecting to 6 agents…" (UI §4.1.2); apply(Reconnected) → line gone, returns true */ }
#[test] fn org_file_banner() { /* org_error Some{ line: Some(42), "unknown CLI \"claud\"" } → top line " ✕ org.toml line 42: unknown CLI \"claud\" · using your last good org · o open in $EDITOR"; line None → no "line 42" part; settings_error Some{ line: Some(3), "invalid type" } → " ✕ settings.toml line 3: invalid type · using default settings" (D29) */ }
#[test] fn too_small_screen() { /* 79×24 → "Make the window at least 80×24" and "◆ 2 need you" */ }
#[test] fn tmux_missing_screen_matches_spec() { /* render_blocking(TmuxMissing{ found: None }) at 80×24 → the UI §4.12 box text with "Found: none."; Some("3.1c") → "Found: 3.1c." */ }
#[test] fn empty_org_and_empty_role() { /* Org::blank → side pane "Your org is empty · n to add a team, role or agent"; Payments›Frontend with no agents → tree line "no agents yet · n to add one" */ }
#[test] fn no_color_keeps_heavy_border_and_bar() { /* palette_for(Auto, no_color: true) → every buffer cell has Color::Reset fg/bg; the org pane still draws "┏━ Org" and the selected row still starts with "▌" */ }
#[test] fn spinner_animates_only_without_reduce_motion() { /* working row at spinner 0 and 1 → glyphs "◐" then "◓"; with reduce_motion → "●" both times */ }
#[test] fn main_screen_snapshots() { insta::assert_snapshot!(render_to_string(80, 24)); insta::assert_snapshot!(render_to_string(160, 48)); }
#[test] fn apply_events_update_model() { /* AgentStatusChanged → agents[x].status; NeedsYouChanged → needs_you; returns true */ }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.** Review snapshots with `cargo insta review`.
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(tui): theme, org tree and main screen"`

### Task 24: TUI — app loop, navigation, start/stop and delete

**Files:**
- Create: `crates/duohive-tui/src/{input.rs, app.rs, render/dialog.rs}`

**Interfaces:**
- Consumes: Task 23 (including `Dialog` and `Confirm`).
- Produces:

```rust
pub enum Action { Up, Down, StartStop, New, Edit, Delete, ToggleReplies, OpenOrgFile, Quit, DialogYes, DialogNo, DialogEnter, Esc, Tab, BackTab, Char(char), /* … */ }
pub fn map_key(m: &UiModel, k: crossterm::event::KeyEvent) -> Option<Action>;
pub fn update(m: &mut UiModel, a: Action) -> Vec<UiEffect>;          // pure; tested without a terminal
pub enum UiEffect { Send(Command), Quit, OpenEditor(PathBuf) }
pub struct AppConfig { pub org_toml: PathBuf, pub tmux_socket: String, pub settings: Settings, pub no_color: bool, pub editor: Option<String> }   // editor: the `$EDITOR` value, read by the caller (the TUI reads no environment)
pub enum InputEvent { Key(crossterm::event::KeyEvent), Resize(u16, u16) }
pub async fn run_app_on<B: ratatui::backend::Backend>(terminal: &mut ratatui::Terminal<B>, input: mpsc::Receiver<InputEvent>,
                     cfg: AppConfig, events: mpsc::UnboundedReceiver<Event>, commands: mpsc::Sender<Command>,
                     first_frame: Option<tokio::sync::oneshot::Sender<()>>) -> Result<(), TuiError>;
// the backend-generic loop: all drawing, input handling and timing live here; input is a channel (not a Stream), so no `futures` dependency
pub async fn run_app(cfg: AppConfig, events: mpsc::UnboundedReceiver<Event>, commands: mpsc::Sender<Command>,
                     first_frame: Option<tokio::sync::oneshot::Sender<()>>) -> Result<(), TuiError>;
// the crossterm wrapper: raw mode, alternate screen and panic hook; a blocking thread forwards crossterm::event::read() as InputEvent; then run_app_on
// first_frame fires once, right after the first frame is drawn: the app's single first-frame signal (start_system waits on it before CLI detection, Task 27)
```
Behaviour (UI §4.11, §4.8, D8):
- `↑↓` move between node rows (reply/problem lines are skipped). Landing on a finished agent sends `ViewAgent` (UI §2.1 "Selecting the agent makes it idle").
- `s`: stopped agent → `StartAgent`; errored agent → `StopAgent` (D16: → stopped, leaves Needs you; matches Task 23's ` · s stop` hint); running agent → the "Stop agent?" dialog (UI §4.8; phase 1 text: `Stop <agent>? It is <word> (<age>).`), focus on Cancel; on Duo, same rules. `s` on a team or role → footer `s starts or stops an agent`.
- `d`: a stopped agent → `Delete agent <agent> in <breadcrumb of role>?`; a role or team → UI §4.8 "Delete role?" (or "Delete team?") listing every agent it removes, without the worktree line; disabled with the footer `stop <agent> first` when any is running; on Duo → footer `Duo can't be deleted`.
- Dialogs: open with focus on Cancel; `enter` acts on the focused button, `y` confirms, `n` or `esc` cancels; `tab` moves between buttons.
- `v` toggles `show_last_reply` and sends `UpdateSettings`. `o` → `OpenEditor(org_toml)`: leave the alternate screen, run `$EDITOR` (else `vi`), restore, send `ReloadOrg`.
- `q` or `ctrl+c` quit (agents keep running) — not while a form has focus.
- `CommandRejected { reason }` and `GiveWorkRefused` set the footer note until the next key.
- **Drawing** (TD §3.3): redraw only when `apply` or `update` returns a change, capped at 30 fps; the 250 ms spinner tick runs only while a working row is on screen and Reduce motion is off; a once-a-minute tick redraws while any age (state cell or Needs you row) is on screen, so ages stay current; otherwise nothing is drawn while idle. The terminal is restored on exit and on panic (panic hook).

- [x] **Step 1: Write the failing tests** (pure `update` and `map_key`; dialogs rendered with `TestBackend`).

```rust
#[test] fn down_up_skip_reply_lines() { /* UI §4.1 model, selected auth-service; Down → selected Agent(billing-api) (its "Running go test" reply line skipped); Up → Agent(auth-service) */ }
#[test] fn selecting_a_finished_agent_sends_view_agent() { /* selected squad-1; Down onto squad-2 (finished) → effects == [Send(ViewAgent{ squad-2 })] */ }
#[test] fn s_on_stopped_sends_start() { /* selected pay-qa (stopped); s → [Send(StartAgent{ pay-qa })] */ }
#[test] fn s_on_errored_sends_stop() { /* D16: pay-qa errored; s → [Send(StopAgent{ pay-qa })]; no dialog, no StartAgent */ }
#[test] fn s_on_running_opens_stop_confirm_focused_on_cancel() { /* snapshot: "Stop auth-service? It is working (2m).", "[ y Stop ]   ▌[ n Cancel ]" */ }
#[test] fn y_in_stop_confirm_sends_stop() { /* stop dialog open for auth-service; DialogYes → [Send(StopAgent{ auth-service })]; dialog None */ }
#[test] fn enter_on_cancel_and_esc_close_without_command() { /* dialog focused on Cancel; DialogEnter → [] and dialog None; reopen, Esc → [] and dialog None */ }
#[test] fn d_on_running_agent_sets_footer_reason() { /* auth-service working; d → [] and footer_note Some("stop auth-service first") */ }
#[test] fn d_on_role_lists_its_agents() { /* "Delete role QA in Payments? It removes 1 agent:", "pay-qa (stopped)" */ }
#[test] fn v_toggles_replies_and_saves() { /* show_last_reply true; v → [Send(UpdateSettings(s))] with s.show_last_reply false */ }
#[test] fn q_and_ctrl_c_quit() { /* map_key('q') and map_key(ctrl+c) → Some(Quit); update(Quit) → [UiEffect::Quit], no StopAgent */ }
#[test] fn rejection_shows_in_footer_until_next_key() { /* apply(CommandRejected{ "pay-qa is already running" }) → footer_note Some(that); next key Down → footer_note None */ }
#[tokio::test] async fn first_frame_signal_fires_once_after_drawing() { /* run_app_on over Terminal<CountingBackend>; Some(tx) → rx resolves once the draw count is 1, and is not sent again */ }
#[tokio::test(start_paused = true)] async fn idle_app_does_not_redraw() { /* run_app_on over Terminal<CountingBackend>, input channel kept open and silent, a model with no working row (no spinner); after the first frame, no events, no keys for 1 s of paused time → draw count unchanged */ }
#[tokio::test(start_paused = true)] async fn ages_redraw_once_a_minute() { /* run_app_on; example_org with agents idle, finished, waiting and stopped (ages shown, no working row, so no spinner) → exactly one more draw after 60 s of paused time */ }
```
The test harness's `CountingBackend` wraps `TestBackend` and counts `flush` calls (ratatui flushes once per `Terminal::draw`); the tests drive `run_app_on` with it and an `mpsc` input channel, never a real terminal.
`start_paused` needs tokio's `test-util` feature: add `tokio = { workspace = true, features = ["macros", "rt", "test-util"] }` to `duohive-tui`'s `[dev-dependencies]`.
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(tui): app loop, navigation, start/stop and delete"`

### Task 25: TUI — team, role, agent and Edit org forms

**Files:**
- Create: `crates/duohive-tui/src/forms/{mod.rs, field.rs, line_edit.rs, agent.rs, role.rs, team.rs, org.rs, chooser.rs}`

**Interfaces:**
- Consumes: `inherited_for`, `validate`, `FieldError`, `next_agent_names`, `DirProbe`, `DirStatus` (Tasks 7–8); `OrgEdit` drafts (Task 10); `Dialog` (Task 23); `UiEffect`, `Action` (Task 24).
- Modifies: `Dialog` gains `Form(Box<Form>)` and `Chooser(Chooser)`; `update` routes actions to the open form.
- Produces:

```rust
pub struct LineEdit { /* text, cursor */ }  // insert, backspace, delete, ←→, home/end; alt+enter newline in prompt fields
pub enum FieldKind { Text, Choice(Vec<String>) /* ←→ cycle; first entry "inherit: <value>" */, Prompt, Count, ReadOnly }
pub struct Field { pub label: String, pub kind: FieldKind, pub edit: LineEdit, pub error: Option<String>, pub note: Option<String> }
pub struct Form { pub title: String, pub fields: Vec<Field>, pub focus: usize, pub stacked: Vec<(String, Option<String>)>, pub footer: Option<String>, /* target */ }
impl Form {
    pub fn new_agent(org: &Org, role: &RoleKey, dirs: &dyn DirProbe, home: &Path) -> Form;
    pub fn edit_agent(org: &Org, name: &AgentName, view: &AgentView, dirs: &dyn DirProbe, home: &Path) -> Form;
    pub fn new_role(org: &Org, team: Option<&NodeName>, ..) -> Form;   pub fn edit_role(..) -> Form;
    pub fn new_team(org: &Org, ..) -> Form;                             pub fn edit_team(..) -> Form;
    pub fn edit_org(org: &Org, duo: &AgentView) -> Form;            // Duo's view decides whether Name is editable
    pub fn handle(&mut self, a: Action, org: &Org) -> FormOutcome;    // Edit(OrgEdit) | Cancel | Continue | Confirm(Confirm)
    pub fn error_count(&self) -> usize;
}
pub struct Chooser { /* UI §4.4.2 "New · <x> selected" */ }   impl Chooser { pub fn for_selection(org: &Org, sel: &NodeKey) -> Chooser; }
```
`AppConfig` gains `dirs: Arc<dyn DirProbe>` and `home: PathBuf`.

Look and rules (UI §4.4.1–4.4.2, D11, D14):
- Agent form fields: Name, CLI (choice), Directory (with `✓ git repo` / `✓ found` / `✕ not found`), Model and Effort (text, `inherit: <value>` placeholder in `fg.muted`), Flags, Worktrees (choice `inherit: <v>` / `on` / `off`; `on` is disabled with `new worktree needs a git repo; <dir> isn't one` when the resolved directory isn't a git repo), then `Prompt  (stacks under: Org › Payments › Backend)` with the `┊ <level>  <prompt or (none)>` lines and the own-prompt field.
- Clearing a field restores inheritance; validation runs on every change (`validate` on the org with the draft applied, plus field-local checks); errors show on the line under the field (`✕ an agent named auth-service already exists`); `Save` is disabled and the footer says `fix N error(s) to save` while errors remain.
- Rename only while stopped: otherwise the Name field is read-only with `rename only while stopped with no worktrees` under it. A running agent shows `<agent> is running: changes apply on next start`.
- Role form (UI §4.4.1 "New role") plus Model, Effort, Flags rows (D11), and `Agents [ n ] create this many now (<first name> …)` using `next_agent_names` with the role's team (D11: `payments-frontend-1` in Payments). Team form: the role form without Agents. Edit org form: UI §4.4.1 "Edit org"; a changed Duo CLI while Duo runs asks `Restart Duo on <CLI>?` (focus on Cancel) before saving. Duo's Name follows the agent rename rule (Task 14): unless Duo is stopped, the Name field is read-only with `rename only while stopped with no worktrees` under it.
- Keys: `tab`/`shift+tab` move fields, `ctrl+s` saves, `esc` cancels, `enter` on the last field moves to Save.
- Chooser (`n`): on Duo → `Team`, `Role`; on a team → `Role in <team>`, `Team next to <team>`, `Agent: select a role first` (disabled); on a role or an agent → `Agent in <role>`.

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn edit_agent_form_matches_spec_4_4_1() { /* billing-api running; snapshot equals UI §4.4.1 "Edit agent" box with Model/Effort as text placeholders */ }
#[test] fn duplicate_name_error_disables_save() { /* "✕ an agent named auth-service already exists"; footer "fix 1 error to save"; ctrl+s does nothing */ }
#[test] fn worktree_on_disabled_for_non_git_dir() { /* "new worktree needs a git repo; ~/notes isn't one" */ }
#[test] fn clearing_a_field_restores_inherit() { /* model "opus" → cleared → draft.fields.model None, shows "inherit: sonnet" */ }
#[test] fn rename_read_only_while_running() { /* edit_agent(billing-api, Working) → Name field kind ReadOnly, note "rename only while stopped with no worktrees"; Char('x') leaves the name "billing-api" */ }
#[test] fn stacked_prompts_show_none_for_empty_levels() { /* edit_agent(billing-api) → stacked == [("Org", Some("Ask before you delete anything.")), ("Payments", None), ("Backend", Some(..))]; rendered line "┊ Payments  (none)" */ }
#[test] fn ctrl_s_on_valid_agent_form_sends_edit() { /* UiEffect::Send(EditOrg(EditAgent{..})) */ }
#[test] fn new_role_form_creates_agents() { /* new role in Payments, "Frontend", 2 → AddRole{ team: Some(Payments), agents: 2 }; label "(payments-frontend-1 …)" as in UI §4.4.1; under the root → "(frontend-1 …)" */ }
#[test] fn new_team_form() { /* new_team; type "Growth"; ctrl+s → Edit(AddTeam(TeamDraft{ name: "Growth", defaults: Fields::default() })); no Agents field */ }
#[test] fn edit_org_form_changing_duo_cli_asks_first() { /* Duo running on OpenCode; Runs on → Claude Code; ctrl+s → FormOutcome::Confirm with title "Restart Duo on Claude Code?" and focus_yes false; yes → Edit(EditOrgDefaults{ duo.cli: Claude, .. }) */ }
#[test] fn edit_org_name_read_only_while_duo_runs() {
    /* edit_org(org, Duo Idle) → Name field kind ReadOnly, note "rename only while stopped with no worktrees"; Char('x') leaves "Duo";
       edit_org(org, Duo Stopped) → Name editable; type "Boss", ctrl+s → Edit(EditOrgDefaults{ duo.name: "Boss", .. }) */
}
#[test] fn chooser_options_follow_selection() { /* Duo → ["Team", "Role"]; Payments → ["Role in Payments", "Team next to Payments", "Agent: select a role first" (disabled)]; auth-service → ["Agent in Backend"] */ }
#[test] fn esc_cancels_without_command() { /* edit_agent with the model changed; Esc → FormOutcome::Cancel; no UiEffect::Send */ }
#[test] fn line_edit_basics() { /* type "abc", ←, backspace → "ac" cursor 1; home, delete → "c"; end, type "d" → "cd" */ }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui forms`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat(tui): team, role, agent and Edit org forms"`

### Task 26: Real ports — system clock, random tokens and the directory probe

**Files:**
- Create: `crates/duohive/src/real_ports.rs`; Modify: `crates/duohive/src/lib.rs`, `crates/duohive/Cargo.toml` (`getrandom`)

**Interfaces:**
- Consumes: `Clock`, `Ids`, `Token`, `Millis` (Tasks 6, 10); `DirProbe`, `DirStatus` (Task 8).
- Produces (used by `start_system` in Task 27 and by the TUI forms through `AppConfig.dirs` in Task 29):

```rust
pub struct SystemClock;   // impl Clock: ms since UNIX_EPOCH from SystemTime::now()
pub struct RandomIds;     // impl Ids: 32 bytes from getrandom::fill, lowercase hex (D1)
pub struct FsDirProbe;    // impl DirProbe: GitRepo when <dir>/.git exists (file or directory), Found when <dir> is a directory, else Missing
```
`getrandom` failing means the OS has no entropy source; a guessable token would break hook authentication, so `RandomIds::new_token` logs the error and calls `std::process::abort()` rather than return a weak token (no `unwrap`/`expect`).

- [x] **Step 1: Write the failing tests** (unit tests in `real_ports.rs`).

```rust
#[test] fn random_ids_are_64_lowercase_hex() {
    let t = RandomIds.new_token().0;
    assert_eq!(t.len(), 64);
    assert!(t.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')), "{t}");
    assert_ne!(t, RandomIds.new_token().0);
}
#[test] fn system_clock_is_unix_ms() {
    let sys = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    assert!(SystemClock.now().0.abs_diff(sys) < 1_000);
}
#[test] fn dir_probe_detects_git_repos_and_worktrees() {
    /* tmp/repo with a .git directory → GitRepo; tmp/wt with a .git file (a git worktree) → GitRepo;
       tmp/plain (a directory) → Found; tmp/file.txt (a file) → Missing; tmp/none → Missing */
}
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive --lib real_ports`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat: real clock, token and directory ports"`

### Task 27: Fake CLI adapter, `bench-agent` and the headless system

**Files:**
- Create: `crates/duohive-cli/src/fake.rs`; `crates/duohive/src/{system.rs, fake_agent.rs}`; tests `crates/duohive/tests/system.rs`
- Modify: `crates/duohive/src/{args.rs, main.rs}` (hidden `bench-agent` subcommand)

**Interfaces:**
- Consumes: `Engine`, `run_core` (Tasks 11–15); `Paths`, `SqliteStore`, settings and org files (Tasks 16–17); `TmuxRuntime` (Task 19); `HookListener`, `AgentClis` (Task 22); `SystemClock`, `RandomIds`, `FsDirProbe` (Task 26).
- Produces:

```rust
// duohive-cli/src/fake.rs
pub struct FakeCliAdapter { pub kind: CliKind, pub exe: PathBuf, pub hooks: bool }   // impl CliAdapter
// launch → exe "bench-agent" [--no-hooks] [--task <spec.task>] (the task as one argument, verbatim); detect → Found { version: "0.0.0-fake" } with tested_versions ["0.0"] (no notice),
// so the bench, the e2e tests and CI need no real CLI installed; parse_hook maps the five DuoHive events;
// read_screen on the last non-empty line, trailing whitespace trimmed (capture-pane pads rows and lines): "❯" → AtRest,
// "working…" → Busy, ends with "allow? (y/n)" → NeedsInput; quit_keys [Text("/exit"), Enter];
// submit [Enter]; startup_prompts false; coverage: all six fields true when hooks, else all false.
pub fn fake_adapters(exe: &Path) -> Adapters;   // Claude → FakeCliAdapter { hooks: true }, Opencode → { hooks: false }
// duohive/src/system.rs — the headless assembly the binary, the bench and the e2e tests share
pub struct SystemOptions { pub tap: Option<mpsc::UnboundedSender<HookMessage>>,
                           pub first_frame: Option<oneshot::Receiver<()>> }   // the single first-frame signal
pub struct SystemHandle { pub commands: mpsc::Sender<Command>, pub events: mpsc::UnboundedReceiver<Event>, /* join handles */ }
impl SystemHandle { pub async fn shutdown(self); }
pub async fn start_system(paths: &Paths, adapters: Arc<Adapters>, opts: SystemOptions) -> Result<SystemHandle, StartError>;
pub struct EnrichingStore { /* SqliteStore, Arc<Adapters>, AgentClis */ }   // impl Store (D22)
```
`start_system`: `SqliteStore::open` wrapped in `EnrichingStore`, load settings and the org (D29: an `org.toml` that doesn't parse → `last_good_org()`, else `Org::blank(CliKind::Claude)`, with `org_error = Some(e)`; an invalid `settings.toml` → `Settings::default()` with `settings_error = Some(e)`), `HookListener::bind` (→ `StartError::AlreadyRunning` with the message `DuoHive is already running`), `TmuxRuntime::connect` with `TmuxConfig { socket: paths.tmux_socket, session: "duohive", run_root: paths.data_dir.join("run"), .. }` (D31), `Engine::new` with `org_error`, `settings_error`, `Ports { SystemClock, RandomIds, Arc::new(FsDirProbe), .. }` and `Arc::clone(&adapters)`, then `start_up`; spawn `run_core`, the listener (with the same `Arc<Adapters>`) and the detection task. It keeps `AgentClis` updated from `OrgChanged`. The listener forwards live hooks at once, even during reconnect; the engine buffers them until the pending-hook replay finishes (Task 15), so replayed hooks always apply first (Review Focus 3).
- **CLI detection** lives only here: the detection task waits for `opts.first_frame` when it is `Some` (the app passes the receiver whose sender `run_app` fires after its first frame, Task 24), or runs at once when it is `None` (bench, tests); then it runs `adapters.detect_all()` on `spawn_blocking` and sends `Input::CliInventory`, then probes every resolved agent directory (`expand_home`) with `FsDirProbe` on `spawn_blocking` and sends `Input::DirInventory` (D26). With `fake_adapters` every kind reports `Found`, so nothing depends on which CLIs the machine has. On first run (no `org.toml`) `start_system` instead calls `detect_all()` once before the core starts, picks the blank org's CLI (`Org::blank`: the first `Found` in `CliKind::ALL` order, else Claude), and sends those results as the `CliInventory`; it never detects twice.
- **`EnrichingStore`** forwards every call to `SqliteStore`, except that `take_pending_hooks` runs the agent's adapter's `enrich_hook` (CLI looked up in `AgentClis`) on each message before returning it, so hooks replayed after a restart carry the Claude Code last reply exactly as live ones do (D22).
- **`SystemHandle::shutdown`** loses no hook (Review Focus 3; amended in review). It runs in this order:
  1. **Stop the listener and await it.** `HookListener::serve_until`:
     - removes `run.sock`, so a hook fired from then on fails to connect and falls back to `pending_hooks`;
     - accepts the connections already made;
     - reads each one to its end (up to 1 s) and forwards it to the core.
  2. **Send `Command::Shutdown`.** On it, `run_core`:
     - closes and drains its inputs;
     - stores every hook it has not applied through `Store::queue_pending_hook`: first the engine's start-up buffer, then the queued hooks.
  3. **Join the core thread.** Then abort detection and event forwarding and await them.
  4. **Release `run.lock` last.** `HookListener::take_lock` hands the lock to the handle.

  It never stops or kills an agent: tmux windows keep running. `duohive hook` stamps `HookMessage.sent_at_ms` before it delivers, and `pending_hooks` is replayed ordered by `sent_at_ms`, then id (migration 2 adds the column; older rows take `received_at`). So a hook the core stores at shutdown replays before a later one that fell back to the database meanwhile. Dropping a `SystemHandle` without `shutdown` only signals the listener and `try_send`s `Shutdown`; it waits for neither (see the `Drop` doc). Task 29's restart test uses `shutdown`.

`bench-agent [--no-hooks] [--task <text>]` (hidden subcommand, `fake_agent.rs`; `parse_args` gains `Cli::BenchAgent { hooks: bool, task: Option<String> }`): fires `session-start`, prints `❯ `; with `--task`, runs that text as its first turn (exactly as a stdin line, below) before reading stdin, so a launch with a task goes starting → working → finished; then for each stdin line: fires `busy`, prints `working…` lines every 100 ms for 1 s, or for 3 s when the line contains `slow` (so tests control the turn length), prints a reply, fires `turn-end` with `{"last_reply": "...", "sent_at_ms": <now>}`, prints `❯ `; on `/exit` fires `session-end` and exits 0. Every hook payload carries `sent_at_ms`. It runs `duohive hook` through `std::process::Command` with `DUOHIVE_TOKEN` from its environment.

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn fake_adapter_screen_rules() {
    let a = FakeCliAdapter { kind: CliKind::Opencode, exe: "duohive".into(), hooks: false };
    assert_eq!(a.read_screen("hello\n❯ \n\n\n").state, Some(ScreenState::AtRest));            // trailing blank rows, as capture-pane gives
    assert_eq!(a.read_screen("working…  \n\n").state, Some(ScreenState::Busy));
    assert_eq!(a.read_screen("Write file? allow? (y/n)\n\n").state, Some(ScreenState::NeedsInput));
    assert!(!a.coverage().complete());
}
#[test] fn fake_adapter_detects_without_a_real_cli() { /* detect() == Found { version: "0.0.0-fake" }; untested_notice("bench-agent", "0.0.0-fake", a.tested_versions()) == None */ }
#[tokio::test] async fn start_system_refuses_a_second_instance() {
    /* temp dirs, throwaway tmux socket, fake_adapters; start_system twice on the same Paths →
       the second is Err(StartError::AlreadyRunning) whose Display is "DuoHive is already running"; the first still answers commands */
}
#[tokio::test] async fn detection_waits_for_the_first_frame() {
    /* org.toml present (not first run); adapters wrap the fakes so detect() counts its calls; SystemOptions { first_frame: Some(rx) } →
       count 0 after 500 ms; tx.send(()) → count == 2 (one per CLI kind) within 2 s and still 2 a second later;
       the events show no directory problem before tx.send and an OrgChanged with the probed problems after it (D26) */
}
#[tokio::test] async fn start_system_with_broken_files_uses_last_good_and_defaults() {
    /* D29: save_org(example_org) (writes org.last-good.toml), then org.toml ← "cli = \"claud\"" garbage and settings.toml ← "quiet_after_minutes = \"x\"";
       start_system → first Snapshot org == example_org(), org_error Some with a line, settings_error Some;
       remove org.last-good.toml and start again on fresh dirs → Snapshot org == Org::blank(CliKind::Claude), org_error Some */
}
#[tokio::test] async fn bench_agent_slow_turn_takes_three_seconds() { /* fake agent given "a slow task" → turn-end ≥ 3 s after busy; "a task" → < 2 s */ }
#[test] fn fake_adapter_passes_the_task() { /* launch(spec task "it's -rf $(id)") → args == ["bench-agent", "--task", "it's -rf $(id)"]; task None → no "--task" */ }
#[tokio::test] async fn bench_agent_runs_its_task_first() {
    /* run `duohive bench-agent --task "a task"` with stdin open and silent, hooks recorded through a tap → session-start, busy, turn-end
       (last_reply set) in that order, with no stdin line written; then "next" on stdin → a second busy, turn-end */
}
#[tokio::test] async fn replayed_hooks_are_enriched() {
    /* D22: adapter whose enrich_hook inserts "duohive_last_reply": "from transcript"; insert a turn-end into pending_hooks;
       EnrichingStore::take_pending_hooks() → that message's payload["duohive_last_reply"] == "from transcript" */
}
#[tokio::test] async fn shutdown_leaves_agents_running() {
    /* start one fake agent → Idle; handle.shutdown().await → `tmux -L <socket> list-windows` still shows the agent's window;
       run.sock exists but a connect is refused */
}
#[test] fn args_parse_bench_agent() {
    assert_eq!(parse_args(&s(["bench-agent", "--no-hooks"])), Ok(Cli::BenchAgent { hooks: false, task: None }));
    assert_eq!(parse_args(&s(["bench-agent", "--task", "-rf a b"])), Ok(Cli::BenchAgent { hooks: true, task: Some("-rf a b".into()) }));
}
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive --test system && cargo test -p duohive-cli fake && cargo test -p duohive --lib args`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run, see PASS.**
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat: fake CLI adapter, bench-agent and the headless system"`

### Task 28: The basic `bench` and its report

**Files:**
- Create: `crates/duohive/src/bench/{mod.rs, measure.rs}`; tests `crates/duohive/tests/bench.rs`
- Modify: `crates/duohive/src/{args.rs, main.rs}` (`bench` subcommand: `parse_args` gains `Cli::Bench(BenchArgs)`)

**Interfaces:**
- Consumes: `start_system`, `SystemOptions`, `fake_adapters` (Task 27); `duohive_tui::render` (Task 23).
- Produces:

```rust
pub struct BenchArgs { pub agents: u32 /* 10 */, pub idle_secs: u64 /* 30 */, pub turns: u32 /* 3 */, pub json: bool }
pub struct BenchReport { pub agents: u32, pub tmux: String, pub platform: String, pub idle_cpu_pct: f64,
    pub peak_rss_mb: f64, pub hook_to_screen_ms: Percentiles }
pub struct Percentiles { pub p50: f64, pub p95: f64, pub max: f64 }
pub async fn run_bench(a: BenchArgs) -> Result<BenchReport, BenchError>;
pub fn percentiles(samples: &mut [f64]) -> Percentiles;
```
`run_bench`: temp config/data dirs, tmux socket `duohive-bench-<pid>`, an org with one root role `Bench` and N agents, `start_system(paths, Arc::new(fake_adapters(current_exe)), SystemOptions { tap: Some(..), first_frame: None })`. Adapters are keyed by CLI kind, so `Claude` maps to `FakeCliAdapter { hooks: true }` and `Opencode` to `FakeCliAdapter { hooks: false }` (a bench-only mapping that says nothing about the real OpenCode adapter's coverage); every fifth agent has `cli = "opencode"`, which exercises the screen-read path. The runtime silences panes with `no-output` and polls activity, as S0.5 chose (D25), so the bench measures that. It starts every agent, gives each `turns` tasks, and renders every event batch into a 160×48 `TestBackend` through `duohive_tui::render` (at most 30 fps), recording for each tapped hook the time its resulting state first appears in a rendered frame minus `sent_at_ms`. Then it waits for every agent to be at rest and measures idle CPU over `idle_secs` (`getrusage(RUSAGE_SELF)` user+sys delta ÷ wall time) and peak RSS (`ru_maxrss`, bytes on macOS, KiB on Linux). It stops all agents, calls `SystemHandle::shutdown`, and kills its tmux server. Output (plain, or JSON with `--json`):

```
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.4 %    target < 1 %
memory (peak)   11.2 MB  target < 20 MB
hook → screen   p50 12 ms · p95 40 ms · max 61 ms   target < 1000 ms
```
Targets at 10 agents come from PRD Success Metrics. Phase 1 reports; it never fails on a missed target (the release gate is phase 4–5).

- [x] **Step 1: Write the failing tests.**

```rust
#[test] fn percentiles_of_known_samples() { /* [1..=100] → p50 50.5, p95 95.05, max 100 (linear interpolation) */ }
#[test] fn report_formats_like_the_spec() { /* BenchReport{ 10, "3.7c", "macos-aarch64", 0.4, 11.2, {12, 40, 61} } → exactly the four lines above */ }
#[tokio::test] async fn bench_runs_two_agents() {
    /* run the built binary: `duohive bench --agents 2 --idle-secs 2 --turns 1 --json` → exit 0;
       JSON has agents 2, idle_cpu_pct ≥ 0, peak_rss_mb > 0, hook_to_screen_ms.p50 > 0; the bench tmux socket is gone afterwards */
}
#[test] fn args_parse_bench() { assert_eq!(parse_args(&s(["bench", "--agents", "10", "--json"])), Ok(Cli::Bench(BenchArgs { agents: 10, json: true, ..Default::default() }))); }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive --test bench && cargo test -p duohive --lib bench && cargo test -p duohive --lib args`
- [x] **Step 3: Implement.** `measure.rs` is the one module with `unsafe` (`libc::getrusage`), marked `#[expect(unsafe_code, reason = "getrusage has no safe wrapper in std")]`.
- [x] **Step 4: Run, see PASS.** Then `cargo run --release -p duohive -- bench` → the four lines above with real numbers.
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat: basic bench and its report"`

### Task 29: Binary wiring — start-up, logging, subcommands and end-to-end tests

**Files:**
- Create: `crates/duohive/src/{logging.rs, startup.rs}`; Modify: `args.rs`, `main.rs`; tests `crates/duohive/tests/e2e.rs`

**Interfaces:**
- Consumes: `Cli`, `parse_args` (Task 22, with the `BenchAgent` and `Bench` variants from Tasks 27–28), `start_system`, `SystemOptions`, `SystemHandle::shutdown`, `fake_adapters` (Task 27), `run_bench` (Task 28), `run_app` (Task 24), `tmux_version` (Task 19), `real_adapters` (Task 20), `FsDirProbe` (Task 26).
- Produces: `duohive` (the app), `duohive hook …`, `duohive bench …`, hidden `duohive bench-agent …`; `parse_args` gains `--help` (prints usage).

```rust
pub fn init_logging(paths: &Paths) -> Result<tracing_appender::non_blocking::WorkerGuard, LogError>;   // daily file in logs/, never the terminal
pub async fn run_app_main() -> std::process::ExitCode;
```
`run_app_main` order (TD §4.1 Start-up, §8): `Paths::from_env` → `init_logging` → `tmux_version`: missing or < 3.2 → `render_blocking(TmuxMissing)` until `q`, exit 1 → create the first-frame `oneshot` → `start_system(paths, Arc::new(real_adapters()), SystemOptions { tap: None, first_frame: Some(rx) })` (`AlreadyRunning` → print `DuoHive is already running` to stderr, exit 1) → `run_app(cfg with dirs: Arc::new(FsDirProbe), editor: the `$EDITOR` value (unset or blank → the TUI runs `vi`, Task 24), …, Some(tx))`; CLI detection then follows the first frame through that one signal (Task 27) → on quit, `SystemHandle::shutdown`; agents keep running in tmux. The TUI shows `Reconnecting to N agents…` until the engine's `Reconnected` (Tasks 15, 23).

- [x] **Step 1: Write the failing end-to-end tests** (real tmux on a throwaway socket, temp dirs, `fake_adapters` (`Opencode` hookless), events read from `SystemHandle`):

```rust
#[tokio::test] async fn e2e_start_work_finish_view() { /* StartAgent → Starting → Idle; GiveWork → Working → Finished with last reply; ViewAgent → Idle */ }
#[tokio::test] async fn e2e_stop_is_stopped() { /* StopAgent → Stopped; window gone */ }
#[tokio::test] async fn e2e_crash_is_errored_in_needs_you() { /* tmux kill the fake's process with SIGKILL → Errored "exited unexpectedly" (tmux 3.2 may report no status for SIGKILL, so the expected text has no code; the test checks the error starts with "exited unexpectedly" so a newer tmux's code also passes); NeedsYouChanged contains it */ }
#[tokio::test] async fn e2e_crash_then_immediate_restart() {
    /* SIGKILL the fake → Errored; GiveWork "again" sent at once, before the old window's close arrives → Starting → Working → Finished
       (reachable: the relaunch passes "again" as `bench-agent --task again`, whose session-start with a task moves Starting → Working and whose turn-end → Finished);
       no AgentStatusChanged to Errored after the GiveWork although the old window closes meanwhile; the new window's @id differs from the old one */
}
#[tokio::test] async fn e2e_duo_cli_change_restarts_duo() {
    /* Duo on claude (hooked fake) → Idle in window @a; EditOrgDefaults with duo.cli opencode (hookless fake) → Stopped, then Starting,
       then Idle in a new window @b ≠ @a; @a is gone from list-windows; Duo is never Errored */
}
#[tokio::test] async fn e2e_hookless_agent_gets_tilde_states() { /* agent on `opencode` (mapped to the hookless fake) → Working/Finished with from_screen = true within 2 s of settling */ }
#[tokio::test] async fn e2e_restart_reconnects() {
    /* start agents a and b → both Idle; GiveWork a "slow task" (runs 3 s); handle.shutdown().await (aborts the listener and core tasks,
       stops no agent); a's turn completes while closed, so its busy/turn-end hooks land in pending_hooks; start_system again on the same Paths →
       first Snapshot has reconnecting == 2, then Reconnected; a Finished with its last reply, b still Idle; both windows are the same @ids */
}
#[test] fn args_parse() { /* "hook a busy" → Hook; "bench --agents 10 --json" → Bench; "--help" → usage; "frob" → Err */ }
#[test] fn logging_writes_to_logs_dir_only() { /* init_logging(temp paths), tracing::info!("hello") then drop the guard → one file under logs/ contains "hello"; nothing else under the data dir changed */ }
```
- [x] **Step 2: Run, see FAIL.** `cargo test -p duohive --test e2e`
- [x] **Step 3: Implement.**
- [x] **Step 4: Run the full gate.**
  - `cargo fmt --all --check` → no output
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` → no warnings
  - `cargo test --workspace --all-features` → PASS
  - `PATH=$HOME/.local/tmux-3.2a/bin:$PATH cargo test -p duohive-tmux -p duohive --all-features` → PASS on tmux 3.2a
  - Manual smoke: `DUOHIVE_CONFIG_DIR=$(mktemp -d) DUOHIVE_DATA_DIR=$(mktemp -d) DUOHIVE_TMUX_SOCKET=duohive-smoke cargo run -p duohive` → empty org screen; `n` adds a role and an agent; `s` starts it; `q` quits; `tmux -L duohive-smoke ls` shows the session still running; reopening shows `Reconnecting to 1 agent…`, then the same state.
- [x] **Step 5: Commit.** `git add -A && git commit -m "feat: wire start-up, subcommands and end-to-end tests"`

### Task 30: Phase 1 acceptance — the done-when signal

**Files:**
- Create: `docs/plans/bench-results.md`
- Modify: `docs/plans/v1-roadmap.md` (Phase 1 row: add "done" and the date), `docs/design/technical-design.md` (any mechanism that changed during Phase 1)

**Interfaces:**
- Consumes: the built binary.

- [x] **Step 1: Run the bench on the target machine.** `cargo build --release -p duohive && ./target/release/duohive bench --agents 10` on Apple M-series; record the output, the machine, macOS and tmux versions, and the commit hash in `docs/plans/bench-results.md` under "Phase 1". Record a Linux run if one is available (reported, not gating).
- [x] **Step 2: Run the author's daily org.** Move the author's daily agents into `org.toml` (by form or by hand), run them for a working day, and check each against the state table: starting → working → finished/idle, waiting on an approval, `~` only on screen-read transitions, quiet after 5 minutes of silence, errored on a crash, stopped on `s`, and correct state after closing and reopening DuoHive. Log every wrong status as an issue with the agent's CLI version and the hook fixture that reproduces it; fix those before marking the phase done.
- [x] **Step 3: Mark the phase.** Update the roadmap's Phase 1 row and the TD where needed.
- [x] **Step 4: Commit.** `git add -A && git commit -m "docs: phase 1 bench results and acceptance"`


---

## Self-review notes

- **Spec coverage:** roadmap Phase 1 core domain → 6–8, engine (command/event loop, full state machine, Give work by state, quiet, Needs you) → 9–15, store (`org.toml`, `settings.toml`, `state.db`) → 16–17, tmux runtime → 18–19, adapters (Claude Code, OpenCode) and `duohive hook` → 20–22, minimal TUI with live status and forms → 23–25, real ports → 26, fake CLI and headless system → 27, basic `bench` → 28, wiring → 29, done-when → 30. TD §8 rows in scope: CLI missing/untested (11), launch fails (11), crash (12), Duo fails (12, 23), tmux missing (23, 29), `org.toml` invalid (14, 16, 23), directory missing (8, 11, 23), hook silent → `~` then quiet (12–13). Restart and reconnect: 15, 23 (`Reconnecting to N agents…`), 27 (`shutdown`, `EnrichingStore`), 29. Out of scope by phase: worktrees, Give work dialog, agent pane, alerts (2); MCP, inbox, Duo prompt, cards, task to a role (3); templates, model picker, key editor, `?` map, mouse, and TD §8's "No agent CLI installed" full-screen message, which belongs to first-run setup (4; D28). Out of v1: a Codex adapter and its spike (deferred to after v1).
- **Size:** 25 tasks (Tasks 6–30).
- **Proportion:** signatures, rules and tests only; bodies appear only for the launch script and the bench output format, which the specs and signatures don't determine.

# DuoHive Phase 2 Implementation Plan — One screen

**Status:** Ready 2026-10-05 · not started.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One screen shows the org, the selected agent's live terminal and a Duo pane, so you never need `tmux attach`.
- **The agent pane:** you can watch the selected agent's terminal, type into it and zoom it.
- **Needs you:** you can act on it.
- **Alerts:** desktop alerts fire.
- **Give work:** it offers a fresh git worktree, which is removed once its branch is merged.
- **Friction log:** kept through the phase, it drives a UI/UX improvement pass at the end.

**Done when (roadmap row 2):** a day's work needs no `tmux attach`.

**Architecture:** The layers stay as in phases 0–1 (TD §3).
- **New ports in `duohive-core`:**
  - `Notifier` and `Git`, which TD §3.1 already lists;
  - `PaneView` (P2-D1).
- **The engine gains:**
  - alerts;
  - worktrees;
  - Give work with a "Run" choice.
- **All git work is fire-and-forget.** Results return as inputs, so the core never blocks on git (TD §3.3).
- **The live terminal bypasses the engine.** The TUI reads it through `PaneView`, which `duohive-tmux` implements over the path spike S3 picks. The binary wires `PaneView` into the TUI, as it already does for `DirProbe`, which keeps the dependency rule.

**Tech stack:** as phases 0–1:
- Rust 1.99.0 and tokio;
- ratatui `=0.30.2` and crossterm `=0.29.0`;
- rusqlite, insta and tempfile.

New in this phase:
- **`vt100 = "=0.16.2"`:** the latest on crates.io (checked 2026-10-05), for the terminal widget;
- **the `git` binary, 2.20 or newer**, for `git worktree`;
- **`osascript` (macOS) and `notify-send` (Linux)**, both detected at runtime.

**Spec:**
- [PRD](../product/prd.md): One screen; Lifecycle: Worktrees; UX principles; Delivery Phases row 2.
- [UI/UX spec](../ux/ui-spec.md): §2–§5 and §8.
- [Technical design](../design/technical-design.md): §3, §4.2, §4.3, §5.1, §5.3 and §7–§9.
- [Roadmap](v1-roadmap.md): row 2.
- [Roadmap: Planning](v1-roadmap.md#planning) and [traceability](v1-traceability.md).

## Global Constraints

- **Everything in [phase-0-plan.md](phase-0-plan.md#global-constraints) Global Constraints still holds:**
  - toolchain `1.99.0`, edition 2024, Apache-2.0;
  - the dependency rule;
  - `duohive-core` does no I/O;
  - no `unwrap`/`expect` outside tests;
  - no `unsafe`, except `bench/measure.rs`;
  - no `std::env::set_var` in tests;
  - tmux tests run on a throwaway socket, never on `duohive`;
  - conventional commits, with no `Co-Authored-By` line, and never push.
- **Phase 1 decisions D1–D32 ([phase-1-plan.md](phase-1-plan.md)) stay in force.** Four change here:
  - D7 (the phase-1 layout) is replaced by P2-D3;
  - D8 (the phase-1 keys) is extended by P2-D9;
  - D15 is done here (alerts arrive in phase 2);
  - D16's "dialog arrives in phase 2" is done here.
- **Every commit passes `make check`:** format, clippy `-D warnings`, tests, `cargo deny` and `typos`.
  - A task that changes a shared type updates every construction and match site in the same commit; its Files list names them.
  - New `Event` variants are added in Task 2.2 and ignored by the TUI until the task that renders them.
  - New `Ports` fields start as the null implementations from Task 2.2.
- **States and glyphs are exactly UI §2.1:**
  - `○ stopped`, `◌ starting`, `◆ waiting`, `✓ finished`, `◦ idle`, `✕ errored`;
  - `● working`, shown as a spinner `◐◓◑◒` at 4 fps on on-screen rows only, and static with Reduce motion;
  - the overlay `◷ quiet`;
  - the marker `~`.
- **Layout is UI §3:**

  | Terminal size | Org pane | Agent pane | Chat pane |
  |---|---|---|---|
  | ≥ 120×36 | 50 cols, full tree with replies | the rest | 6 rows, 12 while typing |
  | 100–119 × 30 | 50 cols | the rest | 6 rows |
  | 80–99 × 24 | 32 cols, compact tree | the rest | 4 rows |
  | < 80×24 | the too-small screen with the Needs you count | | |

  Mixed width and height use the smaller row (P2-D19).

  - **Shrink order:** agent-pane detail goes first, then last-reply lines, then connectors.
  - **Tab order:** org → Needs you → agent → chat. Needs you is always a `tab` stop.
  - **Borders:** the focused pane is drawn heavy (`┏━┓`), the others light.
- **Keys are UI §4.11.**
  - `esc` never quits.
  - Global letter keys (`q`, `w`, `,`, `z`, `s`, …) do nothing while the chat input, a form or Typing mode has focus; the key types there instead (UI §4.11, §5).
- **Copy:**
  - sentence case;
  - agent names as monospace identifiers;
  - "Give work" is the only term for handing over a task;
  - colour is never the only signal;
  - no feature descriptions on screen (UI §1).
- **Worktrees (TD §5.3):**

  | Item | Value |
  |---|---|
  | Path | `<data_dir>/worktrees/<agent>/<task-id>` |
  | Branch | `duohive/<agent>/<task-id>` |
  | Base | `origin`'s default branch, else the local default (P2-D17), fetched quietly before each check |
  | Merges detected | normal, rebase and squash |
  | Checked | when the agent finishes, after start-up (after the first frame), when the list opens, every 10 minutes, and when its agent stops (P2-D6) |
  | Removed automatically | only when merged **and** clean **and** not in use |

- **Alerts (TD §4.2, UI §4.1.4):**
  - **Sent when** an agent enters waiting, errored or quiet, or an agent other than Duo enters finished.
  - **Never sent** for Duo finishing.
  - **Title:** `<agent> needs you` or `<agent> finished`.
  - **Body:** the last reply, or empty when there is none.
  - **Toggles:** `settings.alerts.needs_you` and `settings.alerts.finished`.
  - **No notifier:** the footer says `desktop alerts unavailable`, once per session.
  - **A click** selects the agent where the notifier reports clicks (Linux `notify-send --action`); it doesn't raise the terminal window (P2-D18).
- **Snapshots (TD §9):** every screen this plan adds or changes gets `insta` snapshots at 80×24 and 160×48. A test name ending `_80x24` / `_160x48` stands for both sizes.
- **Test-first applies** to every task except 2.1 (spike), 2.19 (acceptance) and 2.20 (UX pass). Those are validated as each task states.

## Review Focus

1. **Typing mode keeps every key, and `ctrl+q` always gets you out.**
   - While typing, `q`, `esc`, `tab`, `ctrl+c`, `?` and `space` all go to the agent; only `ctrl+q` leaves.
   - Keys go to the stream that was opened, never to a newer window for the same agent (P2-D1). If that window dies, Typing mode ends with a footer note.
   - Tests: Task 2.7 `typing_forwards_every_key_but_ctrl_q`, `typing_ends_when_window_dies`; Task 2.3 `send_to_stale_stream_fails`.
2. **Worktree removal never loses work.**
   - A dirty or unmerged worktree is never removed automatically.
   - Neither is one an agent's live session runs in. That rule survives a restart, because the session row stores the worktree.
   - A path outside `<data_dir>/worktrees/` is never removed.
   - "Unknown" never counts as "merged".
   - A branch with no commits beyond its base is never "merged".
   - Tests: Task 2.13 `never_removes_dirty_unmerged_or_in_use`, `in_use_survives_restart`; Task 2.14 `remove_refuses_paths_outside_worktrees_dir`, `offline_fetch_uses_local_refs`, `fresh_branch_is_not_merged`.
3. **Watching one agent costs nothing for the rest.**
   - Holding `↓` across 100 agents opens at most one live stream at a time (P2-D4).
   - The once-a-second activity poll keeps running, and the other panes stay silenced.
   - Tests: Task 2.3 `one_stream_at_a_time`, `other_panes_stay_silenced`; Task 2.18 `bench_with_live_view`.
4. **No alert storms.**
   - One alert per agent per state change.
   - None for changes replayed at start-up.
   - None of a kind that is turned off.
   - Tests: Task 2.9 `no_alerts_during_reconnect_replay`, `one_alert_per_state_change`, `disabled_kind_not_sent`.
5. **Hostile task text in names and paths.**
   - A task of `/`, `..`, emoji, 2 000 characters or only whitespace gives a safe slug (`task` when empty) inside the worktrees dir.
   - An existing branch is never reused or reset (P2-D7).
   - Tests: Task 2.11 `task_slugs_are_safe`; Task 2.12 `existing_branch_takes_next_suffix_then_errors`; Task 2.14 `existing_branch_is_an_error_not_reused`.

---

## File Structure

```
crates/duohive-core/src/
  view.rs                  PaneSize, PaneInput, PaneKey, PaneChunk, StreamId, PaneStream, ViewError   (new, 2.2)
  alert.rs                 AlertKind, Alert (2.2); alert_for (2.9)                                   (new)
  worktree.rs              TaskId, BaseRef, MergeState, MergeKind, RunIn (2.2); slugs, ids, WorktreeRow (2.11) (new)
  ports.rs                 + PaneView, Notifier, Git, NoNotifier, NoGit (2.2); Store worktree methods (2.11)
  command.rs  event.rs  launch.rs  status.rs
  engine/{mod,lifecycle,signals,timers,reconnect,org_edit}.rs   engine/alerts.rs  engine/worktrees.rs (new)
  engine/harness.rs  testkit.rs                                 fakes for every new port
crates/duohive-tmux/src/view.rs                       PaneView over the S3 path (new)
crates/duohive-tmux/tests/view.rs                     (new)
crates/duohive-store/src/{db.rs,sqlite_store.rs,paths.rs}     migration 3, worktree rows, worktrees_dir
crates/duohive-cli/src/{lib.rs,claude.rs,opencode.rs,fake.rs,hooks_json.rs}  LaunchSpec.cwd
crates/duohive-tui/src/
  focus.rs  term_view.rs  keys_out.rs                                                               (new)
  render/{agent_pane,chat_pane,zoom,worktrees,settings}.rs                                          (new; side_pane.rs removed)
  forms/give_work.rs                                                                                 (new)
  model.rs  input.rs  app.rs  tree.rs  render/{mod,header,needs_you,dialog}.rs
crates/duohive/src/
  notifier.rs  git/{mod,merge}.rs                                                                    (new)
  system.rs  system/enriching_store.rs  startup.rs  real_ports.rs  args.rs  bench/mod.rs
crates/duohive/tests/{e2e.rs,git.rs,bench.rs}
docs/plans/spike-report.md (S3) · docs/plans/friction-log.md (new) · docs/plans/bench-results.md (Phase 2)
```

## Decisions this plan makes where the specs are silent

| # | Decision |
|---|---|
| P2-D1 | **The live terminal bypasses the engine.** A `PaneView` port in `duohive-core` (traits and data only) is implemented in `duohive-tmux` and handed to the TUI by the binary as `Arc<dyn PaneView>`. Raw bytes never pass through the core's single task. Each `open` returns a `StreamId` bound to the window open at that moment. `send` and `resize` take that id, and they fail with `ViewError::Closed` once that window is gone, so keys never reach a newer window. The engine still receives `ViewAgent`. Task 2.1 adds `PaneView` to TD §3.1 and the S3 path to TD §4.3. |
| P2-D2 | **Spike S3 picks the live-view path** from four candidates: (a) a second control-mode client with `no-output`, enabling only the viewed pane with `refresh-client -A '%<pane>:on'`, if tmux honours that; (b) a second client without `no-output`, with every other pane `off` or paused (TD §4.3's "`!no-output` with the other panes paused"); (c) `pipe-pane -O` to a FIFO under the run dir; (d) `capture-pane -e -p` re-read on a 50 ms timer while the pane is viewed, sending only changed frames. The first frame is always `capture-pane -e -p` plus `display -p '#{cursor_x} #{cursor_y}'`. |
| P2-D3 | **The phase-2 layout is UI §3 in full** and replaces D7. The side pane's summaries move into the agent pane in UI §4.1.2's shape, with phase-2 copy where a feature is still missing:<br>• **Role summary:** the last line is `e edit`; `space give work to this role · e edit` returns in 3.11.<br>• **Empty org and empty role:** they keep D7's text, without "ask Duo"; it returns in 3.9.<br>• **Stopped agent:** `○ stopped · space to give work`, as the spec.<br>• **The attach hint is dropped.**<br>• **Chat-pane resizing** (`ctrl+↑`/`ctrl+↓`) is clamped between the size UI §3 gives the chat and the size that leaves the agent pane 6 rows. |
| P2-D4 | **One live stream at a time.** The stream opens 100 ms after the selection settles on a running agent, and closes when the selection moves or the agent pane is hidden. A move within those 100 ms cancels the pending open. |
| P2-D5 | **The viewed window is resized** to the agent pane's inner size when it is viewed, zoomed or restored (`resize-window`, verified by S3.5). A window that stops being viewed keeps its last size. |
| P2-D6 | **"In use"** (the extra check when an agent stops is proposed to the author. If they approve, Task 2.20 edits the PRD Lifecycle "checked" list and UI §4.9. If they decline, the stop trigger is dropped from 2.13 and the 10-minute check covers it) means the agent is running and its current session was launched in that worktree. `SessionRow.worktree` records it, so a restart keeps it. In-use worktrees are never removed, automatically or by hand: `d` shows `<agent> is running in it · stop it first`. A check runs when the owning agent stops. This pins the PRD's "keeps it while the work is in progress", and Task 2.13 edits TD §5.3 to say so. |
| P2-D7 | **Task ids and branches.** The slug is D11's slug of the task's first line, cut to 32 characters, or `task` when empty. The task id is the slug (cut to 28 characters when a suffix is needed, with a trailing `-` stripped), made unique among this agent's worktree rows with `-2`, `-3`, and so on. The branch is `duohive/<agent>/<task-id>`. When git reports that the branch already exists (a kept unmerged branch, or anything else), the engine tries the next suffix, up to 9 attempts. After that, the row error is `couldn't create worktree: branch exists · space`. An existing branch is never reused or reset. |
| P2-D8 | **The phase-2 chat pane.** It is drawn at UI §3 sizes, titled `⬢ Duo · <state>`, with an input.<br>• **`enter`** sends the line as `GiveWork { agent: Duo, run: Directory }`.<br>• **A refusal** (Duo starting, working or waiting) shows `Duo is <state word> · send again when it is at rest`, and the text stays in the input.<br>• **`GiveWorkNotTaken` for Duo** puts the text back in the input with P2-D11's note.<br>• **History:** in-memory `you` lines, and Duo's replies taken at each Duo turn end (`AgentStatusChanged` to idle, with the reply current then), stamped like `10:05`.<br>A line starting with `@agent` goes to Duo as written, since routing arrives with the inbox. Phase 3 (3.9) replaces this pane. |
| P2-D9 | **Phase-2 keys:**<br>• `tab`/`shift+tab` move focus.<br>• `space` gives work, in the org pane and the agent pane.<br>• `enter` in the org pane: on an agent or Duo it focuses the agent pane; on a team or role it folds.<br>• `←`/`→` fold and unfold; `z` zooms; `w` opens worktrees; `,` opens Settings.<br>• `ctrl+↑`/`ctrl+↓` resize the chat.<br>• Needs you: `↑↓` and `enter`.<br>• Agent pane: `enter`/`ctrl+q`, `PgUp`/`PgDn`.<br>Still phase 4: `?`, `f`, `m`, the mouse and the key-binding editor. Footers leave out `? help` until phase 4. |
| P2-D10 | **Phase-2 Give work goes to agents only.**<br>• **`space` on a role:** shows `select an agent to give work`.<br>• **On a team, or on Duo (in the tree or the agent pane):** shows `space gives work to an agent or a role`, as UI §4.5 has it.<br>• **The busy dialog** reads `<agent> is <state word>. Give work when it is at rest.`, with Give work disabled.<br>Task to a role and **Send as message instead** arrive in 3.11. |
| P2-D11 | **A typed-in task counts as taken only when the agent leaves rest.** This narrows the PRD's "finished or idle: types the task in" for the few seconds a delivery is pending; it is listed for the author in the hand-off. This changes TD §4.2's row "finished, idle → New work → working". The paste no longer fires `NewWork`. A `busy` or prompt-submit hook does, or a `~` busy read for hookless coverage.<br>• **Until then:** the agent stays finished or idle, with `delivery_pending`. A further Give work is refused as `GiveWorkRefused { state, pending: true }`, shown as `<agent> is still taking the last task`.<br>• **No sign within 5 s:** the engine publishes `GiveWorkNotTaken { agent, task, reason: "no sign it started" }`. `RuntimeEvent::PasteFailed` gives the same event, with its reason.<br>• **The TUI** shows `<agent> didn't take the task · space to try again`, and Give work opens pre-filled (Duo: P2-D8).<br>Task 2.12 edits TD §4.2, §5.1 and §6.3. Phase 3's inbox delivery uses the same rule. |
| P2-D12 | **Scrolling back.** `PgUp` freezes a snapshot (`capture-pane -e -p -S -<n>`, at most 2 000 lines), and the pane title gains `· scrolled`. `PgDn` past the bottom, or `enter` to type, returns to live. |
| P2-D13 | **Settings moves into phase 2** because alerts ship here, and the PRD, TD §4.2 and UI §4.1.4 say each alert kind "can be turned off in Settings". The `,` screen has UI §4.10's You, Attention and Look groups. The Keys and Org rows arrive in phase 4. Theme `auto` keeps phase 1's rendering, dark, until 4.9. Roadmap row 2 and UI §8 item 2 were edited to match on 2026-10-05. |
| P2-D14 | **No alerts for replayed history.** From `start_up` until `Reconnected`, state changes publish no `Alert`. |
| P2-D15 | **`z` on a team or role** shows `z zooms an agent`. `z` on any agent zooms it, whatever its state: a stopped agent's zoomed pane shows its stopped line. |
| P2-D16 | **Worktree states.** A row is `Unchecked` until its first check. `InUse` takes precedence over every merge result. `Merged` (clean, not in use) is the instant before `Removing`. Two state words that UI §4.9 doesn't draw are decided here: `not checked yet` and `couldn't check · <reason>`. |
| P2-D17 | **Base and fresh branches.** With an `origin` remote, the base is `origin`'s default (from `refs/remotes/origin/HEAD`, else `git ls-remote --symref origin HEAD`). Without one, or offline with neither known, the local default is the first that exists of `init.defaultBranch`, `main`, `master`; with none, the check is `CheckFailed("no default branch")`. Each row stores `created_from`, the base commit its branch was created on. A branch with no commits after `created_from` is `NotMerged { files_changed: 0 }`, never merged. This test runs before the merge tests, and a merged branch always has the agent's commits after `created_from`, so normal merges are still detected. |
| P2-D18 | **An alert click selects the agent but doesn't raise the terminal window.** UI §4.1.4 says a click "focuses the terminal and selects the agent". Neither notifier can name the window that runs DuoHive, so raising it is left out, and Task 2.20 edits UI §4.1.4 to match. |
| P2-D19 | **Mixed sizes in UI §3.** Width and height each pick a row of the layout table, and the smaller row applies: 130×30 uses the 100–119 × 30 row, and 110×24 the 80–99 × 24 row. A size below 80 wide or 24 tall gets the too-small screen. |

---

## Tasks

### Task 2.1: Spike S3 — a live view of one pane at 100 agents

This is a spike: the code is throwaway, under `spikes/s3/`, with no code under test.

**Files:**
- Create `spikes/s3/`.
- Modify `docs/plans/spike-report.md` (new section S3) and `docs/design/technical-design.md` (§3.1 ports, §4.1 source 2, §4.3 "No pane streams").

- [ ] **Step 1: Build each P2-D2 candidate.** Run each against 100 noisy windows (S0's generator) on tmux 3.2a and the latest tmux, and record:

  | # | Measure |
  |---|---|
  | S3.1 | Keystroke-to-echo latency, p50 and p95, over 200 `send-keys -l` keys |
  | S3.2 | The extra CPU of viewing one noisy pane (reader plus server) over S0's baseline |
  | S3.3 | That the other 99 panes stay silent on the main connection |
  | S3.4 | Colour, cursor and alternate-screen fidelity in `vt100` `=0.16.2`, on Claude Code and OpenCode screens recorded live; save the byte captures as fixtures for Task 2.4 |
  | S3.5 | That `resize-window` on the viewed window works with the server's `window-size` setting, and that the CLI redraws |
  | S3.6 | The cost of `capture-pane -e -S -2000` |
  | S3.8 | Whether Claude Code shows its workspace-trust dialog in a new worktree path; if so, note it as a phase-2 risk for Task 2.19 |

- [ ] **Step 2: Choose.** Pick the candidate with p95 echo under 50 ms and the lowest S3.2. Record the choice and its exact tmux commands as S3.7.
- [ ] **Step 3: Update the TD** with P2-D1 and P2-D2. Validation: every S3 subsection holds measured values and names its log.
- [ ] **Step 4: Commit.** `docs: spike S3 live pane view`

### Task 2.2: Core — new ports, shared types, null implementations and fakes

**Files:**
- Create `crates/duohive-core/src/{view.rs, alert.rs, worktree.rs}`.
- Modify:
  - core: `ports.rs`, `command.rs`, `event.rs`, `lib.rs`, `engine/mod.rs` (`Ports`), `engine/harness.rs`, `testkit.rs`;
  - `crates/duohive/src/system.rs` (`Ports {` gets `NoNotifier`, `NoGit`);
  - every `Command::GiveWork` site: `crates/duohive-core/src/{event.rs, engine/harness.rs, engine/mod.rs, engine/lifecycle.rs}`, `crates/duohive/src/bench/mod.rs`, `crates/duohive/tests/e2e.rs`;
  - every `GiveWorkRefused` site (it gains `pending`): `crates/duohive-core/src/{event.rs, engine/lifecycle.rs, engine/lifecycle/tests.rs}`, `crates/duohive-tui/src/{model.rs, input/tests.rs}`;
  - `crates/duohive-tui/src/model.rs` (the new `Event` arms do nothing for now).

**Interfaces — Produces:**

```rust
// view.rs
pub struct PaneSize { pub cols: u16, pub rows: u16 }
pub struct StreamId(pub u64);
pub enum PaneInput { Text(String), Key(PaneKey), Paste(String) }             // Paste = bracketed
pub enum PaneKey { Enter, Tab, BackTab, Backspace, Delete, Escape, Up, Down, Left, Right, Home, End,
                   PageUp, PageDown, Insert, F(u8), Ctrl(char), Alt(char) }
pub enum PaneChunk { Reset { bytes: Vec<u8>, cursor: (u16, u16) }, Bytes(Vec<u8>), Closed { reason: String } }
pub struct PaneStream { pub id: StreamId, pub rx: tokio::sync::mpsc::Receiver<PaneChunk> }
#[derive(thiserror::Error)] pub enum ViewError { #[error("{0} is not running")] NotRunning(AgentName),
                                                 #[error("stream closed")] Closed, #[error("{0}")] Tmux(String) }
// alert.rs
pub enum AlertKind { NeedsYou, Finished }
pub struct Alert { pub agent: AgentName, pub kind: AlertKind, pub title: String, pub body: String }
// worktree.rs
pub struct TaskId(pub String);
pub struct BaseRef(pub String);                                               // "origin/main" or "main"
pub enum MergeKind { Normal, Rebase, Squash }
pub enum MergeState { Merged(MergeKind), NotMerged { files_changed: u32 }, Unknown(String) }
pub enum RunIn { Directory, NewWorktree }
// ports.rs
pub trait PaneView: Send + Sync {
    fn open(&self, agent: &AgentName, size: PaneSize) -> Result<PaneStream, ViewError>;  // opening one closes any other
    fn resize(&self, stream: StreamId, size: PaneSize) -> Result<(), ViewError>;
    fn send(&self, stream: StreamId, input: PaneInput) -> Result<(), ViewError>;
    fn scrollback(&self, stream: StreamId, lines: u16) -> Result<tokio::sync::oneshot::Receiver<Vec<u8>>, ViewError>;
}
pub trait Notifier: Send { fn available(&self) -> bool; fn notify(&mut self, alert: &Alert); }
pub trait Git: Send { fn create(&mut self, req: CreateWorktree); fn check(&mut self, req: CheckWorktree); fn remove(&mut self, req: RemoveWorktree); }
pub struct CreateWorktree { pub agent: AgentName, pub task: TaskId, pub repo: PathBuf, pub path: PathBuf, pub branch: String }
pub struct CheckWorktree  { pub agent: AgentName, pub task: TaskId, pub repo: PathBuf, pub path: PathBuf, pub branch: String,
                            pub created_from: String }                     // the base commit the branch was created on (P2-D17)
pub struct RemoveWorktree { pub agent: AgentName, pub task: TaskId, pub repo: PathBuf, pub path: PathBuf, pub branch: String,
                            pub delete_branch: bool, pub force: bool }
pub enum GitError { BranchExists, NotARepo, Other(String) }
pub enum GitEvent {
    Created { agent: AgentName, task: TaskId, result: Result<(BaseRef, String), GitError> },   // (base, base commit id)
    Checked { agent: AgentName, task: TaskId, merge: MergeState, dirty_files: u32, base: Option<BaseRef> },
    Removed { agent: AgentName, task: TaskId, result: Result<(), String> },
}
pub struct NoNotifier;  impl Notifier for NoNotifier { /* available() = false */ }
pub struct NoGit;       impl Git for NoGit { /* answers nothing; used until 2.14 */ }
// engine/mod.rs: Ports gains `notifier: Box<dyn Notifier>`, `git: Box<dyn Git>`; Input gains `Git(GitEvent)`, `AlertClicked { agent }`
// command.rs:    GiveWork { agent, task, run: RunIn }
// event.rs:      Alert(Alert), AlertsUnavailable, AlertClicked { agent }, GiveWorkNotTaken { agent, task, reason },
//                Worktrees(Vec<WorktreeRow>) (WorktreeRow is a placeholder struct here, filled in by 2.11);
//                GiveWorkRefused gains `pending: bool`; AgentView gains `worktree: Option<TaskId>` (None until 2.12 sets it)
```

**Testkit:**
- `FakePaneView` records calls and lets a test push chunks or close a stream.
- `FakeNotifier { available }` records alerts.
- `FakeGit` records `GitCall::{Create(CreateWorktree), Check(CheckWorktree), Remove(RemoveWorktree)}`; tests answer each with a `GitEvent`.

- [ ] **Step 1: Write the failing tests.**
  - `fakes_record_calls`: each fake returns its calls in order, for example `FakeGit` after `create` then `check` → `[Create(..), Check(..)]`.
  - `null_ports_do_nothing`: `NoNotifier.available()` is false, and `NoGit` produces no input.
  - Round trips through `serde_json`, equal after decoding: `git_events_round_trip_json`, `pane_types_round_trip_json`, `new_events_round_trip_json`.
  - `give_work_defaults_to_directory_at_call_sites`: compiles, and the existing e2e still passes.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features ports`
- [ ] **Step 3: Implement.** Add the types, the null implementations, the fakes and the call-site updates.
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: pane view, notifier and git ports`

### Task 2.3: tmux — `PaneView` over the S3 path

**Files:**
- Create `crates/duohive-tmux/src/view.rs` and `crates/duohive-tmux/tests/view.rs`.
- Modify `lib.rs`.

**Interfaces:**
- Consumes `PaneView` (Task 2.2) and the S3.7 commands (Task 2.1).
- Produces `pub struct TmuxPaneView` with `TmuxPaneView::new(cfg: &TmuxConfig) -> Result<Self, TmuxError>`, which implements `PaneView`.

**Behaviour:**
- **`open`:**
  1. remembers the agent's current window id under a new `StreamId`;
  2. sends `Reset` (the screen and the cursor);
  3. sends `Bytes` as the pane writes;
  4. sends `Closed` when that window dies or the stream is dropped.
- **Opening another stream closes the first.**
- **`send`:**

  | `PaneInput` | tmux command |
  |---|---|
  | `Text` | `send-keys -l` |
  | `Key` | tmux key names: `Enter`, `BTab`, `C-x`, `M-x`, `F1`… |
  | `Paste` | `load-buffer` + `paste-buffer -p` |

  Each targets the stream's window id, never the agent's name.

- [ ] **Step 1: Write the failing integration tests.** They run on a throwaway socket, with `sh` loops as fake agents.
  - `open_gives_reset_then_bytes`: `printf 'hi\n'` arrives as `Bytes` within 200 ms.
  - `send_text_and_keys_reach_the_pane`: `cat -v` echoes `a`, `^C` and `^[[A`.
  - `paste_is_bracketed`: the fake pane first turns on bracketed paste (`printf '\e[?2004h'`), then receives `\e[200~`…`\e[201~`.
  - `one_stream_at_a_time`: a second `open` → the first gets `Closed`.
  - `other_panes_stay_silenced`: the main connection sees no `%output` for the other windows.
  - `resize_changes_window_size`: `#{window_width}` follows.
  - `scrollback_returns_history`: after 2 000 lines, the result holds line 1.
  - `closed_when_window_dies`: `kill-window` → the stream's last chunk is `Closed`, and the receiver then ends.
  - `send_to_stale_stream_fails`: kill the window and start a new one for the same agent → `send` on the old id returns `Err(Closed)`, and the new pane receives nothing.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tmux --test view`
- [ ] **Step 3: Implement** with the S3.7 commands.
- [ ] **Step 4: Run, see PASS** on the latest tmux and on 3.2a: `PATH=$HOME/.local/tmux-3.2a/bin:$PATH cargo test -p duohive-tmux --test view`
- [ ] **Step 5: Commit.** `feat: live pane view over tmux`

### Task 2.4: TUI — the terminal widget

**Files:**
- Create `crates/duohive-tui/src/term_view.rs`.
- Fixtures go in `crates/duohive-tui/tests/fixtures/term/`, copied from the S3.4 captures.
- Modify the workspace `Cargo.toml` (`vt100 = "=0.16.2"`), `crates/duohive-tui/Cargo.toml` and `lib.rs`.

**Interfaces — Produces:**

```rust
pub struct TermView { parser: vt100::Parser }
impl TermView {
    pub fn new(size: PaneSize) -> Self;
    pub fn apply(&mut self, chunk: &PaneChunk);                // Reset clears, processes the bytes, sets the cursor
    pub fn resize(&mut self, size: PaneSize);
    pub fn render(&self, area: Rect, buf: &mut Buffer, show_cursor: bool, no_color: bool);
}
```

**Behaviour:**
- Each terminal cell maps to one Ratatui cell: foreground, background, bold, italic, underline and inverse.
- A wide character takes two cells.
- `no_color` drops colours but keeps inverse.

- [ ] **Step 1: Write the failing tests.**
  - Snapshots: `claude_idle_80x20`, `opencode_approval_80x20`, `colours_and_wide_chars`.
  - `reset_replaces_screen`: after `Reset` the old text is gone.
  - `cursor_shown_only_when_asked`.
  - `no_color_keeps_inverse`: the cursor cell keeps `Modifier::REVERSED`, with no `fg`/`bg`.
  - `resize_keeps_bottom_lines`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui term_view`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `cargo test -p duohive-tui term_view`
- [ ] **Step 5: Commit.** `feat: terminal widget for the agent pane`

### Task 2.5: TUI — three panes, focus, folding and the phase-2 chat pane

**Files:**
- Create `crates/duohive-tui/src/focus.rs`, `render/agent_pane.rs` (moving the summaries out of `side_pane.rs`, which is deleted) and `render/chat_pane.rs`.
- Modify `render/mod.rs`, `render/header.rs`, `model.rs`, `input.rs` and `tree.rs`.

**Interfaces — Produces:**

```rust
pub enum Pane { Org, NeedsYou, Agent, Chat }
pub enum AgentMode { Selected, Typing }
pub struct Focus { pub pane: Pane, pub agent_mode: AgentMode, pub needs_you_cursor: usize }
impl Focus { pub fn next(&self) -> Pane; pub fn prev(&self) -> Pane }
pub struct Layout { pub org: Rect, pub needs_you: Rect, pub agent: Rect, pub chat: Rect, pub compact: bool }
pub fn layout(area: Rect, chat_rows_extra: i16, chat_typing: bool) -> Option<Layout>;   // None below 80×24
pub enum Who { You, Duo }
pub struct ChatLine { pub at: Millis, pub who: Who, pub text: String }
pub struct ChatLines { pub lines: Vec<ChatLine> }    // P2-D8
```

**`UiModel` gains:**
- `focus`;
- `folded: BTreeSet<NodeKey>`;
- `chat_rows_extra: i16` (clamped per P2-D3);
- `chat: ChatLines`;
- `chat_input: LineEdit`, reusing `forms/line_edit.rs`.

**Footers, by focused pane:**

| Focus | Footer |
|---|---|
| Org | `tab pane · ↑↓ move · space give work · s start/stop · n new · e edit · z zoom` |
| Needs you | `↑↓ move · enter go to agent` |
| Agent (Selected mode) | `enter type · z zoom · PgUp/PgDn scroll · space give work` |
| Chat | `enter send · alt+enter new line · tab pane` |

**Folding:**
- `←` folds and `→` unfolds; `enter` on a team or role toggles.
- The compact tree shows `▾` and `▸`, and the count of a folded group (`▸ QA (1)`).

- [ ] **Step 1: Write the failing tests.**
  - Snapshots matching UI §4.1 and its compact mock, with the agent pane holding a summary: `main_80x24`, `main_100x30`, `main_120x36`, `main_160x48`.
  - Focus:
    - `tab_cycles_org_needs_you_agent_chat`, including an empty Needs you;
    - `shift_tab_reverses`;
    - `focused_pane_has_heavy_border`.
  - Layout and size:
    - `chat_grows_to_12_rows_while_typing_at_120x36`;
    - `ctrl_arrows_resize_chat_within_p2_d3_bounds`;
    - `below_80x24_shows_too_small_with_needs_you_count`;
    - `shrink_order_drops_replies_then_connectors`;
    - `mixed_sizes_take_the_smaller_row`: 130×30 gives a 50-column org and a 6-row chat; 110×24 gives a 32-column org and a 4-row chat.
  - Folding and footer:
    - `fold_hides_children_and_shows_count`;
    - `footer_matches_focused_pane`.
  - Chat pane:
    - `chat_enter_gives_work_to_duo`: sends `GiveWork { agent: Duo, run: Directory }`;
    - `chat_refused_keeps_text_and_notes`: Duo waiting → `Duo is waiting · send again when it is at rest`;
    - `chat_not_taken_returns_text`;
    - `letter_keys_type_in_chat`: `q`, `w`, `,`, `z` and `s` with the chat focused go into the input, and none quits or opens anything.
  - Copy: `role_summary_has_phase_2_footer`, `empty_org_keeps_d7_text`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `cargo test -p duohive-tui`. The old side-pane snapshots are replaced; the reviewer checks the diff against UI §4.1.2 and P2-D3.
- [ ] **Step 5: Commit.** `feat: three-pane layout, focus and folding`

### Task 2.6: TUI — Needs you you can act on

**Files:**
- Modify `render/needs_you.rs`, `input.rs` and `model.rs`.

**Behaviour** (UI §4.1.3):
- `↑↓` move the cursor.
- `enter` on an agent row:
  - selects the agent, unfolding its parents;
  - focuses the agent pane in Selected mode;
  - sends `ViewAgent`.
- The cursor stays on the same agent when the list re-orders. When that agent leaves, it moves to the nearest row.
- The header's need-you count equals the list's length.

- [ ] **Step 1: Write the failing tests.**
  - `enter_selects_agent_and_focuses_agent_pane`: selection is the agent, focus is `Agent`/`Selected`, and `Send(ViewAgent)` was emitted.
  - `enter_unfolds_parents`.
  - `cursor_follows_agent_across_reorder`.
  - `cursor_moves_to_neighbour_when_item_leaves`.
  - `header_count_equals_list`.
  - Snapshot `needs_you_focused_80x24` / `_160x48`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui needs_you`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `cargo test -p duohive-tui needs_you`
- [ ] **Step 5: Commit.** `feat: act on Needs you`

### Task 2.7: TUI — the agent pane (live terminal, Selected and Typing, scrolling)

**Files:**
- Create `crates/duohive-tui/src/keys_out.rs`.
- Modify `render/agent_pane.rs`, `app.rs` (stream lifecycle), `input.rs` and `model.rs`.
- Modify `crates/duohive/src/startup.rs`: build `TmuxPaneView` and set `AppConfig.pane_view`.
- Modify `crates/duohive-tui/src/app/tests.rs`: test configs use `FakePaneView`.

**Interfaces:**
- Consumes `PaneView` and `PaneStream` (Task 2.2), `TermView` (Task 2.4) and `TmuxPaneView` (Task 2.3).
- `AppConfig` gains `pane_view: Arc<dyn PaneView>`.
- Produces `pub fn key_to_input(k: KeyEvent) -> Option<PaneInput>`.

**Behaviour:**
- **Title:**

  | Selection | Title |
  |---|---|
  | An agent in a worktree | `<agent> · <CLI> · worktree <task-id>` |
  | Any other agent | `<agent> · <CLI> · <dir>` |
  | Duo | `⬢ Duo · <CLI> · your manager agent` |

  `· TYPING` is added while typing, and `· scrolled` while scrolled back.
- **The stream:**
  - opens per P2-D4, and its chunks are applied to the `TermView`;
  - `resize` is called on open, on terminal resize and on zoom (P2-D5).
- **Typing mode:**
  - `enter` in Selected mode starts it;
  - every key except `ctrl+q` goes to `send(stream, key_to_input(k))`;
  - a bracketed paste is sent as `Paste`;
  - the border is heavy, and the footer reads `TYPING · every key goes to <agent>, ctrl+c too · ctrl+q stop typing`.
- **`PgUp`/`PgDn`:** P2-D12.
- **Non-running selections** render P2-D3's summaries.
- **Focusing the pane** on a finished agent sends `ViewAgent`.

- [ ] **Step 1: Write the failing tests.**
  - `typing_forwards_every_key_but_ctrl_q`: `q`, `esc`, `tab`, `ctrl+c`, `?`, `space`, `↑`, `F5` and `alt+x` give their `PaneInput`. `ctrl+q` gives nothing, and the mode becomes Selected.
  - `typing_ends_when_window_dies`: on `Closed`, the mode becomes Selected and the footer note reads `<agent> stopped`.
  - `paste_while_typing_is_bracketed`.
  - Stream lifecycle:
    - `stream_opens_after_100ms_settle_and_only_one`;
    - `stream_closes_on_selection_change`;
    - `resize_follows_pane_size`.
  - `pgup_enters_scrolled_snapshot_and_pgdn_returns_live`.
  - `title_shows_worktree_or_directory`: `AgentView.worktree = Some("retry-loop")` → `· worktree retry-loop`; `None` → `· <dir>`.
  - `stopped_agent_shows_give_work_hint`.
  - `focusing_finished_agent_sends_view`.
  - Snapshots, `_80x24` / `_160x48`:
    - `agent_pane_typing`;
    - `duo_waiting_prompt`;
    - `role_summary`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui agent_pane && cargo test -p duohive-tui keys_out`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: live agent pane with typing and scrollback`

### Task 2.8: TUI — zoom

**Files:**
- Create `render/zoom.rs`.
- Modify `input.rs` and `model.rs`.

**Behaviour** (UI §4.2, P2-D15):
- `z` toggles zoom on any agent, from the org pane or from the agent pane in Selected mode.
- **Header:** `DuoHive › <agent> (zoomed)`, with `z restore` at the right.
- **Footer:** `enter type · z restore · PgUp/PgDn scroll · ◆ N need you (<names>)`, cut with `…`.
- Typing works as in Task 2.7.

- [ ] **Step 1: Write the failing tests.**
  - `z_toggles_zoom_and_resizes_view`.
  - `zoom_footer_lists_needs_you`.
  - `z_while_typing_is_sent_to_agent`.
  - `z_on_stopped_agent_zooms_its_stopped_line`.
  - `z_on_role_notes`: the footer reads `z zooms an agent`.
  - Snapshot `zoomed_80x24` / `_160x48`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui zoom`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `cargo test -p duohive-tui zoom`
- [ ] **Step 5: Commit.** `feat: zoom the agent pane`

### Task 2.9: Core — alerts

**Files:**
- Create `crates/duohive-core/src/engine/alerts.rs`.
- Modify:
  - `alert.rs` (`alert_for`);
  - `engine/signals.rs` and `engine/timers.rs` (call sites);
  - `engine/reconnect.rs` (suppression);
  - `engine/mod.rs` (`AlertClicked`).
- In the TUI, `model.rs` handles `Event::AlertClicked`: select the agent and focus the agent pane.

**Interfaces — Produces:**

```rust
pub fn alert_for(agent: &AgentName, is_duo: bool, before: &Status, after: &Status, last_reply: Option<&str>) -> Option<Alert>;
```

**Rules:**
- **`alert_for` returns `Some`** when the agent enters waiting or errored, when quiet goes from false to true, or when a non-Duo agent enters finished.
- **Title:** `<agent> needs you` or `<agent> finished`. **Body:** the last reply, or empty.
- **The engine:**
  - drops kinds turned off in `settings.alerts`;
  - suppresses alerts while reconnecting (P2-D14);
  - calls `Notifier::notify` and publishes `Event::Alert`;
  - publishes `AlertsUnavailable` once at `start_up` when `!notifier.available()`.
- **`Input::AlertClicked { agent }`** → `Event::AlertClicked { agent }`.

- [ ] **Step 1: Write the failing tests.** They use the engine harness with `FakeNotifier`.
  - `alerts_on_waiting_errored_quiet_finished`: four transitions give four alerts with these titles.
  - `duo_finishing_never_alerts`.
  - `one_alert_per_state_change`: waiting → working → waiting gives 2 alerts; a repeated `needs-input` hook while waiting gives 0.
  - `no_alerts_during_reconnect_replay`.
  - `disabled_kind_not_sent`.
  - `unavailable_published_once`.
  - `click_becomes_event`.
  - `body_is_last_reply_or_empty`.
  - TUI: `alert_click_selects_agent`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features alerts && cargo test -p duohive-tui alert`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: desktop alert triggers`

### Task 2.10: Desktop notifier

**Files:**
- Create `crates/duohive/src/notifier.rs`.
- Modify:
  - `real_ports.rs` and `system.rs`, where `NoNotifier` is replaced by `DesktopNotifier`. `SystemOptions` gains `notifier: Option<Box<dyn Notifier>>`, an override every test sets to `FakeNotifier`, so tests never post real alerts. The other `SystemOptions` sites set it too: `crates/duohive/tests/{system.rs, app.rs, e2e.rs}`, `crates/duohive/src/{startup.rs, bench/mod.rs}`;
  - the TUI's `model.rs` and `render/header.rs`, for the footer note.

**Interfaces — Produces:**
- `pub struct DesktopNotifier`, implementing `Notifier`, with:

  ```rust
  DesktopNotifier::detect(path_env: &HashMap<String, String>, tx: mpsc::Sender<Input>, timeout: Duration) -> Self
  ```

  The production timeout is 30 s.

**Behaviour:**
- **Detection:** `osascript` on macOS; `notify-send` on Linux, with `--action=open=Open` when `notify-send --help` lists `--action`.
- **One child process per alert**, killed after the timeout.
- **A printed `open`** sends `Input::AlertClicked`.
- **Text is passed as arguments, never through a shell.** osascript receives the title and body through `on run argv`.
- **No notifier:** the TUI shows `desktop alerts unavailable` once per session (UI §4.1.4).

- [ ] **Step 1: Write the failing tests.**
  - `detect_none_when_neither_on_path`.
  - `osascript_args_are_argv_not_script_text`: the hostile body `"); do shell script "rm` appears only as an argv element.
  - `notify_send_uses_action_when_supported`.
  - `click_sends_alert_clicked`: a fake `notify-send` script on a temp `PATH` prints `open`.
  - `child_killed_after_timeout`: timeout 200 ms.
  - TUI: `alerts_unavailable_note_once`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive --lib notifier && cargo test -p duohive-tui alerts_unavailable`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: desktop notifier for macOS and Linux`

### Task 2.11: Core and store — worktree records

**Files:**
- Modify `crates/duohive-core/src/{worktree.rs, ports.rs}`:
  - `WorktreeRow` replaces the placeholder from Task 2.2;
  - `Store` gains three methods;
  - `SessionRow` gains `worktree`.
- Modify `testkit.rs` (`FakeStore`) and `crates/duohive-store/src/{db.rs, sqlite_store.rs, paths.rs}`.
- Modify `crates/duohive/src/system/enriching_store.rs`, which implements `Store`.
- Update every `SessionRow {` site: `crates/duohive-core/src/{ports.rs, engine/mod.rs, engine/reconnect/tests.rs, engine/org_edit/tests.rs}` and `crates/duohive-store/{src/sqlite_store.rs, tests/state_db.rs}`.

**Interfaces — Produces:**

```rust
pub fn task_slug(task: &str) -> String;                                         // P2-D7
pub fn task_id(slug: &str, taken: &BTreeSet<TaskId>, attempt: u8) -> TaskId;    // attempt 0 = first free; n = n more suffixes on
pub fn branch_name(agent: &AgentName, id: &TaskId) -> String;                   // duohive/<agent>/<id>
pub fn worktree_path(data_dir: &Path, agent: &AgentName, id: &TaskId) -> PathBuf;
pub struct WorktreeRow { pub agent: AgentName, pub task: TaskId, pub repo: PathBuf, pub path: PathBuf, pub branch: String,
                         pub created_at: Millis, pub created_from: String, pub state: WorktreeState,
                         pub checked_at: Option<Millis>, pub base: Option<BaseRef> }
pub enum WorktreeState { Unchecked, InUse, NotMerged { files_changed: u32 }, MergedDirty, Merged, Removing, CheckFailed(String) }
// SessionRow gains: pub worktree: Option<TaskId>
// Store gains:
fn load_worktrees(&mut self) -> Result<Vec<WorktreeRow>, StoreError>;
fn save_worktree(&mut self, row: &WorktreeRow) -> Result<(), StoreError>;
fn delete_worktree(&mut self, agent: &AgentName, task: &TaskId) -> Result<(), StoreError>;
```

**Migration 3** (append-only):

```sql
CREATE TABLE worktrees(agent TEXT NOT NULL, task TEXT NOT NULL, repo TEXT NOT NULL, path TEXT NOT NULL,
                       branch TEXT NOT NULL, created_at INTEGER NOT NULL, created_from TEXT NOT NULL, state TEXT NOT NULL,
                       checked_at INTEGER, base TEXT, PRIMARY KEY(agent, task));
ALTER TABLE sessions ADD COLUMN worktree TEXT;
```

`Paths::worktrees_dir()` is `<data_dir>/worktrees`.

- [ ] **Step 1: Write the failing tests.**
  - `task_slugs_are_safe`: `/`, `..`, emoji, 2 000 characters and blank each give ASCII matching `^[a-z0-9-]{1,32}$` or `task`, and the path stays under `worktrees/<agent>/`.
  - `task_ids_get_suffixes`: with `fix` and `fix-2` taken, `task_id("fix", taken, 0)` is `fix-3`, and attempt 1 gives `fix-4`; every id is at most 32 characters (the slug is cut to 28 for the suffix, P2-D7).
  - `same_task_two_agents_two_rows`: `(squad-1, fix-tests)` and `(squad-2, fix-tests)` coexist.
  - `worktree_rows_round_trip`: a row of every `WorktreeState` saves and loads back equal.
  - `session_worktree_round_trips`.
  - `migration_3_applies_on_v2_db`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features worktree && cargo test -p duohive-store`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: worktree records and migration 3`

### Task 2.12: Core — Give work with the Run choice, and delivery confirmation

**Files:**
- Modify `crates/duohive-core/src/{launch.rs, engine/lifecycle.rs, engine/signals.rs, engine/timers.rs, engine/mod.rs}` (the 5 s timer and `AgentView.worktree`).
- Modify the tests that expect a paste to move an agent straight to working: `crates/duohive-core/src/engine/lifecycle/tests.rs` and `crates/duohive/tests/e2e.rs`.
- Modify every `LaunchSpec {` site:
  - `crates/duohive-core/src/adapter.rs`;
  - `crates/duohive-cli/src/{claude.rs, fake.rs, hooks_json.rs, lib.rs}`;
  - `crates/duohive-cli/src/opencode_tests.rs`.
- Modify `docs/design/technical-design.md` §4.2 (the "finished, idle" row), §5.1 and §6.3 ("Delivery moves the recipient to working" becomes "Delivery counts once the recipient leaves rest", P2-D11).

**Interfaces — Produces:**

```rust
LaunchSpec { …, cwd: Option<PathBuf> }       // launch_cwd(spec) prefers it
```

**Behaviour:**

| Agent state | Run | What happens |
|---|---|---|
| stopped, errored | `Directory` | A fresh launch, as before |
| stopped, errored | `NewWorktree` | 1. When the launch runs, the engine probes the agent's own directory, as D26 has every launch do. It must be a git repo, else `CommandRejected("new worktree needs a git repo; <dir> isn't one")`.<br>2. The engine picks a `task_id`, saves an `Unchecked` row and calls `Git::create`.<br>3. `Created(Ok((base, from)))` → store `base` and `created_from = from`, then launch with `cwd = Some(path)`, `SessionRow.worktree = Some(task)` and `AgentView.worktree = Some(task)`, and set the row to `InUse`.<br>4. `Created(Err(BranchExists))` → retry with the next suffix, up to 9 attempts (P2-D7).<br>5. Any other error, or 9 attempts used → delete the row; the agent stays as it was (stopped, or errored with this error replacing the old one), with row error `couldn't create worktree: <e> · space`; `last_task` keeps the task |
| finished, idle | either | The task is typed in where the session already runs, under P2-D11 |

- [ ] **Step 1: Write the failing tests.**
  - `worktree_give_work_creates_then_launches_in_it`: after `Created(Ok)`, the `RuntimeCall::Start` cwd is the worktree path and the session row's `worktree` is `Some`.
  - `existing_branch_takes_next_suffix_then_errors`.
  - `worktree_creation_failure_keeps_task_and_stays_stopped`.
  - `worktree_needs_git_repo`.
  - `next_turn_ignores_run_choice`.
  - `next_turn_stays_at_rest_until_taken`: after the paste the state is unchanged, and a `busy` hook moves it to working.
  - `second_give_work_refused_while_pending`.
  - `untaken_paste_reported_after_5s`.
  - `tilde_busy_read_confirms_for_hookless`.
  - `paste_failed_reported`.
  - `launch_cwd_prefers_spec_cwd` (in `duohive-cli`).
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features lifecycle && cargo test -p duohive-cli launch_cwd`
- [ ] **Step 3: Implement**, and edit TD §4.2, §5.1 and §6.3.
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: give work in a new worktree, confirmed delivery`

### Task 2.13: Core — worktree checks and clean-up

**Files:**
- Create `crates/duohive-core/src/engine/worktrees.rs`.
- Modify:
  - `command.rs` (`CheckWorktrees`, `DeleteWorktree`);
  - `engine/{timers.rs, reconnect.rs, org_edit.rs, lifecycle.rs}`;
  - `docs/design/technical-design.md` §5.3 (P2-D6, P2-D7's branch `duohive/<agent>/<task-id>`, P2-D16, P2-D17) and §7 (the worktrees table and `sessions.worktree`).

**Interfaces — Produces:**

```rust
Command::CheckWorktrees
Command::DeleteWorktree { agent: AgentName, task: TaskId, confirmed: bool }   // by hand; distinct from the RemoveWorktree port request
// Event::Worktrees(Vec<WorktreeRow>) on every change and in Snapshot
```

**When checks run** (TD §5.3): `Git::check` for each row that isn't removing, when:
- its agent enters finished;
- its agent stops;
- the first `DirInventory` arrives (after the first frame, D26);
- a `CheckWorktrees` command arrives;
- the 10-minute timer fires.

**Check results:**

| Result | New state |
|---|---|
| In use (its agent's live session runs in it) | Stays `InUse` whatever the merge result (P2-D16); it is checked again when the session stops |
| `Merged` and 0 dirty | `Merged`, then `Removing`, then `Git::remove { delete_branch: true, force: false }`; `Removed(Ok)` deletes the row |
| `Merged` and dirty | `MergedDirty` |
| `NotMerged` | `NotMerged` |
| `Unknown` | `CheckFailed`; never removed |

**`DeleteWorktree`:**
- **In use:** refused, with `CommandRejected("<agent> is running in it · stop it first")` (P2-D6).
- **`Merged` and clean:** removed directly, with `delete_branch: true`.
- **Anything else:** needs `confirmed`. It is then removed with `force: true` and `delete_branch: false`, so an unmerged branch is kept.

**Restart:** after `start_up`, a session's `worktree` field restores its in-use state.

**Rename and delete:**
- Rename is refused while the agent has any worktree row, which makes `org_edit.rs:24`'s message true.
- Deleting an agent, role or team keeps its rows.

- [ ] **Step 1: Write the failing tests.**
  - `checks_on_finish_stop_startup_open_and_every_10_minutes`: each trigger issues one `GitCall::Check` per eligible row.
  - `merged_clean_unused_is_removed`: `Checked { Merged(Squash), dirty 0 }` → `GitCall::Remove { delete_branch: true, force: false }`; `Removed(Ok)` → the row is gone from `Worktrees`.
  - `never_removes_dirty_unmerged_or_in_use`.
  - `in_use_survives_restart`.
  - `removed_after_session_stops`.
  - `manual_remove_needs_confirm_unless_merged_clean`.
  - `confirmed_remove_forces_and_keeps_branch`.
  - `in_use_remove_refused`.
  - `unknown_merge_never_removes`.
  - `rename_refused_with_worktrees`.
  - `delete_agent_keeps_worktrees`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features worktrees`
- [ ] **Step 3: Implement**, and edit the TD.
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: worktree merge checks and clean-up`

### Task 2.14: Git adapter over the `git` binary

**Files:**
- Create `crates/duohive/src/git/{mod.rs, merge.rs}` and `crates/duohive/tests/git.rs`.
- Modify `system.rs`, replacing `NoGit` with `GitCli`. `SystemOptions` gains `git: Option<Box<dyn Git>>`. Tests that don't exercise git set it to `NoGit`, at the `SystemOptions` sites listed in 2.10.

**Interfaces — Produces:**
- `pub struct GitCli`, with `GitCli::new(worktrees_dir: PathBuf, tx: mpsc::Sender<Input>, env: Vec<(OsString, OsString)>) -> Self`, implementing `Git`. `env` is added to every git child process. Production passes an empty list; tests use it instead of `set_var`.
- Requests for one repo run in order on one blocking worker; different repos run in parallel.
- Each request ends with `Input::Git(GitEvent)`.

**How it works:**
- **Every command:** `git -C <repo>` with `GIT_TERMINAL_PROMPT=0` and `LC_ALL=C`, and a 30 s timeout per command. `commit-tree` also gets `-c user.name=duohive -c user.email=duohive@localhost`, so a machine with no git identity can still check squash merges.
- **Base:**
  - With an `origin` remote: `git symbolic-ref --short refs/remotes/origin/HEAD` gives `origin/<b>`; when that ref is unset, `git ls-remote --symref origin HEAD` (read-only) gives `<b>`. Without a remote, or offline with neither, P2-D17's local order applies.
  - Before a check, `git fetch --quiet origin <b>`. A failed fetch is ignored: offline, the local refs are used.
- **`create`:** `git worktree add -b <branch> <path> <base>`. A "branch already exists" error maps to `GitError::BranchExists`.
- **`check`**, in order:
  1. Fresh: `git rev-list --count <created_from>..<branch>` is 0 → `NotMerged { 0 }` (P2-D17). A merged branch always has the agent's commits after `created_from`, so this never hides a merge.
  2. Normal merge: `merge-base --is-ancestor <branch> <base>`.
  3. Rebase merge: `git cherry <base> <branch>` has no `+` lines.
  4. Squash merge: `commit-tree <branch>^{tree} -p <merge-base> -m squash`, then `git cherry <base> <synthetic>` has no `+` lines.

  The dirty count is the number of lines from `git -C <path> status --porcelain`. `files_changed` comes from `diff --name-only <merge-base>..<branch>`.
- **`remove`:**
  - refuses any path that, once canonicalised, is not under `worktrees_dir`;
  - runs `git worktree remove [--force] <path>`, with `--force` only when the request has `force`;
  - with `delete_branch`, then runs `git branch -D <branch>`.

- [ ] **Step 1: Write the failing tests.** Each runs on throwaway repos in `tempfile`, with a bare `origin`.
  - Merge detection:
    - `detects_normal_merge`: a merge commit and a fast-forward each give `Merged(Normal)`;
    - `detects_rebase_merge`;
    - `detects_squash_merge`;
    - `squash_check_works_without_git_identity`: `GitCli`'s `env` sets `HOME` and `GIT_CONFIG_GLOBAL` to an empty temp dir;
    - `fresh_branch_is_not_merged`: a branch with no commits since `created_from` gives `NotMerged { 0 }`, although `--is-ancestor` holds;
    - `unmerged_counts_files`;
    - `dirty_worktree_counted`.
  - Base and offline:
    - `base_prefers_origin_head_then_ls_remote_then_default_main_master`;
    - `offline_fetch_uses_local_refs`, with `origin` removed.
  - Branches and removal:
    - `existing_branch_is_an_error_not_reused`: returns `BranchExists`, and the existing branch tip is unchanged;
    - `remove_refuses_paths_outside_worktrees_dir`;
    - `remove_without_force_keeps_dirty_worktree`;
    - `create_then_remove_round_trip`.
  - `same_repo_requests_run_in_order`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive --test git`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: git worktrees and merge detection`

### Task 2.15: TUI — the Give work dialog

**Files:**
- Create `crates/duohive-tui/src/forms/give_work.rs`.
- Modify `forms/mod.rs`, `input.rs` and `model.rs`.

**Opening** (UI §4.5, P2-D10, P2-D11):
- `space` on an agent opens the dialog, in the org pane or the agent pane in Selected mode.
- `space` on Duo in the tree, or on a team or role, shows P2-D10's notes.

**The dialog:**
- **Title:** `Give work · <agent> · <glyph> <word>`.
- **Task box:** multi-line with `alt+enter`; `enter` is disabled while it is empty.
- **By state:**

  | State | The dialog shows |
  |---|---|
  | stopped, errored | `Run  ( ) new worktree   (•) <dir>   agent default: directory\|worktree`, pre-set from the resolved `worktree` default; then `Starts a fresh session.` The worktree radio is disabled, with the reason under it, when the directory isn't a git repo |
  | finished, idle | `Run  in worktree <id>, as the next turn of <agent>'s session`, or `in <dir>, …` |
  | starting, working, waiting | P2-D10's busy text, with Give work disabled |
  | finished or idle with a delivery pending (P2-D11) | `<agent> is still taking the last task`, with Give work disabled |

- **`enter`** sends `GiveWork { agent, task, run }`.
- **`GiveWorkNotTaken`** reopens the dialog pre-filled, with P2-D11's note.

- [ ] **Step 1: Write the failing tests.**
  - Opening and notes:
    - `space_opens_for_agent_only`;
    - `role_team_and_duo_notes`;
    - `space_in_needs_you_does_nothing`.
  - Task box:
    - `enter_disabled_when_empty`;
    - `alt_enter_adds_line`.
  - Run choice:
    - `run_preset_from_agent_default`;
    - `worktree_radio_disabled_without_git_repo`.
  - Variants:
    - `errored_title_shows_errored`;
    - `next_turn_variant_shows_where_it_runs`;
    - `busy_variant_refuses`;
    - `pending_variant_says_still_taking`;
    - `not_taken_reopens_prefilled`.
  - Snapshots of the three variants, `_80x24` / `_160x48`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui give_work`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `cargo test -p duohive-tui give_work`
- [ ] **Step 5: Commit.** `feat: give work dialog`

### Task 2.16: TUI — the worktree list (`w`) and worktree copy elsewhere

**Files:**
- Create `render/worktrees.rs`.
- Modify `input.rs`, `model.rs`, `render/dialog.rs` (the stop and delete copy) and `forms/agent.rs` (rename).

**The list** (UI §4.9):
- **Opening:** `w` opens it, except in the chat input, a form or Typing mode, and sends `CheckWorktrees`.
- **Title:** `Worktrees · <n> · removed automatically once merged and clean`.
- **Columns:** Agent, Branch, State. State words are in `fg.muted`:

  | State | Word |
  |---|---|
  | `Unchecked` | `not checked yet` |
  | `InUse` | `in use` |
  | `Merged`, `Removing` | `merged · removing` |
  | `NotMerged` | `not merged · N files changed` |
  | `MergedDirty` | `merged · uncommitted changes, kept` |
  | `CheckFailed` | `couldn't check · <reason>` |

- **Status line:** `checked Nm ago against <base>`.
- **Keys:**
  - `enter` selects the agent and focuses the agent pane;
  - `d` removes:
    - merged and clean: at once;
    - in use: the P2-D6 note;
    - otherwise: a `Remove worktree?` confirmation in UI §4.8's style, with focus on Cancel;
  - `esc` goes back.

- **Footer:** `enter open in agent · d remove (asks unless merged and clean) · esc back` (UI §4.9).

**Copy elsewhere:**
- The agent form's Name field is read-only while the agent has any worktree row, with `rename only while stopped with no worktrees` under it (PRD Lifecycle; `forms/agent.rs` checks only "stopped" today).
- The stop confirmation adds `in a worktree. The worktree and its branch are kept.` when the agent is in one (UI §4.8).
- The delete-role and delete-team confirmations end with `Worktrees are kept and stay listed under w.`

- [ ] **Step 1: Write the failing tests.**
  - Opening:
    - `w_opens_and_requests_check`;
    - `w_types_in_chat_input`.
  - `states_render_as_words_not_glyphs`: covers every `WorktreeState`.
  - Removing:
    - `d_on_merged_clean_removes_without_asking`;
    - `d_on_unmerged_asks_with_cancel_focused`;
    - `d_on_in_use_notes`.
  - Navigation and copy:
    - `enter_goes_to_agent`;
    - `stop_dialog_mentions_worktree`;
    - `name_read_only_with_worktrees`.
  - Snapshot `worktrees_80x24` / `_160x48`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui worktrees`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `cargo test -p duohive-tui worktrees`
- [ ] **Step 5: Commit.** `feat: worktree list`

### Task 2.17: TUI — Settings (`,`)

**Files:**
- Create `render/settings.rs`.
- Modify `forms/mod.rs`, `input.rs` and `model.rs`.

**Behaviour** (UI §4.10, P2-D13):
- **Opening:** `,` opens it, except in the chat input, a form or Typing mode.
- **Fields:**

  | Group | Fields |
  |---|---|
  | You | Your name |
  | Attention | Quiet after, in minutes, at least 1 (D19); Desktop alerts `[x] needs you  [x] finished` |
  | Look | Theme `auto \| dark \| light`; Last reply under agents; Reduce motion |

- **Saving:** every change sends `UpdateSettings` at once.
- **Copy:** the title is `Settings · saved to settings.toml`, and the footer is `changes save immediately · esc back`. The text `org defaults and Duo: e on Duo (Edit org)` sits under the groups.
- **A broken `settings.toml`** (D29) shows the banner, and changes apply in memory only.

- [ ] **Step 1: Write the failing tests.**
  - Opening:
    - `comma_opens_settings`;
    - `comma_types_in_chat_input`.
  - Changes:
    - `each_change_sends_update`;
    - `quiet_minimum_one`.
  - Effects:
    - `alert_toggle_sends_update_with_alerts_field`: unticking "finished" sends `UpdateSettings` with `alerts.finished == false`. Task 2.9's `disabled_kind_not_sent` covers what the engine then does;
    - `reduce_motion_stops_spinner`.
  - `broken_settings_file_edits_in_memory`.
  - Snapshot `settings_80x24` / `_160x48`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui settings`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: settings screen`

### Task 2.18: Wiring, end-to-end tests and bench

**Files:**
- Modify `crates/duohive/src/{system.rs, startup.rs, args.rs, bench/mod.rs}`.
- Modify `crates/duohive/tests/{e2e.rs, bench.rs}`.
- Modify `docs/plans/bench-results.md`.

**Wiring:** the notifier and git overrides exist since 2.10 and 2.14. The e2e tests here use `FakeNotifier`, and real `GitCli` on temp repos.

- [ ] **Step 1: Write the failing tests.** They run on a real tmux with a throwaway socket, and a fake agent CLI.
  - `e2e_view_and_type_reaches_agent`: open a view, send `Text("hello")` and `Enter`, and the pane echoes `hello`.
  - `e2e_worktree_task_merged_then_removed`:
    1. Give work with `NewWorktree` on a temp repo that has a bare origin;
    2. the agent finishes;
    3. the test squash-merges the branch into the origin default;
    4. stop the agent;
    5. `Worktrees` no longer lists the worktree, and its directory is gone.
  - `e2e_alert_on_waiting`.
  - `e2e_not_taken_reported`: a fake agent that ignores pastes → `GiveWorkNotTaken` within 6 s.
  - `bench_with_live_view`: `duohive bench --agents 10 --view` holds one stream open and still reports hook → screen.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive --test e2e --test bench`
- [ ] **Step 3: Implement** the wiring and the bench's `--view` flag.
- [ ] **Step 4: Run, see PASS.**
  1. Run `make check`.
  2. Run `cargo build --release -p duohive && ./target/release/duohive bench --agents 10 --view`, and record it under "Phase 2".
  3. The 10-agent targets still pass: idle CPU < 1 %, memory < 20 MB, hook → screen < 1 s.
- [ ] **Step 5: Commit.** `feat: wire the one screen and record phase 2 bench`

### Task 2.19: Phase 2 acceptance — a day without `tmux attach`, and the friction log

This is acceptance, with no code under test.

**Files:**
- Create `docs/plans/friction-log.md`.
- Modify `docs/plans/bench-results.md` (live checks).

- [ ] **Step 1: Start the friction log** the day Tasks 2.1–2.18 are merged. Each entry has the date, the screen, what was slow, confusing or missed, and a severity from 1 to 3.
- [ ] **Step 2: Run the author's daily org** for at least three working days without `tmux attach`. Any moment that needs it is a severity-1 entry.
- [ ] **Step 3: Run the live checks** carried from phase 1 and S3, and record each result:
  - a newline and non-ASCII text in OpenCode's `--prompt=`;
  - `--effort=` on a real turn;
  - a task starting with `/` on OpenCode;
  - a Claude turn that runs on past `Escape`;
  - S3.8, the trust dialog in a new worktree path.

  If the trust dialog appears, decide with the author whether to accept the `~◆ waiting` it shows or to pre-trust per launch, which must never touch global config.
- [ ] **Step 4: Commit.** `docs: phase 2 friction log and live checks`

### Task 2.20: UI/UX improvement pass, then mark the phase done

This is validated by snapshot diffs and the checklist below.

**Files:**
- Fixes as found, each with a test or snapshot.
- Docs:
  - `docs/ux/ui-spec.md`: changed screens, §4.1.4 (P2-D18) and §4.9's "Checked" list (P2-D6's check on stop, if approved);
  - `docs/product/prd.md` Lifecycle "checked" list (P2-D6, if approved);
  - `docs/design/technical-design.md` §12: row 2 gains Settings (P2-D13), plus anything else phase 2 changed;
  - `docs/plans/v1-roadmap.md`: row 2 is marked done, and row 3 becomes next;
  - `docs/product/prd.md`: the status in Delivery Phases row 2;
  - `docs/README.md`;
  - `docs/plans/v1-traceability.md`.

- [ ] **Step 1: Fix the top friction-log items.**
  - Every severity-1 item, then severity-2 items by count.
  - Also fix the carried item "long agent names cut in the middle in the 50-column pane".
  - Each fix starts with a failing test or snapshot.
- [ ] **Step 2: Re-check every screen** against the six PRD UX principles:
  - at 80×24, 100×30 and 160×48;
  - in both themes, and with `NO_COLOR`.

  Record pass or fixed for each screen × principle in `friction-log.md` under "UI/UX pass".
- [ ] **Step 3: Update the docs listed above.** The interim decisions (P2-D3 copy, P2-D8, P2-D10, P2-D13's missing rows) stay in this plan and in the traceability matrix's adaptations table, not in the UI spec.
- [ ] **Step 4: Run `make check`, then commit.** `docs: phase 2 UI/UX pass and phase status`

---

## Self-review notes

- **Spec coverage, by roadmap row 2 item:**

  | Item | Tasks |
  |---|---|
  | Three-pane layout | 2.5 |
  | Agent pane, Selected and Typing modes | 2.3, 2.4, 2.7 |
  | Zoom | 2.8 |
  | Needs you you can act on | 2.6 |
  | Desktop alerts | 2.9, 2.10 |
  | Give work with worktrees and merge clean-up | 2.11–2.16 |
  | Friction log, then the UI/UX improvement pass | 2.19, 2.20 |
  | Settings (P2-D13) | 2.17 |

- **TD §8 rows covered:**
  - worktree creation fails: 2.12;
  - worktree not merged or dirty: 2.13, 2.16;
  - no desktop notifier: 2.10.
- **Out of scope here** (owners are in the traceability matrix):
  - phase 3: Duo's tools, the inbox, the real chat, cards, Give work to a role, Send as message instead;
  - phase 4: `?`, `f`, `m`, the mouse, the key-binding editor, New org from template, first run, the model picker, theme auto-detection.
- **Size and proportion:** 20 tasks. The plan gives signatures, rules and test names. Bodies appear only for the merge-detection order and the migration SQL, which the TD fixes.

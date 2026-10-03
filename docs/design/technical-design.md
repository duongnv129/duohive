# DuoHive — Technical Design

**Status:** Draft v1 · **Last updated:** 2026-10-03
Living document, updated in place. It says *how* DuoHive meets the [product requirements](../product/prd.md) and the [UI/UX spec](../ux/ui-spec.md); read those first.

---

## 1. Summary

DuoHive is a Rust app built on Ratatui. For v1 it runs as **one process**, organised in clean layers so its core can later move into a background service without a rewrite.

- **Agents:** each agent runs in its own window on a **dedicated tmux server** (`tmux -L duohive`), so it survives DuoHive closing.
- **Status:** status arrives as **events** rather than through per-agent polling. Exact states come from each CLI's own hooks; tmux reports activity through one control-mode connection; a screen is read only after its activity settles.
- **Memory:** only the agent you are watching is decoded as a full terminal screen, so DuoHive stays light from 10 to 100 agents.

## 2. Decisions

| Topic | Decision | Why |
|---|---|---|
| Language and UI | Rust + Ratatui (crossterm backend) | Smallest footprint and fastest start-up among the options measured |
| Agent runtime | tmux, on DuoHive's own socket (`tmux -L duohive`) | Agents outlive the app and can be attached from any terminal; a proven model |
| Process model | One process in v1; core behind a command/event boundary | Simple to ship; the boundary becomes a socket when the core moves into a background service |
| Status | CLI hooks, then tmux activity, then a screen read on settle | Exact where possible, cheap at scale, honest (`~`) where guessed |
| Concurrency | tokio; the core is one task that owns all state | Commands are processed in order, with no locks in business logic |
| Storage | `org.toml` for the org; `settings.toml` for personal settings; SQLite (WAL) for live state | The org stays shareable and hand-editable; live state stays local |
| Supported CLIs | Claude Code and OpenCode in v1; Codex is deferred to after v1 | Two CLIs done well before a third; each CLI is one adapter behind the `CliAdapter` port, so adding Codex later changes no other layer |
| Agents' tools | MCP through a `duohive mcp` helper per agent, authenticated by a per-launch token | Every supported CLI speaks MCP; the helper finds DuoHive by socket |
| CLI settings | Hooks, MCP and instructions passed per launch | The user's global CLI configuration is never modified |
| Approvals | DuoHive never auto-approves a CLI prompt and passes no auto-approve flags of its own | Commit, push, merge, publish and delete wait for the user, for every agent (PRD) |

## 3. Architecture

```
            ┌───────────────────────── duohive (one process) ─────────────────────────┐
  You ────► │  TUI (Ratatui)            MCP endpoint (agents' tool calls)             │
            │      │  commands ▲ events      │ commands ▲ results                     │
            │      ▼           │             ▼          │                             │
            │  ┌──────────────────────── Core (engine) ────────────────────────────┐  │
            │  │ Org model · Agent lifecycle · Status engine · Inbox · Duo actions │  │
            │  │ Worktrees · Approval cards · Duo chat                             │  │
            │  └───────┬──────────────────┬────────────────────┬───────────────────┘  │
            │          ▼                  ▼                    ▼                      │
            │   Runtime port        CLI adapter port      Storage port                │
            │   (tmux impl)         Claude·OpenCode       (SQLite + TOML)             │
            └──────────┬──────────────────┬────────────────────┬──────────────────────┘
                       ▼                  ▼                    ▼
               tmux -L duohive     agent CLIs and hooks   ~/.config, ~/.local/share
```

### 3.1 Layers

Dependencies point inward only:

| Layer | Holds | Must not know about |
|---|---|---|
| **Core** | Org model and inheritance, agent lifecycle, status engine, inbox, Duo's actions and chat, approval cards, worktree rules, task-to-role picking | tmux, terminals, SQL, the TUI, MCP |
| **Ports** (traits in core) | `Runtime`, `CliAdapter`, `Store`, `Clock`, `Notifier`, `Git` | Any implementation |
| **Adapters** | tmux runtime, the two CLI adapters (Claude Code, OpenCode), SQLite and TOML store, system clock, desktop notifier (`osascript` on macOS, `notify-send` on Linux), git via the `git` binary | The TUI |
| **Interfaces** | TUI; MCP endpoint | Each other |

### 3.2 Command/event boundary

- **Interfaces send commands to the core:** `GiveWork`, `StartAgent`, `StopAgent`, `ViewAgent`, `SendMessage`, `ChatToDuo`, `AssignToRole`, `ProposeOrgChange`, `ApproveCard`, `EditOrg`, `NewOrgFromTemplate`, …
- **The core publishes events:** `AgentStatusChanged`, `LastReplyChanged`, `MessageQueued`, `MessageDelivered`, `CardRaised`, `CardAnswered`, `ChatLine`, `OrgChanged`, `NeedsYouChanged`, `Alert`, …
- **No interface calls another:** the TUI renders events; the MCP endpoint turns tool calls into commands and awaits their results.

Commands and events are plain serialisable enums. When the core moves into a background service, this boundary becomes a local socket, and nothing else changes.

### 3.3 Concurrency

- **Runtime:** one tokio runtime.
- **The core:** a single task that owns all state, receives commands and adapter events on channels, and handles them one at a time. Its timers (quiet threshold, settle, worktree re-checks) run on one timer wheel, not per agent.
- **Adapters:** each runs as its own task (the tmux control connection, the hook listener, the MCP listener, git checks) and talks to the core only through channels.
- **The TUI:** runs on its own task. It draws on change only, capped at 30 frames a second, and draws nothing when idle; the working spinner (4 fps, on-screen rows only) is the one exception.

### 3.4 Workspace

| Crate | Contents |
|---|---|
| `duohive-core` | Domain, ports, engine. No I/O |
| `duohive-tmux` | `Runtime` over tmux (control mode, windows, flow control, subscriptions) |
| `duohive-cli` | `CliAdapter` for Claude Code and OpenCode |
| `duohive-store` | `Store` over SQLite, `org.toml` and `settings.toml` |
| `duohive-tui` | Ratatui views, input, layout |
| `duohive-mcp` | MCP endpoint and the `duohive mcp` helper |
| `duohive` | Binary: wiring, subcommands (`mcp`, `hook`, `bench`), start-up |

The crate boundaries make the inward-dependency rule a compile error to break.

## 4. Status pipeline

### 4.1 Sources, best first

| Source | Mechanism | Gives |
|---|---|---|
| **1. CLI hooks** (exact) | Each launch passes the CLI's hook settings. Every hook runs `duohive hook <agent> <event>`, which reads the JSON payload from stdin (Claude Code's hooks and DuoHive's OpenCode plugin both write it there), authenticates with the session's token (§6.1), and writes one line to `run.sock` (or to the database when DuoHive is closed) | Started, working, waiting for approval, turn finished, ended, and last-reply data where the payload carries it |
| **2. tmux activity** | One control-mode connection to `tmux -L duohive`. Every pane except the selected one is silenced (no-output or paused flow control, chosen by spike S0), so it sends no `%output`. Activity comes instead from a control-mode format subscription on each agent window's activity time (`refresh-client -B`, tmux ≥ 3.2; one window per agent, so `#{window_activity}` is the agent's), which tmux reports about once a second when it changes | Activity times, which feed "quiet" and trigger settle reads |
| **3. Screen read on settle** (`~`) | When an agent's activity time hasn't changed for one report period, read that one pane once (`capture-pane`) and apply the adapter's screen rules | States a CLI does not report (marked `~`), and the last-reply line where no hook carries it |

- **Spike S0:** the subscription mechanism (reports keep arriving for silenced panes, at the expected rate, at 100 windows) and the silencing method (no-output or paused flow control) are **verified in spike S0** before `duohive-tmux` is written. Fallback: one `list-windows -F '#{window_id} #{window_activity}'` per second over the same connection, which costs one command for all agents.
- **Start-up:** git checks (worktree merges, directory checks for git repos) run after the first frame, so they never delay it.

### 4.2 Status engine

A state machine per agent. A hook event moves it exactly; a settled screen read can move it only where the adapter has no hook for that transition, and marks the result `~`.

| From | Event | To |
|---|---|---|
| stopped | Give work, `s`, your `@` message, `start_agent` | starting |
| errored | Give work, your `@` message, `start_agent` (`s` on an errored agent stops it) | starting |
| starting | The CLI is ready and has a task | working |
| starting | The CLI is ready with no task (`s`) | idle |
| starting | Launch fails: CLI missing, pane dies, or not ready within the timeout | errored |
| working | The CLI asks for approval or input | waiting |
| waiting | The user answers | working |
| working | Turn ends (any agent but Duo) | finished |
| working | Turn ends (Duo) | idle |
| finished | The user selects the agent (`ViewAgent`) | idle |
| finished, idle | New work: a task as the next turn, or a delivered message | working |
| any running state | Unexpected exit (non-zero status, or the pane dies without the CLI's end hook) | errored |
| any running state | Stopped by the user or Duo, or a normal exit | stopped |
| errored | Stopped (`s`, or by Duo) | stopped |

- **Quiet:** an overlay flag on working and starting only. It is set when an agent has had no activity and no hook event for the quiet threshold (default 5 minutes, from `settings.toml`), and cleared by any activity or hook. A waiting agent stays waiting.
- **Needs you:** waiting agents, quiet agents, errored agents, and every pending card (`CardRaised` adds it, `CardAnswered` removes it). Ordered oldest first by the time each entered its state.
- **Alert triggers:** an agent enters waiting, errored or quiet; a card is raised; an agent other than Duo enters finished. One `Alert` per agent per state change; the alert kinds can be turned off in Settings. Clicking an alert selects the agent where the notifier reports clicks (`notify-send --action` on Linux); `osascript` notifications on macOS can't, so there the alert only informs. When no notifier is found, the TUI shows a footer note instead.

### 4.3 Cost at scale

- **No per-agent polling:** activity is pushed by tmux, so an idle org costs almost nothing.
- **One full screen at a time:** the selected pane alone streams full output into a terminal emulator; the other 99 send activity reports only.
- **Bounded reads:** a busy agent redrawing a spinner many times a second costs one screen read after it settles.

**Latency budget** (PRD: reported states under 1 s at 10 agents and under 2 s at 100; screen-read states under 2 s):

| Path | Expected |
|---|---|
| Hook → status on screen | < 50 ms |
| Activity settles → read → status (`~`) | about 1–1.5 s (one or two report periods, plus the read) |
| Quiet | the threshold |

## 5. Agents and CLIs

### 5.1 Launch path

The same path serves you, Duo, and a task given to a role. Giving work depends on the agent's state:

| State | Giving work |
|---|---|
| stopped, errored | A fresh launch, below |
| finished, idle | The task is typed into the existing session as its next turn (through the same paste path as inbox delivery, §6.3); it runs where the session already runs |
| working, waiting | Refused; the caller is offered "send as message instead" (§6.3) |

A fresh launch:

1. **Resolve settings:**
   - **Prompt:** stacked as org, then team, then role, then agent, each under a heading naming its level.
   - **Directory, CLI, flags, model, effort, worktree default:** the nearest value up the tree wins, ending at the org defaults in `org.toml`.
2. **Choose where it runs:** the agent's directory, or a new worktree (§5.3).
3. **Build the command:** `CliAdapter::launch(spec)` produces the command line and environment:
   - the standing instructions, the hooks, the MCP config and the model and effort;
   - any extra flags;
   - the task as the first prompt, if there is one (`s` starts with none);
   - `DUOHIVE_TOKEN=<per-launch secret>` (§6.1).
4. **Start the window:** `Runtime::start` opens the agent's window on `tmux -L duohive`; the agent becomes `starting`, and its first hook moves it on.

### 5.2 CLI adapters

| | Standing instructions | Hooks | MCP | Model, effort | Duo's chat replies (§6.5) |
|---|---|---|---|---|---|
| Claude Code | `--append-system-prompt` | per-launch `--settings` file | `--mcp-config` | `--model`; effort setting | transcript path in the hook payload |
| OpenCode | DuoHive-owned agent definition, selected through an environment variable | DuoHive plugin | same config | provider and model config | the plugin's events |

- **v1 CLIs:** Claude Code and OpenCode only. Codex is deferred to after v1; it would be a third adapter with its own spike.
- **Spike first:** each mechanism is verified by a short spike against the pinned CLI version before its adapter is written (S1 Claude Code, S2 OpenCode). Where a mechanism is missing, the fallback is a temporary instructions file in the run directory, never the user's global configuration.
- **Model and effort lists:** read from each CLI at runtime, never hard-coded. While loading, the form shows "loading models…"; on failure, the user can type a name.
- **Version check:** each adapter declares the CLI versions it was tested on. An untested version still runs, with a notice on the agent's row.

### 5.3 Worktrees

- **Location:** `~/.local/share/duohive/worktrees/<agent>/<task-id>`.
- **Branch:** `duohive/<agent>/<task-slug>`, from the base branch below.
- **Base branch:** `origin`'s default branch (`refs/remotes/origin/HEAD`) when the repo has a remote, else the local default branch. Before a check, DuoHive fetches that branch quietly; offline, it uses what it has.
- **Merge detection:** a normal merge when the branch tip is an ancestor of the base (`git merge-base --is-ancestor`); a rebase merge when every branch commit has an equivalent on the base (`git cherry`); a squash merge when one synthetic commit of the branch's whole change, on the merge base, has an equivalent on the base (`git commit-tree`, then `git cherry`).
- **When checked:** when the agent finishes, after start-up (after the first frame), when the worktree list opens, and every 10 minutes.
- **Removal:** only when merged **and** clean (no uncommitted changes). Everything else stays and is listed; removing a worktree that isn't merged and clean asks first.
- **Creation failure:** the agent stays stopped, the error goes on its row, and the task is kept for retry.

### 5.4 Task to a role

- **Free:** an agent that is stopped, finished or idle.
- **A free agent takes it:** of the role's free agents, the one at rest longest. Giving the task then follows §5.1 for that agent's state.
- **None free:** the core raises an approval card proposing a new agent cloned from the role; the card holds the task, and approving it creates the agent and gives it the task.
- **Agent Squad:** this is how all its tasks flow, with worktrees on by default.

### 5.5 Stop and exit

- **Stopping:** DuoHive sends the CLI's own quit, then kills the window after a grace period. The agent becomes `stopped`.
- **Normal exit:** a CLI that exits on its own with its end hook and a zero status leaves its agent `stopped`; the next task starts a fresh session.
- **Failed launch or crash:** the agent becomes `errored`, with the reason (from the launch error, or the exit status kept by tmux's `remain-on-exit`) on its row. It stays in Needs you until the user gives it work (a fresh launch) or stops it.

## 6. Messaging, Duo and MCP

### 6.1 Identity

- **Token:** every launch gets a fresh secret token, generated by DuoHive and passed to the session's environment as `DUOHIVE_TOKEN`. The core keeps the token-to-agent map in memory and in `state.db`, so reconnecting after a restart keeps it.
- **Checked on every call:** the `duohive mcp` helper and `duohive hook` send the token with each call; the core resolves it to the agent and rejects unknown tokens. Identity is never a name anyone can set.
- **Rights:** the endpoint exposes only the tools that agent may use (§6.2), so no agent can act as Duo or start a delegation chain.

### 6.2 Tools

| Tool | Who | Does |
|---|---|---|
| `list_agents` | every agent | Org with status and last reply |
| `send_message` | every agent | Queue a message; returns an id |
| `message_status` | every agent | queued · delivered · answered · dropped |
| `read_agent` | Duo | Another agent's current screen as text |
| `wait_for_agent` | Duo | Wait for an agent's turn to end, with a timeout |
| `start_agent` | Duo | Give work to an agent (§5.1): a fresh launch (directory or worktree) or the next turn; refused for working or waiting agents |
| `stop_agent` | Duo | Finished or idle: stops it directly. Working or waiting: raises an approval card and returns its result |
| `assign_to_role` | Duo | Task to a role (§5.4) |
| `propose_org_change` | Duo | Add, edit, move or remove a team, role or agent as an approval card; returns approved or rejected. The core rejects removing an agent that isn't stopped |

No tool answers a CLI's permission prompt, and messages are never typed onto a waiting prompt (§6.3), so Duo cannot approve anything for the user.

### 6.3 Inbox

- **Delivery:** a message is delivered only when the recipient is at rest (finished or idle), never onto a waiting prompt. It is pasted through a tmux buffer named for the delivering process. Delivery moves the recipient to working.
- **To a stopped agent:**
  - **the user's own `@agent` message** starts the agent with the message as its task;
  - **a message from another agent** waits, with status `queued · stopped`, until the agent is started and at rest.
- **To a working or waiting agent:** every message waits until it is at rest, the user's included.
- **Framing:**
  - **Your own messages:** typed in exactly as written.
  - **Messages from agents:** wrapped with the sender's name and a boundary token minted at delivery, so the recipient can tell where the other agent's text starts and ends.
- **Statuses:** `queued`, `delivered`, `answered` (the recipient sent a message to the sender after delivery), `dropped`.
- **Limits (between agents only):**
  - a duplicate message is refused within 10 minutes;
  - at most 5 messages a minute per sender → recipient pair;
  - at most 20 messages can wait for one recipient.

  You are never throttled.
- **Failures:** a message that cannot be typed in is marked `dropped` with the reason, and is never retried in a way that could deliver it twice.
- **When DuoHive is closed:** messages and Duo's requests wait in the database and are processed when it next opens.

### 6.4 Duo

- **An ordinary agent:** Duo sits at the root of the tree and runs on the CLI set in `org.toml`. Changing that CLI restarts Duo after a confirmation.
- **Its prompt:** DuoHive supplies a fixed prompt explaining the org, the tools and the rules (approval cards for org changes and for stopping busy agents; commit, push, merge, publish and delete wait for the user); the org prompt is stacked on top.
- **States:** like any agent, except that a finished turn goes straight to idle and never raises a "finished" alert.
- **Visible actions:** each tool call Duo makes is recorded as an action and shown in the chat as an `→` line with its result.

### 6.5 Duo chat

- **Input:** the user's chat line goes into Duo's inbox as a "you" message. It is typed in when Duo is at rest; otherwise it is queued and shown as queued in the chat.
- **Replies:** each adapter extracts Duo's reply at the end of a turn (§5.2): Claude Code from the transcript path in its hook payload; OpenCode from its plugin events. Each is verified in that CLI's spike.
- **Transcript:** chat lines, action lines and card results are stored in a `chat` table in `state.db`, so the history survives restarts.
- **Duo's own CLI prompts:** when Duo's CLI asks for approval, Duo becomes waiting and joins Needs you; the user answers in the agent pane by selecting Duo.

## 7. Storage

| What | Where | Format |
|---|---|---|
| The org: the tree, the org-level defaults (org prompt, default CLI, model, effort, worktree setting), Duo's name and CLI | `~/.config/duohive/org.toml` | TOML; written atomically, with the previous version kept as a backup |
| Personal settings: your name, theme, key bindings, alerts, quiet threshold, last-reply and reduce-motion toggles | `~/.config/duohive/settings.toml` | TOML; edited from the Settings screen |
| Live state: sessions and tokens, inbox, Duo chat, approval cards, worktrees, task history | `~/.local/share/duohive/state.db` | SQLite (WAL); migrations append-only |
| Socket | `~/.local/share/duohive/run.sock` | Unix socket, owner-only permissions |
| Agent runtime | tmux server `tmux -L duohive` | One window per agent |
| Hooks | `duohive hook <agent> <event>`, configured per launch | JSON payload on stdin |

- **One org per install.** "New org from template" stops running agents, then replaces `org.toml` after a confirmation, keeping the old file as a timestamped backup.
- **Hand edits:** `org.toml` is re-read on start. A file that doesn't parse shows its line and error, and the last good org stays in use.
- **Validation:** agent names must be unique; the tree has up to three levels (team → role → agent), and teams are optional; every CLI must be supported. Each error names the field.
- **Missing directory:** makes only that agent invalid, with the error on its row; the rest of the org loads and runs.

## 8. Errors

Errors are shown where they apply, never only in a log.

| Case | Shown |
|---|---|
| CLI missing or unsupported version | Starting it fails: `errored` on the agent's row, with how to fix it; other agents unaffected |
| Untested CLI version | A notice on the agent's row; it still runs |
| Launch fails | `errored`, with the reason on its row, in Needs you; the task is kept for retry |
| Crash (unexpected exit) | `errored`, with the exit status on its row, in Needs you |
| Duo fails to start | Duo's row is `errored`, with the reason; agents are unaffected |
| No agent CLI installed | Full-screen message at first run |
| tmux missing or older than 3.2 | Full-screen message with install steps |
| `org.toml` invalid | Line and error; last good org kept |
| Directory missing | Only that agent is invalid, with the error on its row |
| Worktree creation fails | Error on the agent's row; it stays stopped and the task is kept |
| Hook silent | Status falls back to screen reading (`~`), then to quiet |
| Message undeliverable | `dropped` with the reason |
| Worktree not merged or dirty | Kept and listed |
| No desktop notifier | A footer note; alerts stay in Needs you |

## 9. Testing

| Layer | How |
|---|---|
| Core | Unit tests with fake ports and an injected clock. They cover inheritance and prompt stacking, every transition of the state machine (§4.2), quiet, Needs you and alerts, giving work by state, the inbox and its limits, token rights, approval cards, task-to-role picking and worktree rules |
| CLI adapters | For Claude Code and OpenCode: snapshot tests of each built launch command; parsing of recorded hook payloads, screen captures and Duo replies from pinned CLI versions |
| tmux runtime | Integration tests on a real tmux with a throwaway socket. They cover start, send, flow control, activity subscriptions, exit statuses and pane reads |
| Git | Integration tests on throwaway repos for normal, squash and rebase merge detection |
| TUI | Ratatui `TestBackend` snapshots at 80×24 and 160×48 for every screen, and key and mouse tests for every action |
| End to end | A fake agent CLI that fires hooks drives full scenarios, such as "story → Duo splits it → three agents finish → Duo reports", with no model calls |

## 10. Benchmark

`duohive bench` measures the PRD's targets against fake agents.

- **Agents:** it starts N fake agents (10, then 100) that produce realistic output and bursts of hook events; some report no hooks, to exercise the screen-read path.
- **Measured:**
  - idle CPU;
  - DuoHive's own memory, with agents excluded;
  - the time from a hook to status on screen;
  - the time from output settling to a screen-read (`~`) status on screen;
  - start-up time;
  - binary size.
- **Phase 1:** a basic `bench` (10 agents; idle CPU, memory and hook-to-screen time) runs from the start, so the silencing (no-output or paused flow control, chosen by spike S0) and settle choices are measured early.
- **Release gate:** CI runs the full benchmark on every release. Results on macOS arm64 (Apple M-series) must meet every PRD target, or the release fails; Linux x86_64 results are reported but don't block.

## 11. CI and release

- **CI:** GitHub Actions runs `cargo fmt --check`, `clippy -D warnings`, and tests on macOS and Linux against tmux 3.2 and the latest tmux.
- **Releases:** `cargo-dist` builds from a version tag, publishing GitHub Release binaries, a Homebrew tap and `cargo install duohive`.
- **Dependencies:** Ratatui and crossterm are pinned and upgraded on purpose, and the minimum supported Rust version is declared.

## 12. Delivery mapping

| PRD phase | Built here |
|---|---|
| 1. Org and agents | Spike S0 and the adapter spikes (S1 Claude Code, S2 OpenCode); `duohive-core`, `duohive-store`, `duohive-tmux`, `duohive-cli` (the Claude Code and OpenCode adapters); a minimal org tree with live status and the team, role and agent forms in `duohive-tui`; a basic `duohive bench` |
| 2. One screen | `duohive-tui`: agent pane (watch and type), Needs you, notifications, Give work with the worktree option; the UI/UX improvement pass at the end |
| 3. Duo and messaging | `duohive-mcp` and tokens, inbox, Duo and Duo chat, Duo's start and stop, visible actions, approval cards, task to a role |
| 4. Scale and polish | Performance targets at 100 agents on the release benchmark, flow-control tuning, first-run setup with templates, model and effort picker, key and mouse completeness, the key-binding editor |
| 5. Open-source launch | The pre-launch UX check, CI release gate, `cargo-dist` release, docs |

## 13. Risks

| Risk | Mitigation |
|---|---|
| A CLI changes its hooks or flags | Adapters pinned to tested versions, spikes before each adapter, a notice for untested versions, screen reading as fallback |
| tmux control mode differs across versions | Support 3.2 and newer only; CI on 3.2 and latest |
| The settle mechanism doesn't work as planned: subscriptions don't report for silenced panes (no-output or paused flow control, chosen by spike S0), report too slowly, or cost too much at 100 windows | Spike S0 before `duohive-tmux`; fallback: one `list-windows` activity query per second for all agents |
| Silencing (no-output or paused flow control, chosen by spike S0) does not quiet busy panes enough at 100 agents | Measured by the basic `bench` in phase 1; fallback: pause unselected panes harder and read on settle only |
| Ratatui pre-1.0 breaking changes | Pinned version, deliberate upgrades |
| Users whose agents run on Codex can't use DuoHive for them in v1 | Codex is deferred to after v1, not ruled out: it is one more adapter and spike behind the same `CliAdapter` port |
| Single process: work pauses while DuoHive is closed | Accepted for v1; the command/event boundary makes a background service a later move |

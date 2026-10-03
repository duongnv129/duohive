# DuoHive — Technical Design

**Status:** Draft v1 · **Last updated:** 2026-10-03
Living document, updated in place. It says *how* DuoHive meets the [product requirements](../product/prd.md); read that first.

---

## 1. Summary

DuoHive is a Rust app built on Ratatui. For v1 it runs as **one process**, organised in clean layers so its core can later move into a background service without a rewrite.

- **Agents:** each agent runs in its own pane on a **dedicated tmux server**, so it survives DuoHive closing.
- **Status:** status arrives as **events** rather than through polling. Exact states come from each CLI's own hooks; tmux reports activity through one control-mode connection; a screen is read only after its output settles.
- **Memory:** only the agent you are watching is decoded as a full terminal screen, so DuoHive stays light from 10 to 100 agents.

## 2. Decisions

| Topic | Decision | Why |
|---|---|---|
| Language and UI | Rust + Ratatui (crossterm backend) | Smallest footprint and fastest start-up among the options measured |
| Agent runtime | tmux, on DuoHive's own socket | Agents outlive the app and can be attached from any terminal; a proven model |
| Process model | One process in v1; core behind a command/event boundary | Simple to ship; the boundary becomes a socket when the core moves into a background service |
| Status | CLI hooks, then tmux activity, then a screen read on settle | Exact where possible, cheap at scale, honest (`~`) where guessed |
| Concurrency | tokio; the core is one task that owns all state | Commands are processed in order, with no locks in business logic |
| Storage | `org.toml` for the org; SQLite (WAL) for live state | The org stays shareable and hand-editable; live state stays local |
| Agents' tools | MCP through a `duohive mcp` helper per agent | Every supported CLI speaks MCP; the helper finds DuoHive by socket |
| CLI settings | Hooks, MCP and instructions passed per launch | The user's global CLI configuration is never modified |

## 3. Architecture

```
            ┌───────────────────────── duohive (one process) ─────────────────────────┐
  You ────► │  TUI (Ratatui)            MCP endpoint (agents' tool calls)              │
            │      │  commands ▲ events      │ commands ▲ results                      │
            │      ▼           │             ▼          │                              │
            │  ┌──────────────────────── Core (engine) ────────────────────────────┐   │
            │  │ Org model · Agent lifecycle · Status engine · Inbox · Duo actions │   │
            │  │ Worktrees · Approval cards                                        │   │
            │  └───────┬──────────────────┬────────────────────┬───────────────────┘   │
            │          ▼                  ▼                    ▼                       │
            │   Runtime port        CLI adapter port      Storage port                 │
            │   (tmux impl)         Claude·Codex·OpenCode (SQLite + org.toml)          │
            └──────────┼──────────────────┼────────────────────┼───────────────────────┘
                       ▼                  ▼                    ▼
               tmux server (own socket)  agent CLIs and hooks   ~/.config, ~/.local/share
```

### 3.1 Layers

Dependencies point inward only:

| Layer | Holds | Must not know about |
|---|---|---|
| **Core** | Org model and inheritance, agent lifecycle, status engine, inbox, Duo's actions, approval cards, worktree rules, task-to-role picking | tmux, terminals, SQL, the TUI, MCP |
| **Ports** (traits in core) | `Runtime`, `CliAdapter`, `Store`, `Clock`, `Notifier`, `Git` | Any implementation |
| **Adapters** | tmux runtime, the three CLI adapters, SQLite and TOML store, system clock, desktop notifier, git via the `git` binary | The TUI |
| **Interfaces** | TUI; MCP endpoint | Each other |

### 3.2 Command/event boundary

- **Interfaces send commands to the core:** `StartAgent`, `StopAgent`, `SendMessage`, `AssignToRole`, `ProposeOrgChange`, `ApproveCard`, `EditOrg`, …
- **The core publishes events:** `AgentStatusChanged`, `LastReplyChanged`, `MessageQueued`, `MessageDelivered`, `CardRaised`, `OrgChanged`, `NeedsYou`, …
- **No interface calls another:** the TUI renders events; the MCP endpoint turns tool calls into commands and awaits their results.

Commands and events are plain serialisable enums. When the core moves into a background service, this boundary becomes a local socket, and nothing else changes.

### 3.3 Concurrency

- **Runtime:** one tokio runtime.
- **The core:** a single task that owns all state, receives commands and adapter events on channels, and handles them one at a time.
- **Adapters:** each runs as its own task (the tmux control connection, the hook listener, the MCP listener, git checks) and talks to the core only through channels.
- **The TUI:** runs on its own task. It draws on change only, capped at 30 frames a second, and draws nothing when idle.

### 3.4 Workspace

| Crate | Contents |
|---|---|
| `duohive-core` | Domain, ports, engine. No I/O |
| `duohive-tmux` | `Runtime` over tmux (control mode, panes, flow control) |
| `duohive-cli` | `CliAdapter` for Claude Code, Codex, OpenCode |
| `duohive-store` | `Store` over SQLite and `org.toml` |
| `duohive-tui` | Ratatui views, input, layout |
| `duohive-mcp` | MCP endpoint and the `duohive mcp` helper |
| `duohive` | Binary: wiring, subcommands (`mcp`, `hook`, `bench`), start-up |

The crate boundaries make the inward-dependency rule a compile error to break.

## 4. Status pipeline

### 4.1 Sources, best first

| Source | Mechanism | Gives |
|---|---|---|
| **1. CLI hooks** (exact) | Each launch passes the CLI's hook settings. Every hook runs `duohive hook <agent> <event> [payload]`, which writes one line to `run.sock` (or to the database when DuoHive is closed) | Started, working, waiting for approval, turn finished, ended, and last-reply data where the payload carries it |
| **2. tmux activity** | One control-mode connection to DuoHive's tmux server; every pane except the selected one is paused by flow control, so each sends only an activity notice with a timestamp | Activity times, which feed "quiet" and trigger settle reads |
| **3. Screen read on settle** (`~`) | After 300 ms without output from a pane, read that one pane once and apply the adapter's screen rules | States a CLI does not report (marked `~`), and the last-reply line where no hook carries it |

### 4.2 Status engine

- **A state machine per agent:** `stopped → starting → working ⇄ waiting → finished`, plus `errored`. A hook event moves it exactly; a settled screen read can move it only where the adapter has no hook for that transition, and marks the result `~`.
- **Quiet:** an agent with no activity and no hook event for the quiet threshold (default 5 minutes, a setting) becomes `quiet` and is added to Needs you. Any activity clears it.
- **Needs you:** waiting, errored and quiet agents, plus finished agents you have not yet looked at.

### 4.3 Cost at scale

- **No polling:** nothing runs on a timer per agent, so an idle org costs almost nothing.
- **One full screen at a time:** the selected pane alone streams full output into a terminal emulator; the other 99 send activity notices only.
- **Bounded reads:** a busy agent redrawing a spinner many times a second costs one screen read after it settles.

**Latency budget** (PRD: under 1 s at 10 agents, under 2 s at 100):

| Path | Expected |
|---|---|
| Hook → status on screen | < 50 ms |
| Output settles → read → status | ~300–500 ms |
| Quiet | the threshold |

## 5. Agents and CLIs

### 5.1 Launch path

The same path serves you, Duo, and a task given to a role.

1. **Resolve settings:**
   - **Prompt:** stacked as org, then team, then role, then agent, each under a heading naming its level.
   - **Directory, CLI, flags, model, effort, worktree default:** the nearest value up the tree wins.
2. **Choose where it runs:** the agent's directory, or a new worktree (§5.3).
3. **Build the command:** `CliAdapter::launch(spec)` produces the command line and environment:
   - the standing instructions, the hooks, the MCP config and the model and effort;
   - any extra flags;
   - the task as the first prompt;
   - `DUOHIVE_AGENT=<name>`.
4. **Start the pane:** `Runtime::start` opens the pane on DuoHive's tmux socket; the agent becomes `starting`, and its first hook moves it on.

### 5.2 CLI adapters

| | Standing instructions | Hooks | MCP | Model, effort |
|---|---|---|---|---|
| Claude Code | `--append-system-prompt` | per-launch `--settings` file | `--mcp-config` | `--model`; effort setting |
| Codex | per-launch `-c` override | per-launch `notify` plus approval events | `-c mcp_servers…` | `-m`, `-c model_reasoning_effort` |
| OpenCode | DuoHive-owned agent definition, selected through an environment variable | DuoHive plugin | same config | provider and model config |

- **Spike first:** each mechanism is verified by a short spike against the pinned CLI version before its adapter is written. Where a mechanism is missing, the fallback is a temporary instructions file in the run directory, never the user's global configuration.
- **Model and effort lists:** read from each CLI at runtime, never hard-coded.
- **Version check:** each adapter declares the CLI versions it was tested on. An untested version still runs, with a notice on the agent's row.

### 5.3 Worktrees

- **Location:** `~/.local/share/duohive/worktrees/<agent>/<task-id>`.
- **Branch:** `duohive/<agent>/<task-slug>`, from the repo's current default branch.
- **Removal:** a worktree is removed once its branch is merged into the default branch **and** it has no uncommitted changes. DuoHive checks when the agent finishes and when DuoHive starts.
- **Kept otherwise:** an unmerged or dirty worktree stays, and is listed for you to remove.

### 5.4 Task to a role

- **A free agent takes it:** of the role's stopped or finished agents, the one idle longest.
- **None free:** the core raises an approval card proposing a new agent cloned from the role.
- **Agent Squad:** this is how all its tasks flow, with worktrees on by default.

### 5.5 Stop and exit

- **Stopping:** DuoHive sends the CLI's own quit, then kills the pane after a grace period.
- **Exit:** a CLI that exits on its own leaves its agent `stopped`; the next task starts a fresh session.

## 6. Messaging, Duo and MCP

### 6.1 Identity

Every agent launches with `DUOHIVE_AGENT=<name>`. The `duohive mcp` helper passes it with each call, and the endpoint exposes only the tools that caller may use.

### 6.2 Tools

| Tool | Who | Does |
|---|---|---|
| `list_agents` | all | Org with status and last reply |
| `read_agent` | all | Another agent's current screen as text |
| `send_message` | all | Queue a message; returns an id |
| `message_status` | all | queued · delivered · answered · dropped |
| `wait_for_agent` | all | Wait for an agent's turn to end, with a timeout |
| `start_agent` | Duo | Start an agent with a task (directory or worktree) |
| `stop_agent` | Duo | Stop an agent |
| `assign_to_role` | Duo | Task to a role: a free agent takes it, or an approval card is raised |
| `propose_org_change` | Duo | Add, edit, move or remove a team, role or agent as an approval card; returns approved or rejected |

### 6.3 Inbox

- **Delivery:** a message is delivered only when the recipient is at rest (finished or idle by its hooks), never onto a waiting prompt. It is pasted through a tmux buffer named for the delivering process.
- **Framing:**
  - **Your own messages:** typed in exactly as written.
  - **Messages from agents:** wrapped with the sender's name and a boundary token minted at delivery, so the recipient can tell where the other agent's text starts and ends.
- **Limits (between agents only):**
  - a duplicate message is refused within 10 minutes;
  - the send rate is capped per sender and recipient pair;
  - at most 20 messages can wait for one recipient.

  You are never throttled.
- **Failures:** a message that cannot be typed in is marked `dropped` with the reason, and is never retried in a way that could deliver it twice.
- **When DuoHive is closed:** messages and Duo's requests wait in the database and are processed when it next opens.

### 6.4 Duo

- **An ordinary agent:** Duo sits at the root of the tree and runs on the CLI you pick.
- **Its prompt:** DuoHive supplies a fixed prompt explaining the org, the tools and the rules (approval cards for org changes; commit, push, merge, publish and delete wait for you); your org prompt is stacked on top.
- **Visible actions:** each tool call Duo makes is recorded as an action and shown in the chat as an `→` line.

## 7. Storage

| What | Where | Format |
|---|---|---|
| The org | `~/.config/duohive/org.toml` | TOML; written atomically, with the previous version kept as a backup |
| Settings | `~/.config/duohive/settings.toml` | TOML; edited from the Settings screen |
| Live state: sessions, inbox, chat history, approval cards, worktrees, task history | `~/.local/share/duohive/state.db` | SQLite (WAL); migrations append-only |
| Socket | `~/.local/share/duohive/run.sock` | Unix socket, owner-only permissions |

- **Hand edits:** `org.toml` is re-read on start. A file that doesn't parse shows its line and error, and the last good org stays in use.
- **Validation:** agent names must be unique, the tree has three levels, every directory must exist, and every CLI must be supported. Each error names the field.

## 8. Errors

Errors are shown where they apply, never only in a log.

| Case | Shown |
|---|---|
| CLI missing or unsupported version | On the agent's row, with how to fix it; other agents unaffected |
| Launch fails | Agent stays `stopped`, error on its row, task kept for retry |
| tmux missing or older than 3.2 | Full-screen message with install steps |
| `org.toml` invalid | Line and error; last good org kept |
| Hook silent | Status falls back to screen reading (`~`), then to quiet |
| Message undeliverable | `dropped` with the reason |
| Worktree dirty | Kept and listed |

## 9. Testing

| Layer | How |
|---|---|
| Core | Unit tests with fake ports and an injected clock. They cover inheritance and prompt stacking, the lifecycle state machine, quiet, the inbox and its limits, approval cards, task-to-role picking and worktree rules |
| CLI adapters | Snapshot tests of each built launch command; parsing of recorded hook payloads and screen captures from pinned CLI versions |
| tmux runtime | Integration tests on a real tmux with a throwaway socket. They cover start, send, activity notices, flow control and pane reads |
| TUI | Ratatui `TestBackend` snapshots at 80×24 and 160×48 for every screen, and key and mouse tests for every action |
| End to end | A fake agent CLI that fires hooks drives full scenarios, such as "story → Duo splits it → three agents finish → Duo reports", with no model calls |

## 10. Benchmark

`duohive bench` measures the PRD's targets against fake agents.

- **Agents:** it starts N fake agents (10, then 100) that produce realistic output and bursts of hook events.
- **Measured:**
  - idle CPU;
  - DuoHive's own memory, with agents excluded;
  - the time from a hook to status on screen;
  - start-up time;
  - binary size.
- **Release gate:** CI runs it on every release (macOS arm64, Linux x86_64), and a release fails when any result misses a PRD target.

## 11. CI and release

- **CI:** GitHub Actions runs `cargo fmt --check`, `clippy -D warnings`, and tests on macOS and Linux against tmux 3.2 and the latest tmux.
- **Releases:** `cargo-dist` builds from a version tag, publishing GitHub Release binaries, a Homebrew tap and `cargo install duohive`.
- **Dependencies:** Ratatui and crossterm are pinned and upgraded on purpose, and the minimum supported Rust version is declared.

## 12. Delivery mapping

| PRD phase | Built here |
|---|---|
| 1. Org and agents | `duohive-core`, `duohive-store`, `duohive-tmux`, `duohive-cli` (after the adapter spikes); org tree with status |
| 2. One screen | `duohive-tui`: agent pane, Needs you, notifications, quick prompt with worktree option; the UI/UX pass at the end |
| 3. Duo and messaging | `duohive-mcp`, inbox, Duo, approval cards, task to role |
| 4. Scale and polish | `duohive bench`, flow-control tuning, templates, model and effort picker |
| 5. Open-source launch | CI, `cargo-dist` release, docs |

## 13. Risks

| Risk | Mitigation |
|---|---|
| A CLI changes its hooks or flags | Adapters pinned to tested versions, spikes before each adapter, a notice for untested versions, screen reading as fallback |
| tmux control mode differs across versions | Support 3.2 and newer only; CI on 3.2 and latest |
| Flow control does not quiet busy panes enough at 100 agents | Measured by `bench` in phase 1; fallback: pause selected panes harder and read on settle only |
| Ratatui pre-1.0 breaking changes | Pinned version, deliberate upgrades |
| Single process: work pauses while DuoHive is closed | Accepted for v1; the command/event boundary makes a background service a later move |

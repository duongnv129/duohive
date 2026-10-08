# DuoHive — Technical Design

**Status:** Draft v1 · **Last updated:** 2026-10-04
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
| **Ports** (traits in core) | `Runtime`, `CliAdapter`, `Store`, `Clock`, `Ids`, `DirProbe`, `Notifier`, `Git`, `PaneView` | Any implementation |
| **Adapters** | tmux runtime and pane view, the two CLI adapters (Claude Code, OpenCode), SQLite and TOML store, system clock, desktop notifier (`osascript` on macOS, `notify-send` on Linux), git via the `git` binary | The TUI |
| **Interfaces** | TUI; MCP endpoint | Each other |

- **`Ids`** makes per-launch tokens, and **`DirProbe`** reports whether a directory is a git repository, exists, or is missing (for validation; the core does no I/O). Both came in phase 1.
- **`PaneView`** (phase 2, P2-D1) is the live terminal of the selected agent, and bypasses the engine:
  - **Wiring:** the port (traits and data only) is implemented in `duohive-tmux`, and the binary hands it to the TUI as `Arc<dyn PaneView>`. Raw bytes never pass through the core's single task. The engine still receives `ViewAgent`.
  - **Binding:** `open` returns a `StreamId` bound to the window open at that moment (its pane id, which tmux never reuses within a server).
  - **Keys never reach a newer window:** `send` and `resize` take that id, and fail with `ViewError::Closed` once that window is gone.
  - **The tmux path** is in §4.3 (spike S3).

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
| `duohive-tmux` | `Runtime` over tmux (control mode, windows, no-output silencing, activity polling) |
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
| **2. tmux activity** | One control-mode connection to `tmux -L duohive`. Its panes are silenced with the client flag `no-output` (`refresh-client -f no-output,ignore-size`, chosen by spike S0, followed by `refresh-client -C 160x48` so tmux 3.2a doesn't size windows from the control client; it covers new windows with no per-window command, and silences every pane on the connection; the live view of the selected pane takes its own path, a second control client, §4.3 and spike S3), so they send no `%output`. Activity comes instead from one `list-windows -F '#{window_id} #{window_activity} #{pane_dead} #{pane_dead_status}'` a second over that connection, for all agents at once (one window per agent, so `#{window_activity}` is the agent's); an agent's activity changed when its value differs from the previous reply. A CLI killed by a signal leaves `#{pane_dead_status}` empty (spike S1): it has no exit status, and is errored "exited unexpectedly" | Activity times, which feed "quiet" and trigger settle reads; pane deaths with exit status, or none for a signal death |
| **3. Screen read on settle** (`~`) | When an agent's activity time hasn't changed for one report period, read that one pane once (`capture-pane`) and apply the adapter's screen rules | States a CLI does not report (marked `~`), and the last-reply line where no hook carries it |

- **Spike S0** (`docs/plans/spike-report.md` S0): format subscriptions (`refresh-client -B`) did report for silenced panes at 100 windows, about once a second, with the same delay as polling (median 0.47–0.57 s), but their reader cost more CPU than the one-a-second `list-windows` at 100 noisy windows on both tmux 3.2a and latest, so the poll is the mechanism. `no-output` beat paused flow control on tmux server + reader CPU and needs nothing per new window. A closed window is reported as `%unlinked-window-close @N` (not `%window-close`), and is also absent from the next reply.
- **Start-up:** git checks (worktree merges, directory checks for git repos) run after the first frame, so they never delay it.

### 4.2 Status engine

A state machine per agent. A hook event moves it exactly; a settled screen read can move it only where the adapter has no hook for that transition, and marks the result `~`. One exception: while an agent is waiting, a settled screen read applies whatever the hook coverage (at rest → finished or idle, busy → working), because Claude Code hooks no answer to an approval prompt (spike S1: a declined prompt fires no hook, and an accepted long-running tool fires its next hook only when it ends). OpenCode reports the answer (`permission.replied`, spike S2), where the exception is harmless.

| From | Event | To |
|---|---|---|
| stopped | Give work, `s`, your `@` message, `start_agent` | starting |
| errored | Give work, your `@` message, `start_agent` (`s` on an errored agent stops it) | starting |
| starting | The CLI is ready and has a task | working |
| starting | The CLI is ready with no task (`s`) | idle |
| starting | Launch fails: CLI missing, pane dies, or not ready within the timeout | errored |
| starting | A start-up prompt (workspace trust, login) before any hook | waiting |
| working | The CLI asks for approval or input | waiting |
| waiting | The user answers | working |
| waiting | The turn ends after the user declines | finished (idle for Duo) |
| working | Turn ends (any agent but Duo) | finished |
| working | Turn ends (Duo) | idle |
| finished | The user selects the agent (`ViewAgent`) | idle |
| finished, idle | New work: a task as the next turn, or a delivered message | working |
| finished, idle | A turn the user typed straight into the pane | working |
| any running state | Unexpected exit (non-zero status or no status (killed by a signal), or a zero status without the CLI's end hook when the CLI has one; a CLI with no end hook that exits 0 is stopped) | errored |
| any running state | Stopped by the user or Duo, or a normal exit | stopped |
| errored | Stopped (`s`, or by Duo) | stopped |

- **Quiet:** an overlay flag on working and starting only. It is set when an agent has had no activity and no hook event for the quiet threshold (default 5 minutes, from `settings.toml`), and cleared by any activity or hook. A waiting agent stays waiting.
- **Needs you:** waiting agents, quiet agents, errored agents, and every pending card (`CardRaised` adds it, `CardAnswered` removes it). Ordered oldest first by the time each entered its state.
- **Alert triggers:** an agent enters waiting, errored or quiet; a card is raised; an agent other than Duo enters finished. One `Alert` per agent per state change; the alert kinds can be turned off in Settings. Clicking an alert selects the agent where the notifier reports clicks (`notify-send --action` on Linux); `osascript` notifications on macOS can't, so there the alert only informs. When no notifier is found, the TUI shows a footer note instead.

### 4.3 Cost at scale

- **No per-agent polling:** one activity query a second covers every agent (spike S0: about 0.2–0.3 % of a core in the reader at 100 windows).
- **One pane stream, the viewed one (phase 2, spike S3):** the main connection stays `no-output`, so it carries only command replies and the once-a-second activity poll (`no-output` has no per-pane exception, spike S0).
  - **The viewer:** the agent pane's live view (`PaneView`, §3.1) uses a second control client, `attach-session -f no-output,ignore-size`.
  - **Opening a view** is one command line, in this order: `refresh-client -f '!no-output'` (it resets every pane's on/off state, so it comes first), `refresh-client -A '%N:off'` for every other pane, `-A '%P:on'` for the viewed pane, then `display -p '#{cursor_x} #{cursor_y} #{cursor_flag} #{alternate_on}'` and `capture-pane -e -N -p` for the first frame (`-N` keeps the background of trailing blanks).
  - **The stream:** the `%output %P` lines after the capture's reply continue exactly where the capture ends.
  - **Keys and resizes:** `send-keys -t %P` and `resize-window -t @W`, over the same connection. They fail once the window is gone.
  - **A window added while viewing** is turned `off` on its `%window-add`.
  - **Closing:** `refresh-client -f no-output`.
  - **Cost:** about 0.1 % of a core, with keystroke echo at a p95 under 2 ms, at 100 noisy windows on tmux 3.2a and latest. A 50 ms `capture-pane` timer cost about 15 times as much, with its p95 at about 50 ms.
  - **Scrollback:** read once on request (`capture-pane -e -p -S -2000`, 12–23 ms at 100 noisy windows), never on a timer.
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
| starting, working, waiting | Refused; the caller is offered "send as message instead" (§6.3). A starting agent already carries its launch task, and a paste sent before the CLI's input box exists is lost |

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
4. **Start the window:** `Runtime::start` opens the agent's window on `tmux -L duohive`, a server DuoHive starts with `-f /dev/null` and `default-shell /bin/sh`, so the user's tmux config (hooks, `destroy-unattached`) and a non-POSIX login shell can't break replies or the launch script (the cost: attaching users get stock key bindings); the agent becomes `starting`, and its first hook moves it on.

### 5.2 CLI adapters

| | Standing instructions | Hooks | MCP | Model, effort | Duo's chat replies (§6.5) |
|---|---|---|---|---|---|
| Claude Code | `--append-system-prompt` | per-launch `--settings` file (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PermissionRequest`, `Stop`, `StopFailure`, `SessionEnd`) | `--mcp-config` | `--model`; `--effort` | `last_assistant_message` in the `Stop` payload; the transcript at `transcript_path` as fallback |
| OpenCode | `instructions` file list in a per-launch config file named by `OPENCODE_CONFIG` (merged over the user's config) | DuoHive's JS plugin, loaded from that file (`plugin: ["file://<run>/opencode-plugin.js"]`); it runs `duohive hook` with the event JSON on stdin (first `catalog.updated` or first prompt, `session.status` busy, `permission.asked`, `permission.replied`, `session.idle`, `dispose`), and its `config` hook makes `edit` and `bash` ask where the user's value is `allow` or absent (§2 approvals) | the same file's `mcp` | `--model <provider>/<model>`; effort as `agent.build.variant` in the file | `duohive_last_reply`, added by the plugin to the `session.idle` payload from the event stream |

- **v1 CLIs:** Claude Code and OpenCode only. Codex is deferred to after v1; it would be a third adapter with its own spike.
- **Spike first:** each mechanism is verified by a short spike against the pinned CLI version before its adapter is written (S1 Claude Code, S2 OpenCode). Where a mechanism is missing, the fallback is a temporary instructions file in the run directory, never the user's global configuration.
- **Spike S1 (Claude Code 2.1.288):** every mechanism above is a per-launch flag, so no fallback file is needed. Three gaps are covered by other sources: an approval answered "No" ends the turn with no hook (a screen read on settle finds the agent at rest; while waiting, screen reads apply whatever the hook coverage, §4.2); the workspace-trust dialog in a directory Claude Code has not seen appears before `SessionStart` (a screen rule marks the agent waiting `~`, and the user answers it); a crash fires no `SessionEnd` (the pane death, with no exit status for a signal death). Claude Code has no non-interactive model listing (its `/model` picker saves the pick as the user's default, so DuoHive never drives it).
- **Spike S2 (OpenCode 1.18.34):** no fallback file is needed beyond the per-launch config and the plugin, both in the run directory. OpenCode has no command hooks, so DuoHive's plugin maps bus events: the first `catalog.updated` after it loads, or the first prompt's event (`session.created` or `chat.message`) if that comes first, or 3 s after load → `session-start` (OpenCode's `session.created` waits for the first prompt, and the plugin's own load comes about 0.8 s before the input box is drawn: a paste then was lost, one after the first `catalog.updated` was not), `session.status` busy and `permission.replied` → `busy`, `permission.asked` → `needs-input`, `session.idle` → `turn-end` (also after a rejected approval), `dispose` (on `/exit`) → `session-end`. Unlike Claude Code, the user's answer to an approval prompt is reported. The plugin must deliver hooks in event order (unordered spawning let a `busy` arrive after `turn-end`) and send `busy` only on change, and it must never disturb OpenCode: its hooks return at once and queue each delivery on one internal ordered chain; spawn errors (a missing exe throws synchronously), rejections and non-zero exits are caught and dropped, never thrown into OpenCode; the child's output is ignored; a hung delivery is killed after 10 s (a loaded machine can take seconds to start one, so a shorter limit dropped real hooks; deliveries stay serial) and the chain goes on; `dispose` waits at most 1 s for the chain, then sends `session-end` with a bounded synchronous spawn. A crash fires no `dispose` (the pane death, as for Claude Code). The task is passed as `--prompt=<task>` (the positional argument is the project directory, so no `--`). OpenCode's default `build` agent allows every tool without asking (`"*": "allow"`), so with the user's defaults file writes and shell commands do not prompt. **DuoHive makes `edit` and `bash` ask in every OpenCode launch**, because the PRD's rule that commit, push, merge, publish and delete wait for the user, for every agent, through each CLI's own prompts, is otherwise unmet on OpenCode's defaults. It does so in the plugin's `config` hook, not with a static `permission` key: a static key replaces the user's value for that key (spike S2: a user's `"edit": "deny"` became `ask`, and a `bash` pattern object lost its `rm *` deny), while the hook only ever tightens: for each of `edit` and `bash` it gathers the user's rules for that key (last match wins, per pattern for an object) into one key written last in each agent's block, after any `"*"`. The guarded `"*"` is raised from `allow` to `ask` (and added as `ask` when absent, unless the agent's built-in rules restrict the key); the user's specific patterns are kept as written, `allow` included, just as Claude Code honours the user's own allow rules. So every `deny`, `ask` and pattern survives, anything the user did not allow by name asks, and this holds for any key order. It guards only the agents it composes, so DuoHive launches OpenCode with `--agent build` unless the user's flags pass `--agent` (an explicit choice), and a configured `default_agent` is otherwise ignored. Tested against simulated user configs (a property test: the decision after is never looser than before) and live (a write prompted).
- **Model and effort lists:** read from each CLI at runtime, never hard-coded. While loading, the form shows "loading models…"; on failure, the user can type a name. Claude Code (S1): the model aliases and the `--effort` choices are read from `claude --help`, and any other model is typed. OpenCode (S2): `opencode models` lists `provider/model` ids, and `opencode models --verbose` gives each model's `variants` (its efforts).
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

- **Stopping:** DuoHive sends `Escape` (declining any prompt that opened meanwhile) and the CLI's own quit, then kills the window after a grace period. A `waiting` agent gets no keys at all, since a key could answer its open prompt (Enter often selects "Yes"): its window is killed at once. The agent becomes `stopped` either way.
- **Stop keys, measured** (2026-10-04, Claude Code 2.1.289 with `--model haiku`, OpenCode 1.18.34, tmux 3.7c): the runtime waits 150 ms between two keys sent in a row. Sent back to back, a TUI reads the keys as one burst: OpenCode took `Escape` then `/` as one Alt-key with a write approval open, so `Escape` did not decline it and `Enter` answered `Allow once` (2 of 2 runs), and it dropped an `Enter` sent with the `/exit` before it (at rest, 3 of 3 runs; a 20 ms gap was enough in 2 of 2). Claude Code's quit is `C-c` `/exit` `Enter`: a turn interrupted by `Escape` before any output puts its prompt back in the input box, where `/exit` would be appended and sent as a new prompt (3 of 3 runs without the `C-c`). With the pauses, both CLIs exited with status 0 at rest, with a write approval open (declined, no file written) and while working; `Escape` alone declines an open approval in both. The kill after the grace period still covers a CLI that does not quit.
- **Normal exit:** a CLI that exits on its own with its end hook and a zero status leaves its agent `stopped`; the next task starts a fresh session.
- **Failed launch or crash:** the agent becomes `errored`, with the reason (from the launch error, or the exit status kept by tmux's `remain-on-exit`; a CLI killed by a signal has no exit status and shows "exited unexpectedly") on its row. It stays in Needs you until the user gives it work (a fresh launch) or stops it.

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
| `start_agent` | Duo | Give work to an agent (§5.1): a fresh launch (directory or worktree) or the next turn; refused for starting, working or waiting agents |
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
- **Replies:** each adapter extracts Duo's reply at the end of a turn (§5.2): Claude Code from `last_assistant_message` in its `Stop` hook payload (spike S1; the transcript at the payload's `transcript_path` gives the same text and is the fallback); OpenCode from `duohive_last_reply`, which DuoHive's plugin adds to the `session.idle` payload: the text parts of the turn's last assistant message, kept from the event stream (spike S2; OpenCode's own API gives the same text and is the plugin's fallback). Each is verified in that CLI's spike.
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
- **Hand edits:** `org.toml` is re-read on start and on reload (`o`). A file that doesn't parse shows its line and error, and the last good org stays in use. A hand edit never kills a live session: a reload that would remove a running agent is refused like a broken file (banner `<agent> is running · stop it, then press o to reload`), and a start-up where a running agent's session has no agent in the loaded org starts on that org without marking it last-good, keeps the session row, refuses edits and shows a banner until the agent is added back or stopped; a later reload that names it adopts it.
- **Single instance:** DuoHive holds an exclusive lock (`File::try_lock`) on `<data_dir>/run.lock` for the life of the hook listener, so a second instance exits at once; a probe that connects to `run.sock` would race. The lock is taken before any agent is spawned, so a spawned child can't inherit it.
- **Quit and shutdown:** `q`, SIGINT, SIGTERM and SIGHUP all quit with the agents left running. Shutdown stops the hook listener first (a hook then falls back to `pending_hooks` in the database), then drains the hook inputs the core still has queued into `pending_hooks`, and releases `run.lock` last; a second signal exits at once, and the whole shutdown is time-bounded. Hooks stored this way are replayed in order of `sent_at_ms`, the time `duohive hook` stamped when it fired (then by id), not the time they were stored. Live hooks can also arrive out of order (the listener serves connections concurrently), so the core keeps, per launch, the newest `sent_at_ms` it applied: a hook stamped earlier than that still records its reply but moves no state (a `sent_at_ms` of 0, unknown, applies as it comes).
- **Session rows:** a `stopped` agent's session row is deleted; its token dies with it.
- **Logs:** log files are created `0600`; `hook.log` rotates once, to `hook.log.1`, at 1 MiB.
- **Validation:** agent names must be unique; the tree has up to three levels (team → role → agent), and teams are optional; every CLI must be supported. Each error names the field.
- **Missing directory:** makes only that agent invalid, with the error on its row; the rest of the org loads and runs.

**`org.toml` schema.** Nested arrays of tables; every table rejects unknown keys, so a fourth level (an agent under an agent) fails to parse with its line.

```toml
[org]                       # org defaults
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

[[role]]                    # roles directly under Duo
name = "Generalist"
[[role.agent]]
name = "squad-1"
```

A save writes `org.toml.tmp`, syncs it, copies the current file to `org.toml.bak`, renames the tmp over `org.toml`, then refreshes `<data_dir>/org.last-good.toml`. Loading never writes the last-good copy; the engine records an org it accepted through `Store::mark_last_good`. A parse error reports the 1-based line and the error's own text (for a bad CLI, `unknown CLI "claud"`).

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
| tmux runtime | Integration tests on a real tmux with a throwaway socket. They cover start, send, no-output silencing, activity polling, exit statuses and pane reads |
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
- **Phase 1:** a basic `bench` (10 agents; idle CPU, memory and hook-to-screen time) runs from the start, so the `no-output` silencing (spike S0) and settle choices are measured early.
- **Release gate:** CI runs the full benchmark on every release. Results on macOS arm64 (Apple M-series) must meet every PRD target, or the release fails; Linux x86_64 results are reported but don't block.

## 11. CI and release

- **CI:** GitHub Actions runs `cargo fmt --check`, `clippy -D warnings`, and tests on macOS and Linux against tmux 3.2 and the latest tmux.
- **Releases:** `cargo-dist` builds from a version tag, publishing GitHub Release binaries, a Homebrew tap and `cargo install duohive`.
- **Dependencies:** Ratatui and crossterm are pinned and upgraded on purpose, and the minimum supported Rust version is declared.

## 12. Delivery mapping

| PRD phase | Built here |
|---|---|
| 1. Org and agents | Spike S0 and the adapter spikes (S1 Claude Code, S2 OpenCode); `duohive-core`, `duohive-store`, `duohive-tmux`, `duohive-cli` (the Claude Code and OpenCode adapters); a minimal org tree with live status and the team, role and agent forms in `duohive-tui`; a basic `duohive bench` |
| 2. One screen | `duohive-tui`: three-pane layout, agent pane (watch, type and zoom), Needs you, notifications and their Settings, Give work with the worktree option; the `PaneView`, `Notifier` and `Git` ports and their adapters; the UI/UX improvement pass at the end |
| 3. Duo and messaging | `duohive-mcp` and token-checked tool rights (tokens exist since phase 1), inbox, Duo and Duo chat, Duo's start and stop, visible actions, approval cards, task to a role |
| 4. Scale and polish | Performance targets at 100 agents on the release benchmark, silencing and activity-poll tuning, first-run setup with templates, model and effort picker, key and mouse completeness, the key-binding editor, theme `auto` and the accessibility re-check |
| 5. Open-source launch | The pre-launch UX check, CI release gate, `cargo-dist` release, docs |

## 13. Risks

| Risk | Mitigation |
|---|---|
| A CLI changes its hooks or flags | Adapters pinned to tested versions, spikes before each adapter, a notice for untested versions, screen reading as fallback |
| tmux control mode differs across versions | Support 3.2 and newer only; CI on 3.2 and latest |
| The settle mechanism doesn't work as planned: the one-a-second `list-windows` activity query for all agents (chosen by spike S0 over format subscriptions) reports too slowly or costs too much at 100 windows | Spike S0 measured it before `duohive-tmux` (median delay 0.48–0.53 s, reader about 0.2–0.3 % of a core at 100 windows); the basic `bench` re-measures it in phase 1 |
| Silencing (`no-output`, chosen by spike S0) does not quiet busy panes enough at 100 agents | Measured by the basic `bench` in phase 1; fallback: pause unselected panes harder and read on settle only |
| Ratatui pre-1.0 breaking changes | Pinned version, deliberate upgrades |
| Users whose agents run on Codex can't use DuoHive for them in v1 | Codex is deferred to after v1, not ruled out: it is one more adapter and spike behind the same `CliAdapter` port |
| Single process: work pauses while DuoHive is closed | Accepted for v1; the command/event boundary makes a background service a later move |

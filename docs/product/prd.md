# DuoHive — Product Requirements

> Run your AI coding agents as a team, not a pile of terminals.
>
> **Vision: an Engineer Factory.** Define your team once, break each story into small, well-specified subtasks, and the right agent picks up each one and gets it done.

**Status:** Draft v1 · **Owner:** the author · **Last updated:** 2026-10-05
Living document, updated in place. It says *what* DuoHive must do and *why*; the technical design is a separate, later document.

---

## Problem Statement

Developers and engineering leads now run several AI coding agents at once (Claude Code, OpenCode, Codex), but every tool for that treats agents as a flat list of throwaway terminal sessions. With 5–10 agents it is already hard to see who is working, who is stuck waiting for an approval, and what each one was told to do; with dozens, the tools themselves get heavy and slow. The cost is idle agents nobody noticed, instructions retyped for every session, and a manager who spends their attention babysitting panes instead of directing work.

## Evidence

- The author runs **5–10 agents a day** today and expects to need **10–100** on a capable machine.
- **Every tool in the market is a session manager.** Claude Squad, Conductor, Crystal (now Nimbalyst), Vibe Kanban and similar tools give each task a fresh session, usually in its own git worktree. Agents don't keep a lasting identity, and coordination is routed through the user ([comparison](https://munderdiffl.in/blog/claude-code-orchestration-tools-compared/), [roundup](https://parallelcode.app/blog/multi-agent-coding-tools-2026/)).
- **Only one tool lets agents coordinate, and it covers one CLI.** Claude Code Agent Teams has a lead session that coordinates teammate sessions. It is Claude-only, experimental, and its teams last one run.
- **Plain tmux shows no status:** it neither shows agent status nor notifies when an agent needs input, which is the most-cited reason people adopt a manager ([HN discussion](https://news.ycombinator.com/item?id=49107749)).
- **The overhead grows with the agent count:** guides recommend 3–5 agents at most because of coordination overhead ([ClawTab](https://clawtab.cc/articles/claude-code-parallel-agents-tmux)), which DuoHive aims to remove rather than accept.
- **The primary user's own answers** (the author, an engineering manager):
  - **Same work, same agent:** the same kinds of work go to the same kind of agent again and again, for example a backend agent per repo.
  - **Same instructions:** most tasks reuse the same skill prompt.
  - **Teammates over tasks:** they would rather pick a teammate than start an anonymous task.
  - **One task per agent feels natural:** tickets should be small, with clear acceptance criteria and a spec, and a story splits into subtasks (backend, QA, frontend) that a manager assigns to the right agent on the team.
- *Assumption, needs validation (3–5 conversations with other leads running 5+ agents):* other managers work the same way. Developers who think only in parallel tasks may find the org set-up heavier; see Open Questions.

## Proposed Solution

DuoHive is an open-source terminal app that runs your agents as an **org chart**: teams, roles and **standing agents**, each with its own name, instructions, directory and CLI (Claude Code or OpenCode in v1), which you steer from **one screen**. At the root of the tree sits **Duo**, a manager agent you chat with; Duo can see the whole org, message any agent and start or stop them, so you direct the team in plain language instead of hopping between panes. DuoHive stays light enough to forget it is running, from 10 to 100 agents.

We chose standing agents in an org chart over the market's session-per-task model because the problem is not "run tasks in parallel", which existing tools do well. The problem is "manage a team of agents over days": knowing who does what, giving each one standing instructions once, and seeing the whole team at a glance.

## Key Hypothesis

We believe that **an org chart of standing agents, a manager agent you chat with, and one screen showing the whole team** will **cut the effort of running many coding agents** for **engineering leads and developers who run 5+ agents a day**.

We'll know we're right when **the author replaces their current tool with DuoHive for daily work for 30 consecutive days**, and **a new user goes from install to 3 working agents in under 10 minutes**.

## What We're NOT Building (v1)

| Not building | Why |
|---|---|
| Agents that delegate to agents (a chain of command) | The tree organises; you direct. Autonomous delegation is a different, riskier product (cost, loops, trust) |
| Jira or any ticket system | Work is given in plain language, directly or through Duo; integrations come after the core is proven |
| Reviving or forking an agent's old conversation | A stopped agent starts fresh. Simpler, and enough to test the hypothesis. Resuming an old session is planned for v1.1 (see *Resume an old session*); forking is not planned |
| Diff review inside DuoHive | Agents and git already cover review; not core to "managing a team" |
| Shared task lists and file locks | Duo and messaging cover coordination for v1 |
| Shell terminals nested under agents | Not needed to manage agents |
| Codex | Deferred to after v1: focus on two CLIs, Claude Code and OpenCode, done well; Codex is a later candidate |
| Custom CLIs beyond Claude Code and OpenCode | Two well-supported CLIs first; pluggable CLIs later |
| Windows, accounts, cloud sync, telemetry | Local-first and private; Windows lacks tmux (WSL2 best effort) |

## Success Metrics

| Metric | Target | How measured |
|---|---|---|
| **Daily use (primary)** | Author uses DuoHive as the only agent tool for 30 consecutive days | Self-report log |
| Time to first team | Install → 3 working agents in < 10 min, README only | 3 first-time users, timed |
| Idle CPU | < 1% at 10 agents · < 3% at 100 | Built-in benchmark, each release; gated on Apple M-series (Linux numbers are reported but don't block a release) |
| DuoHive memory | < 20 MB at 10 agents · < 60 MB at 100 | Same benchmark (DuoHive only, agents excluded) |
| Status freshness | Reported (hook) states on screen < 1 s at 10 · < 2 s at 100; screen-read (`~`) states < 2 s | Same benchmark |
| Start-up | First frame < 100 ms at 10 · < 300 ms at 100 | Same benchmark |
| Binary size | < 10 MB | Release artifact |

## Open Questions

None. All were resolved on 2026-10-03; each answer is in the Decisions Log. Interviews with 3–5 other leads before launch remain worth doing, but no longer block the design.

---

## Users & Context

**Primary user**

- **Who:** an engineering manager or tech lead who runs 5–10 AI coding agents on Claude Code or OpenCode across several repos, often for different projects or teams.
- **Current behaviour:** opens agents in tmux panes or a session-manager TUI, retypes each agent's instructions, and checks panes one by one to find who is stuck.
- **Trigger:** a new batch of work (a sprint, a set of fixes) to spread across agents, or the moment several agents are waiting at once and it's unclear which.
- **Success state:** opens one screen, sees the whole team, asks Duo what is blocked, and unblocks it, without opening a single pane unless they want to.

**Job to be done**

When **I have more work than I can do and a team of AI agents to do it**, I want to **organise them like a real team and direct them from one place**, so I can **keep every agent busy on the right thing without babysitting terminals**.

When **a story ticket comes in**, I want to **break it into small subtasks (backend, QA, frontend) with clear acceptance criteria and hand each to the right agent**, so I can **ship the whole story without doing every part myself**.

**Non-users (v1)**

- **Someone running one agent at a time:** the CLI alone is enough.
- **Someone who wants a GUI, a web board or cloud agents:** DuoHive is local and terminal-first.
- **Someone who wants fully autonomous multi-agent pipelines:** v1 keeps a human in charge.
- **Windows-native users:** tmux is required.
- **Codex-only users:** v1 supports Claude Code and OpenCode; Codex is deferred to after v1.

**Secondary user: the task runner.** A developer who just wants several tasks run in parallel and doesn't care which agent does which. They start from the **Agent Squad** template and give tasks to its Generalist role, never to a named agent, so the org model costs them nothing.

---

## Solution Detail

### Core capabilities (MoSCoW)

| Priority | Capability | Rationale |
|---|---|---|
| **Must** | Org chart: teams → roles → standing agents, edited in-app, saved to a shareable file; one org per install | The core of the hypothesis |
| **Must** | Agent definition: name, CLI (Claude Code or OpenCode), flags, directory, prompt, model, effort; inherited from team and role, prompts stack | Instructions written once, not per session |
| **Must** | One screen: org tree with live status and last reply, the selected agent's live terminal, chat with Duo | See and steer the whole team at a glance |
| **Must** | Duo, a manager agent: chat, see the org, message agents, start and stop them, and propose org changes you approve through approval cards; every action visible | Direct the team in plain language |
| **Must** | Agent-to-agent messages, delivered only when the receiver is at rest. Each sender is identified by a per-launch secret token DuoHive issues, never by a name anyone can set | Duo, and agents, can coordinate safely; no agent can pose as Duo to start a delegation chain |
| **Must** | "Needs you" list (waiting, quiet and errored agents, and pending approval cards) and desktop notifications | Nobody stuck unnoticed |
| **Must** | Honest status: exact where the CLI reports its state, marked `~` where DuoHive is reading the screen, and "quiet" for an agent unchanged too long | You always know how much to trust a status, and a silent agent can't hide |
| **Must** | Light at scale: the targets in Success Metrics | A requirement, not a feature: the reason to rebuild |
| **Must** | Agents keep running when DuoHive closes; reopening reconnects | Standing agents must outlive the app |
| **Should** | Give work to an agent with one key (`space`); type straight into an agent's pane | Fast steering without the chat |
| **Should** | Worktree option when giving work: run the task in the agent's own directory, or in a fresh git worktree on a new branch, removed automatically once that branch is merged | Two agents on one repo don't collide; the manager picks per task |
| **Should** | Model and effort picked from what each CLI offers | Choices stay current with the CLIs |
| **Should** | Every action by key and by mouse; `?` key map | Usable by everyone |
| **Must** | Setup templates: pick Agent Squad, Product Team, Engineering Org or Blank to get a ready-made org, then customise anything | Managers and task runners both start in seconds; advanced users build their own |
| **Must** | Give a task to a role: a free agent in it takes the task; if none is free, Duo proposes a new agent through an approval card | Task runners never have to pick a teammate |
| **Could** | Zoom the agent pane; compact tree view | Comfort at high agent counts |
| **v1.1** | Resume an old session: pick a past Claude Code or OpenCode session for an agent's directory, including ones started outside DuoHive, and the agent continues it with its full context | Finish work an earlier session started without briefing the agent again |
| **Won't (v1)** | Delegation chains, Jira, revive or fork (resume comes in v1.1), diff review, shared tasks and locks, nested terminals, Codex (deferred to after v1), custom CLIs | See *What We're NOT Building* |

### The org model

```
You (the manager)                ← root; your manager agent, Duo, lives here
├── Team: Payments               ← optional grouping (team or project)
│   ├── Role: Backend            ← a role holds one or more agents
│   │   ├── auth-service         ← agent (standing teammate)
│   │   └── billing-api
│   └── Role: QA
│       └── pay-qa
└── Team: Growth
    └── Role: Backend
        └── web-app
```

- **Levels:** three fixed levels under you: team → role → agent.
  - Teams are optional: a role can sit directly under you.
  - Agents never sit under other agents.
- **Standing agent:** a named teammate with **at most one live session at a time**. To do more in parallel, add another agent to the role.

| Agent field | Meaning |
|---|---|
| Name | Unique; how you address it (`@auth-service`) |
| CLI | Claude Code or OpenCode |
| Flags | Extra options passed to the CLI |
| Directory | Where it works |
| Prompt | Its standing instructions |
| Model, effort | Chosen from what the CLI offers |
| Worktree default | Whether new work runs in a fresh git worktree (on/off); you can change it each time you give work |
| Status (live) | stopped · starting · working · waiting on you · finished · idle · errored, plus **quiet** as an overlay. Rows say "waiting"; definitions say "waiting on you". A `~` before the state (`~● working`) means DuoHive is reading the screen rather than hearing it from the CLI |

- **States:**
  - **idle:** at rest with a live session and nothing new.
  - **finished:** the turn just ended and you haven't looked; it becomes idle when you select the agent. Duo goes straight to idle after a turn and never raises "finished" alerts.
  - **quiet:** a working or starting agent with no output and no hook for the quiet threshold (default 5 minutes, a setting). It replaces the working glyph. A waiting agent stays waiting.
  - **errored:** a failed launch or a crash (an unexpected exit). It shows the reason and stays in Needs you until you give it work or stop it. A normal exit is stopped.
- **Inheritance:** a team or role can set any agent field as a default for everything under it.
  - **Prompts stack:** org, then team, then role, then agent. For example, "API URLs use hyphens" is written once, on the Backend role.
  - **Everything else:** the nearest value wins.
- **Lifecycle:**
  - **Starting:** an agent starts when it is given work, or when you press `s` on it (it starts with no task and waits, idle). It never starts just because DuoHive opened, so 100 defined agents cost nothing until used.
  - **Giving work, by state:**

    | State | What giving work does |
    |---|---|
    | stopped (or errored) | Starts a fresh session with the task |
    | finished or idle | Types the task in as the next turn of the existing session, which keeps its context |
    | working or waiting | Refused, with the offer "send as message instead"; the message waits until the agent is at rest |

  - **Free:** for a task given to a role, a free agent is one that is stopped, finished or idle.
  - **Stopping:** it stops when you or Duo stop it, or when its CLI exits normally. `s` on a running agent stops it, asking first. Its next task starts a fresh session.
  - **Deleting:** only a stopped agent can be deleted.
  - **Renaming:** only while the agent is stopped and has no worktrees.
  - **Worktrees:** a task run in a worktree keeps it while the work is in progress. DuoHive removes the worktree once its branch is merged and it has no uncommitted changes:
    - **merges detected:** normal, squash and rebase;
    - **merged into:** `origin`'s default branch when there is a remote, else the local default branch;
    - **checked:** when the agent finishes, after start-up, when the worktree list opens, and every 10 minutes;
    - **by hand:** an unmerged or dirty worktree stays until you remove it, and removing one that isn't merged and clean asks first.
- **Messages to a stopped agent:** your own `@agent` message starts it, with the message as its task. A message from another agent waits, shown as `queued · stopped`. Your messages to a working agent also wait until it is at rest.
- **Where the org lives:** in a readable file (`org.toml`) you can share or keep in git. DuoHive writes it, and hand edits are picked up on the next start. Live state (sessions, chat history) stays on your machine.
  - **One org per install in v1.** "New org from template" replaces the current org after a confirmation, keeping a backup.
  - **A missing directory** makes only that agent invalid, with the error on its row; the rest of the org keeps working.
  - **Org defaults vs Settings:** `org.toml` holds the org-level defaults (org prompt, default CLI, model, effort, worktree setting) and Duo's name and CLI, edited through **Edit org** on the Duo row. Settings are personal only: your name, theme, key bindings, alerts and the quiet threshold.

### Duo, the manager agent

Duo is a Claude Code or OpenCode session at the root of your org. "Duo" is the default name, since you and Duo make the duo in DuoHive; you can rename it. Changing Duo's CLI restarts Duo after a confirmation.

| Duo can | Example |
|---|---|
| Chat about anything | A normal AI chat |
| See the org and each agent's screen | "What's blocking the Payments team?" |
| Message an agent and wait for its answer | "Tell auth-service to rebase and run the tests, then tell me what failed" |
| Start and stop agents, choosing directory or worktree | "Start pay-qa on the login regression, in a worktree" |
| Give a task to a role | "Give the flaky date test to the Generalists" |
| Propose an org change (add, edit, move or remove a team, role or agent); it happens only after you approve | "No frontend agent in Payments. Add `pay-web` (Frontend, Claude, ~/code/web)? [Approve] [Edit] [Reject]" |
| **Cannot:** change the org without your approval | The org's structure stays yours |
| **Cannot:** stop a working or waiting agent without your approval | Stopping a finished or idle agent is direct |
| **Cannot:** propose removing an agent that isn't stopped | The core rejects it |
| **Cannot:** approve anything for you | Commit, push, merge, publish and delete wait for you |

- **Tools:**

  | Who | Tools |
  |---|---|
  | Every agent | `list_agents`, `send_message`, `message_status` |
  | Duo only | `read_agent`, `wait_for_agent`, `start_agent`, `stop_agent`, `assign_to_role`, `propose_org_change` |

- **Identity:** each session gets a per-launch secret token, generated by DuoHive, passed in the session's environment and checked by DuoHive on every tool call. It is never a name anyone can set, so no agent can use Duo's tools, and agents can't build delegation chains.

- **Visible actions:** every action Duo takes appears in the chat as an `→` line.
- **Approval cards:** a proposed org change, or Duo stopping a working or waiting agent, shows as a card in the chat with **Approve** (`y`), **Edit** (opens the form pre-filled) and **Reject** (`n`). Nothing changes until you approve. Each pending card is also listed in Needs you, and Duo keeps working while it waits.
- **Approvals stay with you:** commit, push, merge, publish and delete wait for you, for **every agent**, Duo included. Each CLI's own permission prompts enforce this: DuoHive never auto-approves, and Duo cannot approve for you.

### One screen

The exact visuals for every screen are in the [UI/UX spec](../ux/ui-spec.md); this is the main screen.

```
 DuoHive   6 agents · ● 2 working · ◆ 2 need you                                            ? help 
┏━ Org ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓┌─ auth-service · Claude · worktree retry-loop ──┐
┃ ⬢ Duo · your manager agent    ◦ idle       1m  ┃│ ✓ go test ./... (14 passed)                    │
┃ ├─ ▣ Payments                 ● 1 ◆ 1          ┃│ ✓ Ready for review · 3 files changed           │
┃ │  ├─ ◇ Backend               ● 1 ◆ 1          ┃│                                                │
┃▌│  │  ├─ auth-service ✉ 1     ● working    2m  ┃│ ❯                                              │
┃ │  │  │   "Running go test ./..."              ┃│                                                │
┃ │  │  └─ billing-api          ◆ waiting    4m  ┃│                                                │
┃ │  │      "Allow edit to api.go?"              ┃│   (live terminal of the selected agent)        │
┃ │  └─ ◇ QA                                     ┃│   enter type · z zoom · PgUp/PgDn scroll       │
┃ │     └─ pay-qa               ○ stopped    1d  ┃│                                                │
┃ └─ ◇ Generalist              ~● 1 ◷ 1          ┃│                                                │
┃    ├─ squad-1                ~● working    1m  ┃│                                                │
┃    ├─ squad-2                 ✓ finished   3m  ┃│                                                │
┃    └─ squad-3                 ◷ quiet      6m  ┃│                                                │
┣━ Needs you (2) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫│                                                │
┃ ◷ squad-3      quiet      6m                   ┃│                                                │
┃ ◆ billing-api  waiting    4m  "Allow edit to…" ┃│                                                │
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└────────────────────────────────────────────────┘
┌─ ⬢ Duo · ◦ idle ─────────────────────────────────────────────────────────────────────────────────┐
│ 10:05 you   what's blocking the Payments team?                                                   │
│ 10:05 Duo   billing-api is waiting on a file-edit approval; auth-service is mid-test.            │
│             → @auth-service "run the tests again"        queued (auth-service is working)        │
│ ❯ _                                                                                              │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
 tab pane · ↑↓ move · space give work · s start/stop · n new · e edit · z zoom · ? help
```

- **Status you can trust:**
  - **Exact where reported:** Claude Code reports its state through hooks, OpenCode through its events. Those states show plainly.
  - **Marked where read:** where a CLI reports nothing, DuoHive reads its screen and marks the state with `~`.
  - **Quiet agents surface:** a working agent with no output and no hook for the quiet threshold shows "quiet" and joins **Needs you**, so a stuck agent shows up even on a CLI that reports little.
- **Org pane:**
  - a connector-line tree, with status and how long each agent has been in that state;
  - the last reply under each agent, which you can hide for a compact view;
  - roll-ups on teams and roles;
  - a **Needs you** list (waiting, quiet and errored agents, and pending approval cards), oldest first, so nothing waits unseen.
- **Agent pane:** the selected agent's live terminal. `enter` types into it, `ctrl+q` stops typing, and `z` zooms it.
- **Desktop alerts:** for every Needs you entry, and when an agent other than Duo finishes.
- **Duo pane:**
  - **plain text** goes to Duo;
  - **`@agent text`** goes straight to an agent (it waits if the agent is busy, and starts it if it is stopped);
  - **commands:** `/help` and `/agents`;
  - **suggestions** on `/` and `@`;
  - the chat history survives restarts.
- **Moving and sizing:** `tab` or a click moves between panes. On a small terminal, the agent pane shrinks first.

### Setup templates

Templates give a ready-made org in seconds; everything they create stays editable. They are offered at first run and later through "New org from template", which replaces the current org after a confirmation and keeps a backup.

| Template | What you get | For |
|---|---|---|
| **Agent Squad** | One role, Generalist, directly under Duo (no team), with N agents (default 3, from 1 to 100) sharing one directory and CLI. Give a task to the role, and a free agent takes it | Task runners who want parallel work |
| **Product Team** | One team with Backend, Frontend and QA roles, one agent each, with a starter prompt per role | A small team shipping stories |
| **Engineering Org** | Several teams you name, each with Backend and QA roles and its own repo | Managers running many projects |
| **Blank** | Only you and Duo | Advanced users who build their own |

- **Agent Squad tasks:** with a squad, tasks go to the Generalist role, not to an agent (task to a role). Worktrees default to **on**, so squad agents never collide in the shared directory.
- **More templates later:** the set can grow, and saving your own org as a template is a later candidate.

### Resume an old session (v1.1)

**Why:** an author often starts work in Claude Code or OpenCode directly, outside DuoHive. Later they want the repo's standing agent to finish it, and re-briefing it from the beginning throws the context away. Resuming hands the agent that whole conversation.

**How it works:**
- **Asked for, never automatic.** `s` still starts a fresh session. `r` on a stopped or errored agent opens the resume picker; on a running agent it says to stop it first.
- **The picker** looks like Claude Code's own: a search box, a count (`1 of 36`), and one entry per session, newest first, each a title over a line with its age, git branch and size. Search matches the title and the branch.
- **Which sessions:** every session the agent's CLI saved for the agent's directory and for each worktree the agent still keeps, including sessions started outside DuoHive. DuoHive's own sessions are listed the same way. An entry in a worktree names it; resuming runs in the session's own directory and marks that worktree in use again (the worktree rules then apply as for any run there).
  - Claude Code: read from its session files under `~/.claude/projects/`. They are the CLI's internal format, not an API: a version DuoHive can't read shows a note, never a crash, and is added to the tested-versions list like any CLI change.
  - OpenCode: read from `opencode session list --format json`, kept to sessions in those directories (OpenCode lists a whole project). A spike confirms the JSON carries each session's directory and that `--session` resumes it from there.
- **A session live in another DuoHive agent** is marked `in use by <agent>` and can't be resumed: DuoHive records each running agent's CLI session id from its hooks to know this. Two agents can share a directory, so the 5-minute warning alone would miss a session at rest in another agent's pane.
- **Resuming continues the original session itself** (`claude --resume <id>`, `opencode --session <id>`), with DuoHive's per-launch hooks, token and role instructions as for any start. The agent reopens at rest, showing its old conversation, for the author to type; DuoHive types no task. Messages queued while the agent was stopped are delivered at its first rest, as after any start.
- **A session that may be open elsewhere:** DuoHive can't see other terminals, but it sees when a session was last written. One written in the last 5 minutes gets a warning that it may still be open in another terminal, and a confirmation, before it resumes.

**Not in scope:** a picker on every start, continuing in a copy (`--fork-session`), giving a task while resuming, hiding DuoHive's own test sessions, and sessions from other directories.

### UX principles

Every screen is checked against these before it ships.

1. **One glance answers "who needs me?"** The Needs you list and the status column are readable without moving the cursor.
2. **Colour is never the only signal.** Every state has a glyph and a word, so it works in any theme and for colour-blind users.
3. **Works at 80×24.** Panes shrink and drop detail in a set order; nothing overlaps or breaks.
4. **The same key means the same thing everywhere.** `enter`, `esc`, `tab` and `?` behave consistently in every pane and form.
5. **Nothing important is more than one key or one click away.** Common actions sit in the footer; the rest are in `?`.
6. **Calm by default.** No flashing or animation except a working spinner; alerts only when an agent needs you or finishes.

**UX checks.** Screens are checked against these principles twice:

- **UI/UX improvement pass (end of phase 2):** while phase 2 is in daily use, the author keeps a **friction log** (anything slow, confusing or missed). At the end of the phase, a pass fixes the top items in the log and re-checks every screen.
- **Pre-launch check:** before launch, a second check covers the phase 3–4 screens.

### MVP scope

The minimum to test the hypothesis is **every Must row**: an org chart of standing agents on both v1 CLIs (Claude Code and OpenCode), the one screen, Duo with chat, start and stop and approval cards, agent messaging, Needs you and alerts, honest status, setup templates, task to a role, agents that outlive the app, and the performance targets. Phases 1–4 deliver them. Should and Could items ship if they fit; they don't block the 30-day daily-use test.

### User flow: install to a working team

1. **Install:** `duohive` (one binary; needs tmux and at least one agent CLI).
2. **First run:** set-up asks your name, creates Duo, then offers the templates (Agent Squad, Product Team, Engineering Org, Blank). You pick one, fill in its directories and CLI (Claude Code or OpenCode), and the org is ready; edit anything afterwards.
3. **Give work:** select the agent, press `space`, type the task, and choose where it runs: the agent's own directory, or a fresh git worktree on a new branch (the agent's default is pre-selected). The agent starts and its status turns to working. An agent that is finished or idle takes the task as its next turn instead.
4. **Watch:** the tree shows status and last reply. When an agent needs you, it appears under **Needs you** and a desktop alert fires.
5. **Steer through Duo:** ask "what's blocked?" or "tell billing-api to add tests", and Duo answers and acts, showing each action.
6. **Close and come back:** quitting DuoHive leaves every agent running, and reopening shows the team exactly as it is.

### User flow: a story through the factory

1. **Bring the story:** paste or describe a story in the Duo pane: "Story: let users retry a failed payment. AC: …".
2. **Break it down:** ask Duo to split it into subtasks by role. Duo proposes a backend, a QA and a frontend subtask, each small, with its own acceptance criteria.
3. **Review:** you edit or approve the breakdown in the chat.
4. **Assign:** Duo gives each subtask to the right agent on the team (`auth-service` for the backend, `pay-qa` for QA), starting each one, in a worktree where needed, or gives it to a role. Each assignment shows as a `→` line.
5. **Track:** the org tree shows each agent working on its subtask. Ask Duo "where is the retry story?" for a roll-up.
6. **Finish:** as each agent finishes, you see its last reply. Duo reports when every subtask of the story is done. Commits, pushes and merges still wait for your approval in each agent's CLI.

---

## Delivery Phases

Product milestones, each usable on its own; the technical plan maps onto these later.

| # | Phase | Delivers | Status | Depends |
|---|---|---|---|---|
| 1 | Org and agents | Define the org (file, a minimal org tree and the team, role and agent forms), start and stop standing agents on Claude Code and OpenCode, the org tree with live status, a basic `bench` | done 2026-10-04 | – |
| 2 | One screen | Three-pane layout; agent pane (watch, type and zoom), Needs you, notifications and their Settings, Give work with the worktree option; touchpad scrolling and click to select; theme `auto` follows the terminal; the UI/UX improvement pass at the end | in progress | 1 |
| 3 | Duo and messaging | Duo chat pane, agent-to-agent messages, Duo's start and stop, visible actions, approval cards, task to a role | pending | 2 |
| 4 | Scale and polish | Performance targets met at 100 agents on the release benchmark, first-run setup with templates (Agent Squad needs task to a role, phase 3), model and effort picker, key and mouse completeness, the key-binding editor, the accessibility re-check | pending | 3 |
| 5 | Open-source launch | The pre-launch UX check of the phase 3–4 screens, the CI release gate, README, install channels, CONTRIBUTING, name checks done (trademark search in USPTO and WIPO, classes 9 and 42; `duohive.dev` or `duohive.ai`, since `.com` is taken; GitHub org; `duohive` crate), 30-day daily-use test passed | pending | 4 |

- **Phase 1:** the author can run a real team of agents from the tree. Done when the author's daily agents run in DuoHive with correct status, and `bench` reports the basic numbers at 10 agents.
- **Phase 2:** the author never needs to leave DuoHive to watch or answer an agent; a friction log is kept during use, and the UI/UX improvement pass runs at the end of the phase. Done when a day's work needs no `tmux attach`.
- **Phase 3:** the author directs the team through Duo. Done when a "what's blocked, then fix it" round trip happens entirely in chat, and a task given to a role reaches a free agent.
- **Phase 4:** DuoHive meets every target in Success Metrics on the release benchmark.
- **Phase 5:** DuoHive is public, and the key hypothesis is tested (30 days of daily use, under 10 minutes to the first team).
- **v1.1, after v1 is released:** resuming an old session (*Resume an old session*), tracked in the [v1.1 roadmap](../plans/v1.1-roadmap.md). Codex stays a candidate.

---

## Decisions Log

| Decision | Choice | Alternatives | Rationale |
|---|---|---|---|
| Product model | Org chart of standing agents | Session per task (market default); chain of command | Matches "manage a team over days"; confirmed by the primary user, who assigns story subtasks to teammates by role; delegation deferred |
| Supported CLIs | Claude Code and OpenCode in v1; Codex deferred to after v1 (decided 2026-10-03) | Claude Code, OpenCode and Codex in v1 | Two CLIs done well beat three done thinly; Codex stays a later candidate |
| Agent identity | One standing agent, one live session | Agent as a template with many sessions | The tree shows what's really running; simple to reason about |
| Resuming old sessions | v1.1: `r` opens a picker of the sessions in the agent's directory and its kept worktrees, including those started outside DuoHive; the original session continues; one live in another DuoHive agent is refused; one written in the last 5 minutes warns and asks (decided 2026-10-17) | A picker on every start; continuing in a copy; refusing recently active sessions | Gets an earlier session's full context back without re-briefing; the author wants the original continued, with a warning rather than a refusal when it may be open elsewhere |
| Two kinds of user | Setup templates (Agent Squad, Product Team, Engineering Org, Blank) and tasks given to a role | Managers only; task-per-session mode | One product serves managers and task runners; the org model stays intact |
| Hierarchy | 3 fixed levels: team → role → agent | Free-form nesting | Clear, readable, enough for real orgs |
| Inheritance | Prompts stack; other fields nearest-wins | Override everything | Shared rules written once |
| Manager agent | Duo: talk, start and stop agents, and propose org changes you approve | Talk only; start/stop only; full control | A story never stalls on a missing agent, and the org never changes without your say-so |
| Status honesty | Exact states from each CLI; `~` when read from the screen; "quiet" after a threshold, surfaced in Needs you | Always look confident; exact status for fully-hooked CLIs only | The manager knows how far to trust each status, on every CLI |
| Layout | One screen: tree, agent, chat | Separate views | A manager sees and steers without switching |
| Scope | Jira, review, revive, terminals out of v1 | Keep the full feature set | Focus on the hypothesis; a light core |
| Worktrees | An option per task: the agent's directory or a fresh worktree, with a per-agent default | Always a worktree (market default); never | Standing agents usually own a repo; worktrees when agents share one |
| Worktree clean-up | Removed automatically once its branch is merged; never while it holds uncommitted changes | Keep until removed by hand; remove when the agent stops | No pile-up of finished worktrees, and no lost work |
| Stack | Rust + Ratatui, tmux-backed | Go + Bubble Tea; own terminal service | Smallest footprint; tmux keeps agents alive and attachable |
| Name | DuoHive (`duohive`, `dh`); Duo for the agent | Hivly, Cohorai, Myrmi | Personal brand and hive metaphor; clean on registries |
| Branding | Keep DuoHive; lead with "Duo + your Engineer Factory", not "Hive" | Rename (Duocrew, DuoHQ); decide at launch | "Hive" is crowded among agent tools; "Duo" and the factory story set DuoHive apart. No software named DuoHive was found |
| Distribution | Open source, Apache-2.0, own repo | Proprietary; fork an existing tool | The author owns and drives the product |
| States | stopped, starting, working, waiting on you, finished, idle, errored; quiet as an overlay. Finished becomes idle when you look; Duo goes straight to idle | One "done" state; quiet as its own state | You see what's new without losing what's merely at rest; Duo's turns don't raise alerts |
| Giving work by state | Stopped: fresh session. Finished or idle: next turn of the same session. Working or waiting: refused, with "send as message instead". Free means stopped, finished or idle | Always a fresh session; queue silently | Keeps context where it helps and never types over a busy agent |
| Errors | A failed launch or a crash is errored, in Needs you, until you give work or stop it; a normal exit is stopped | Fall back to stopped | A crash can't pass for a clean stop |
| Needs you and alerts | Waiting, quiet, errored agents and pending approval cards, oldest first; alerts for those and for finished (never Duo) | Agents only | Cards wait on you too |
| Tool rights and identity | Every agent: `list_agents`, `send_message`, `message_status`; Duo only: read, wait, start, stop, assign, propose. Identity is a per-launch secret token | Same tools for all; identity by name | No agent can act as Duo, so there are no delegation chains |
| Approvals | Commit, push, merge, publish and delete wait for you for every agent, through each CLI's own prompts; DuoHive never auto-approves | DuoHive-level approval; trust Duo | One rule for all agents, enforced where the action happens |
| Duo stopping agents | Working or waiting: approval card. Finished or idle: direct. Removing a running agent can't be proposed | Always direct; always a card | Stopping live work is your call; tidying idle agents is not worth a card |
| Messages to a stopped agent | Yours start it with the message as its task; an agent's message waits (`queued · stopped`) | Always queue; always start | You can wake an agent by talking to it; agents can't start each other |
| The org file | One org per install; new org from template replaces it with a backup; a missing directory invalidates only that agent | Several orgs; reject the whole file | Simple for v1, and one bad path doesn't take the team down |
| Org defaults vs Settings | `org.toml`: org defaults and Duo's name and CLI (Edit org). `settings.toml`: personal only | All in Settings | The shared file carries what the team shares; personal taste stays personal |
| Phases | Phase 1 adds a minimal tree, the forms and a basic `bench`; phase 3 adds approval cards and task to a role; templates in phase 4 | Templates earlier | Each phase is usable; Agent Squad needs task to a role |
| Names | "Give work" is the one term; the Agent Squad role is Generalist, directly under Duo; rename only while stopped with no worktrees | "Quick prompt"; "Squad" | One word per idea; renaming can't orphan sessions or worktrees |
| Merged worktrees | Normal, squash and rebase merges, against `origin`'s default branch (else local); re-checked on finish, after start-up, when the list opens and every 10 minutes | Normal merges only, on start | Squash and rebase merges are common; nothing piles up |
| Benchmark platform | Targets gated on Apple M-series; Linux reported, not blocking. Hook states < 1 s / < 2 s; screen-read states < 2 s | One gate for all platforms | The author's machine is the target; screen reads are honestly slower |
| UX checks | The phase 2 pass plus a pre-launch check of the phase 3–4 screens | One pass only | Later screens get the same scrutiny |

---

## Research Summary

**Market context**

- **Session managers** run parallel agents in tmux or a desktop app, mostly with git worktrees: Claude Squad (AGPL, tmux, about 8.4k stars), Conductor (macOS, closed source, paid tiers), Crystal now Nimbalyst (MIT, desktop GUI), Vibe Kanban (Apache-2.0, a web board, community-run since Apr 2026). They do parallel tasks well, but their agents are disposable, and coordination goes through the user ([comparison](https://munderdiffl.in/blog/claude-code-orchestration-tools-compared/), [roundup](https://parallelcode.app/blog/multi-agent-coding-tools-2026/)).
- **Built-in teams:** Claude Code Agent Teams (experimental) has a lead coordinating teammates in tmux panes, but it covers Claude only and lasts one run. Codex added a desktop app for managing multiple agents and subagents in Feb 2026 ([overview](https://dev.to/bokuwalily/swarming-claude-code-and-codex-in-parallel-running-multiple-agents-at-once-with-tmux-and-git-1fej)).
- **"Hive" agent tools:** [hive](https://github.com/morapelker/hive), an open-source worktree manager for OpenCode, Claude Code and Codex agents; [Hive](https://www.hivecode.app/), a Rust desktop app whose "Queen" coordinator hands tasks to fixed agent teams (Architect, Coder, Reviewer, Tester), each in a worktree, the closest concept to DuoHive found; [Hive](https://hive-ai.dev/), a command centre with a kanban board. None has a user-defined org of standing agents with a manager agent you approve changes from.
- **The gap DuoHive targets:** a **standing, cross-CLI org of agents with a manager agent**, light enough for 100 agents. None of the tools above combines these.
- **A caution:** most comparison articles are written by competing vendors, so their rankings are biased.

**Performance context**

- **The TUI frameworks:** in a benchmark of the same app built three ways, Ratatui and Bubble Tea each used about 0.7 ms of CPU per frame. Ratatui had the smallest footprint (about 8 MB RAM, 0.6 MB binary) against Ink's roughly 160 MB.
- **What really costs at scale:** watching every agent's output, not drawing the screen. This is why "Status freshness" and "Idle CPU" are Success Metrics.

---

*Status: DRAFT — needs validation (interviews, 30-day daily-use test).*

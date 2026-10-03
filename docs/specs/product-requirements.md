# DuoHive — Product Requirements

Living document. Updated in place as decisions change. It covers what DuoHive is and must do; the technical design is a separate, later document.

## 1. What DuoHive is

DuoHive is an open-source terminal app for running an **org of AI coding agents** on your own machine. You lay out your org as a tree (teams, roles, agents), give each agent its standing instructions, directory and CLI, and steer the whole org from one screen, including through **Duong**, a manager agent you chat with.

It is a new product, built from scratch in its own repository.

**Name:** DuoHive, command `duohive`, short alias `dh`. "Duo" is you and your AI crew; "Hive" is many agents working as one colony.

## 2. Why

| Problem today | What DuoHive must do instead |
|---|---|
| Running many agents costs more CPU, memory and battery as the count grows, and the UI can lag | Stay light at 10 agents and usable at 100 (§8) |
| Status updates and screens feel slow | Show an agent's change of state within a second or two |
| Existing tools are large codebases owned by someone else | Be a product its owner drives, small enough to change quickly |
| Agents are a flat list of sessions | Organise agents as a real org chart, with a manager agent at the top |

## 3. Who it is for

**v1:** Duong and his team: an engineering manager running 5–10 agents day to day, up to 100 on a capable machine. Published openly on GitHub, but not tuned for outside users yet: opinionated defaults, macOS and Linux, three CLIs.

**Later:** engineering leads and developers who run AI agent teams.

## 4. Goals and non-goals

**Goals (v1)**

- G1. One screen to see and steer the whole org: who is working, who is waiting on you, and what each agent last said.
- G2. An org chart, not a session list: teams, roles and standing agents, with settings passed down the tree.
- G3. A manager agent (Duong) you chat with, which can see the org, message agents, and start or stop them.
- G4. Light enough to forget it is running: the targets in §8.
- G5. Agents keep running when DuoHive is closed, and DuoHive picks them back up when reopened.

**Non-goals (v1)**

- Agents that supervise and delegate to other agents (a chain of command). The tree organises; you direct the work, alone or through Duong.
- Jira or any ticket system; `/tickets` and `/assign`.
- Reviving or forking an agent's earlier conversation.
- Reviewing an agent's git diff inside DuoHive.
- Git worktrees per agent.
- Shared task lists and file locks between agents.
- Shell terminals nested under agents.
- Defining arbitrary CLIs beyond Claude Code, OpenCode and Codex.
- Windows (WSL2 is best effort), accounts, cloud sync, telemetry.

## 5. The org model

```
Duong (you, the manager)         ← root; your manager agent lives here
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

**Levels.** Three fixed levels under the root: team → role → agent.

- Teams are optional: a role may sit directly under the root.
- A role holds one or more agents.
- Agents never sit under other agents.

**Agent.** A standing, named teammate with **at most one live session at a time**. To work on more things at once, you add another agent to the role.

| Field | Meaning |
|---|---|
| Name | Unique in the org; how you address it (`@auth-service`) |
| Tool | Claude Code, OpenCode or Codex |
| Command | Extra flags passed to the tool |
| Directory | Where it works |
| Prompt | Its standing instructions |
| Model, effort | Chosen from what the tool itself offers |
| Status | Live, never stored: stopped, starting, working, waiting on you, finished, errored |

**Inheritance.** A team or role can set any agent field as a default for everything under it.

- **Prompts stack.** An agent's instructions are the org prompt, then the team's, then the role's, then its own. A rule like "API URLs use hyphens" is written once on the Backend role.
- **Everything else: the nearest value wins.** That covers directory, tool, command, model and effort: the agent's own, else its role's, else its team's, else the org default.

**Lifecycle.**

- **Starting:** an agent starts when you give it work (a quick prompt, an `@` message, or typing into it), never just because DuoHive opened. So 100 defined agents cost nothing until they are used.
- **Stopping:** it stops when you stop it, when Duong stops it, or when its CLI exits. Its next piece of work starts a fresh session.
- **Deleting:** only a stopped agent can be deleted.

**Where the org lives.**

- **Editing:** the org is edited inside DuoHive, with forms, keys and mouse.
- **Saving:** it is saved to a plain, readable file (`~/.config/duohive/org.toml`) that you can keep in git or share with your team. DuoHive writes it; you never have to edit it by hand, but hand edits are picked up on the next start.
- **Live state:** which session belongs to which agent, and the chat history, is kept separately on your machine, so the shareable file stays clean.

## 6. The manager agent: Duong

The root of the tree is **Duong**, a manager agent: a Claude Code, OpenCode or Codex session you chat with about anything. It is how you talk to your whole org from one place.

| Duong can | Example |
|---|---|
| Answer anything | A normal AI chat |
| See the org: agents, roles, teams, status, and what each agent's screen shows | "What's blocking the Payments team?" |
| Message any agent and wait for its answer | "Tell auth-service to rebase and run the tests, then tell me what failed" |
| Start and stop agents | "Start pay-qa on the login regression" |

| Duong cannot (v1) | Why |
|---|---|
| Create, edit, move or delete teams, roles or agents | The org's structure stays yours; it changes only through your own keys or clicks |

- **Visible actions:** every action Duong takes appears in the chat as an `→` line (`→ start pay-qa`, `→ @auth-service rebase…`), so nothing happens out of sight.
- **Approvals stay with you:** commit, push, merge, publish and delete still wait for you, whichever agent is asked.

## 7. Features (v1)

### 7.1 One screen

Everything a manager needs is on one screen, with three panes always visible.

```
 DuoHive · 12 agents · 3 working · 1 waiting on you                           ? keys
┌ Org ───────────────────────────────────────────┐┌ auth-service · Claude · ~/code/auth-service ────┐
│ ★ Duong · manager                      ● idle  ││ ✓ go test ./... (14 passed)                     │
│ ├─ ▣ Payments                    1 waiting     ││ ● Pushed branch fix/retry-loop                  │
│ │  ├─ ◇ Backend                                ││                                                 │
│ │  │  ├─ auth-service          ● working 2m    ││ ❯                                               │
│ │  │  │   "Running go test ./..."              ││                                                 │
│ │  │  └─ billing-api           ◆ waiting       ││   (live terminal of the selected agent;         │
│ │  │      "Allow edit to api.go?"              ││    enter = type into it)                        │
│ │  └─ ◇ QA                                     ││                                                 │
│ │     └─ pay-qa                ○ stopped       ││                                                 │
│ └─ ▣ Growth                                    ││                                                 │
│    └─ ◇ Backend                                ││                                                 │
│       └─ web-app               ● working 9m    ││                                                 │
│           "Reviewing PR #42"                   ││                                                 │
├ Needs you (1) ─────────────────────────────────┤│                                                 │
│ ◆ billing-api  "Allow edit…"                   ││                                                 │
└────────────────────────────────────────────────┘└─────────────────────────────────────────────────┘
┌ Duong ● idle ─────────────────────────────────────────────────────────────────────────────────────┐
│ 10:05 you    what's blocking the Payments team?                                                   │
│ 10:05 Duong  billing-api is waiting on a file-edit approval; auth-service is mid-test.            │
│              → @auth-service "push when tests pass"                               delivered 10:05 │
│ ❯ _                                                                                               │
└───────────────────────────────────────────────────────────────────────────────────────────────────┘
 tab next pane · enter type into agent · space prompt agent · s start/stop · n new · e edit · z zoom
```

**Org pane.**

- **Tree:** a connector-line tree with Duong at the root. Teams are marked ▣ and roles ◇; agents show by name.
- **Status column:** ● working, ◆ waiting on you, ✓ finished, ○ stopped, ✕ errored, with how long the agent has been in that state.
- **Last reply:** each agent's last reply sits under its name. `v` hides it for a compact view.
- **Folding:** a folded group shows a count and a roll-up (`▣ Growth (3) ● 1 working`).
- **Needs you:** a list under the tree of every agent waiting on you, so nothing hides inside a folded team.
- **Narrow panes:** below about 40 columns, the tree drops its connector lines and indents instead.

**Agent pane.** The selected agent's live terminal.

- **Typing:** typing in the pane goes straight to the agent; `ctrl+q` returns to the tree.
- **Zoom:** `z` zooms the pane to full screen and back.

**Chat pane (Duong).**

- **Plain text** goes to Duong.
- **`@agent text`** messages an agent directly; it is typed in once the agent is at rest, never onto an approval prompt.
- **Commands:** `/help` lists commands; `/agents` lists agents with status.
- **Suggestions:** typing `/` or `@` opens a list to pick from.
- **Answers:** a reply from an agent shows under the message it answers.
- **Size:** the pane grows while you type and can be resized.
- **History:** the chat history survives restarts.

**Moving around.** `tab` (or a click) moves between the three panes.

- **Small terminals:** the agent pane shrinks first; the tree and chat keep at least 4 rows each.

### 7.2 Flows

| Flow | What happens |
|---|---|
| First run | No org yet: a 3-step setup asks your name, creates Duong, then your first team, role and agent |
| Add | New team, role or agent through a form; values inherited from above show greyed until overridden |
| Edit, move, delete | On any node; delete only when its agents are stopped |
| Give work | A one-line prompt to the selected agent, which starts it if stopped |
| Type into an agent | Talk to it directly in its pane |
| Start / stop | Per agent, or through Duong |
| Notify | A desktop alert when an agent waits on you or finishes; clicking it selects that agent |
| Settings | Default tool, model and effort; key bindings; notifications; theme |

### 7.3 Agents talking to agents

Agents, including Duong, can message each other through DuoHive's tools (MCP): send a message, check it arrived, read another agent's screen, wait for it to finish. Messages are queued and delivered only when the receiving agent is at rest.

### 7.4 Every action, by key and by mouse

Every action has a key and a mouse path, and the footer and `?` key map list the keys.

## 8. Quality targets

Measured with a benchmark that ships with DuoHive, on an Apple M-series machine, on every release. DuoHive reports its own cost separately from the agents' cost: each agent CLI uses 100–300 MB of its own, so the real agent count depends on the machine.

| Measure | 10 agents | 100 agents |
|---|---|---|
| CPU while idle | < 1% | < 3% |
| DuoHive's own memory | < 20 MB | < 60 MB |
| An agent's state change shown on screen | < 1 s | < 2 s |
| Start-up to first frame | < 100 ms | < 300 ms |
| Binary size | < 10 MB | |

**Reliability**

- **Closing DuoHive never stops agents;** reopening it reconnects to every one.
- **Your own tmux is never touched:** DuoHive's sessions never mix with your own terminal sessions.
- **No data leaves your machine:** DuoHive itself sends nothing anywhere. Agents talk to their providers as they normally would.

## 9. Platform and install

- **Systems:** macOS and Linux. WSL2 is best effort.
- **Requires:** tmux 3.2 or newer, and the agent CLIs you use (Claude Code, OpenCode, Codex).
- **Install:** a single binary, from `cargo install duohive`, a Homebrew tap, or GitHub Releases.

## 10. Open source

- **Repository:** `github.com/<owner>/duohive`, Apache-2.0.
- **Before launch:**
  - trademark check for "DuoHive" (GitLab Duo and Cisco Duo are in the developer-tools market);
  - claim the `duohive` crate and repository name;
  - pick a domain;
  - write a CONTRIBUTING guide and issue templates.

## 11. Success measures

- **Daily driver:** Duong runs his daily work (5–10 agents) on DuoHive instead of his current tool.
- **Performance:** every target in §8 is met on the release benchmark.
- **Org from scratch:** a new user can install DuoHive, create an org of 3 agents and give each work in under 10 minutes, following only the README.

## 12. Risks

- **Status detection varies by CLI.** Each CLI reports its state differently, and some report little, so status for those CLIs may be less exact.
- **Scale is bounded by the CLIs.** 100 agents depends on the machine more than on DuoHive.
- **Name.** "Duo" overlaps with established developer-tool brands; see §10.

## 13. Open items

- **UI/UX improvement pass** on the one-screen layout and the org tree, after v1 is in daily use.
- **Duong's control:** stays at "talk, plus start and stop agents"; full control over the org's structure is a later decision.
- **After-v1 candidates:** revive and fork; Jira; diff review; worktrees; chain of command (agents that delegate to agents).
- **Pre-launch name checks:** trademark, crate, repository and domain.

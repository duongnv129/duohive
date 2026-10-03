# DuoHive — UI/UX Spec

**Status:** Draft v1 · **Last updated:** 2026-10-03
Living document, updated in place. It is the source of truth for what every screen looks like and how it behaves. The [PRD](../product/prd.md) says what DuoHive does and why; the [technical design](../design/technical-design.md) says how it is built.

---

## 1. Design direction

| | |
|---|---|
| **Purpose** | Steer a team of AI agents from one screen |
| **Audience** | A manager who glances at it many times a day; the first question is always *who needs me?* |
| **Tone** | A dense, calm, technical control room. Quiet by default; loud only when someone needs you |
| **Memorable detail** | **Amber means "needs you."** It is the hive's honey colour, the one warm accent in a cool, neutral palette, so the eye goes straight to what is waiting. Duo's glyph is a honeycomb cell, `⬢`. |
| **Constraints** | 80×24 minimum. Colour is never the only signal. Every action works by key and by mouse. The same key does the same thing in every pane. |

**Anti-patterns:**

- **No decoration:** no borders inside borders, no ASCII art, no banners.
- **No motion except the working spinner.**
- **No feature descriptions on screen:** the controls and the `?` map speak for themselves.

## 2. Visual language

### 2.1 Status

Each state has a glyph and a word, so it reads without colour.

| Glyph | State | Colour token | Shown in Needs you |
|---|---|---|---|
| `●` | working | `status.working` (cool blue) | no |
| `◆` | waiting on you | `status.attention` (**amber**) | yes |
| `◷` | quiet (no change for the threshold) | `status.attention` (**amber**) | yes |
| `✕` | errored | `status.error` (red) | yes |
| `✓` | finished | `status.done` (green) | no (desktop alert instead) |
| `◌` | starting | `status.muted` | no |
| `○` | stopped | `status.muted` | no |
| `~` | prefix: state read from the screen, not reported by the CLI (`~ working`) | inherits the state's colour, dimmed | – |

### 2.2 Tree glyphs

| Glyph | Node |
|---|---|
| `⬢` | Duo, at the root of your org |
| `▣` | Team |
| `◇` | Role |
| (none) | Agent: just its name |
| `▾` `▸` | Unfolded / folded group (shown in the compact tree) |

### 2.3 Colour tokens

Themes map tokens to colours, and every token pair meets 4.5:1 contrast.

| Token | Use | Default (dark) | Default (light) |
|---|---|---|---|
| `fg` / `fg.muted` | Text / secondary text, inherited values | #D6DAE0 / #7D8590 | #1F2328 / #6E7781 |
| `bg` / `bg.panel` | Screen / focused pane | #0F1115 / #161A21 | #FFFFFF / #F6F8FA |
| `border` / `border.focus` | Pane borders / focused pane | #2A2F38 / #5B8DEF | #D0D7DE / #2F6FEB |
| `status.working` | ● | #5B8DEF | #2F6FEB |
| `status.attention` | ◆ ◷, Needs you, approval cards | **#F2B33D** (amber) | **#B7791F** |
| `status.done` | ✓ | #3FB950 | #1A7F37 |
| `status.error` | ✕, error lines | #F85149 | #CF222E |
| `accent.duo` | ⬢ and Duo's name | #C9A227 (honey) | #9A6700 |
| `selection` | Selected row | bg #24324A | bg #DDF4FF |

### 2.4 Text

- **Names:** agent names are monospace identifiers (`auth-service`), never title-cased.
- **Times:** short form (`2m`, `4h`); absolute times only in chat (`10:05`).
- **Last reply:** the agent's last reply goes under its row, in quotes, cut with `…` at the pane edge.
- **Labels:** sentence case everywhere ("Needs you", "Give work").

## 3. Layout

The main screen has three panes (§4.1).

| Width × height | Org pane | Agent pane | Chat pane |
|---|---|---|---|
| ≥ 120 × 36 | 50 cols, full tree with last replies | rest | 6 rows, grows to 12 while typing |
| 100–119 × 30 | 50 cols | rest | 6 rows |
| 80–99 × 24 | 32 cols, compact tree (no connectors, no last replies) | rest | 4 rows |
| < 80 × 24 | — | — | — |

Below 80×24, DuoHive shows only "Make the window at least 80×24" and the current Needs you count.

- **Shrink order:** on a small terminal, the agent pane loses detail first, then the last-reply lines go, then the tree's connector lines. Needs you and the chat input never go.
- **Focus:** the focused pane has a `border.focus` border and is named in the footer.
- **Resizing:** drag the chat pane's top edge, or use `ctrl+↑` / `ctrl+↓`.

## 4. Screens

### 4.1 Main screen

```
 DuoHive   7 agents · ● 2 working · ◆ 1 needs you · ◷ 1 quiet                               ? keys 
┌─ Org ──────────────────────────────────────────┐┌─ auth-service · Claude · worktree retry-loop ──┐
│ ⬢ Duo · your manager agent       ● idle        ││ ✓ go test ./... (14 passed)                    │
│ ├─ ▣ Payments                    ◆ 1 needs you ││ ● Pushed branch duohive/auth-service/retry-loop│
│ │  ├─ ◇ Backend                                ││                                                │
│ │  │  ├─ auth-service          ● working 2m    ││ ❯                                              │
│ │  │  │   "Running go test ./..."              ││                                                │
│ │  │  └─ billing-api           ◆ waiting  4m   ││                                                │
│ │  │      "Allow edit to api.go?"              ││   live terminal of the selected agent          │
│ │  └─ ◇ QA                                     ││   enter: type into it · z: zoom                │
│ │     └─ pay-qa                ○ stopped       ││                                                │
│ └─ ▣ Growth                                    ││                                                │
│    └─ ◇ Squad (3)              ● 1 ◷ 1         ││                                                │
│       ├─ squad-1               ~ working 1m    ││                                                │
│       ├─ squad-2               ✓ finished      ││                                                │
│       └─ squad-3               ◷ quiet   6m    ││                                                │
├─ Needs you (2) ────────────────────────────────┤│                                                │
│ ◆ billing-api   waiting 4m  "Allow edit…"      ││                                                │
│ ◷ squad-3       quiet 6m                       ││                                                │
└────────────────────────────────────────────────┘└────────────────────────────────────────────────┘
┌─ ⬢ Duo · ● idle ─────────────────────────────────────────────────────────────────────────────────┐
│ 10:05 you   what's blocking the Payments team?                                                   │
│ 10:05 Duo   billing-api is waiting on a file-edit approval; auth-service is mid-test.            │
│             → @auth-service "push when tests pass"                       delivered 10:05         │
│ ❯ _                                                                                              │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
 tab pane · enter type · space task · s start/stop · n new · e edit · z zoom · ? help
```

- **Header:**
  - the counts by state; clicking a count filters the tree to it;
  - `? keys` opens the key map.
- **Org pane:**
  - the tree, with each agent's state, time in state and last reply;
  - roll-ups on teams and roles (`◆ 1 needs you`, `● 1 ◷ 1`);
  - **Needs you** is pinned at the bottom of the pane, listing every waiting, quiet or errored agent, oldest first.
- **Agent pane:**
  - the selected agent's live terminal;
  - its title shows the agent, its CLI, and where it runs (directory or worktree name);
  - `enter` sends your typing to the agent, and `ctrl+q` returns to the tree.
- **Chat pane (Duo):**
  - plain text goes to Duo, `@name` messages an agent, and `/` opens commands;
  - Duo's actions appear as `→` lines with their results (`delivered`, `started`).
- **Footer:** the keys for the focused pane.

**Compact layout (80×24):** the tree drops connectors and last replies, and the chat shrinks to 4 rows.

```
 DuoHive  ● 2 · ◆ 1 · ◷ 1                                               ? keys 
┌─ Org ────────────────────────┐┌─ auth-service · Claude ──────────────────────┐
│ ⬢ Duo                ● idle  ││ ✓ go test ./... (14 passed)                  │
│ ▾ Payments           ◆ 1     ││ ● Pushed branch                              │
│   ▾ Backend                  ││                                              │
│     auth-service     ● 2m    ││ ❯                                            │
│     billing-api      ◆ 4m    ││                                              │
│   ▸ QA (1)           ○       ││                                              │
│ ▾ Growth                     ││                                              │
│   ▸ Squad (3)        ● 1 ◷ 1 ││                                              │
├─ Needs you (2) ──────────────┤│                                              │
│ ◆ billing-api  4m            ││                                              │
│ ◷ squad-3      6m            ││                                              │
└──────────────────────────────┘└──────────────────────────────────────────────┘
┌─ ⬢ Duo · ● idle ─────────────────────────────────────────────────────────────┐
│ 10:05 Duo  billing-api is waiting on approval.                               │
│ ❯ _                                                                          │
└──────────────────────────────────────────────────────────────────────────────┘
 tab pane · enter type · space task · ? help
```

### 4.2 Zoomed agent

`z` on an agent: its terminal fills the screen, and the footer still shows the Needs you count so nothing is missed.

```
 DuoHive › auth-service (zoomed)                                                            z back 
┌─ auth-service · Claude · ~/code/auth-service ────────────────────────────────────────────────────┐
│ (full-screen live terminal of the agent)                                                         │
│                                                                                                  │
│ ✓ go test ./... (14 passed)                                                                      │
│ ● Pushed branch duohive/auth-service/retry-loop                                                  │
│                                                                                                  │
│ ❯                                                                                                │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
 typing goes to the agent · ctrl+q tree · z restore · ◆ 1 needs you (billing-api)
```

### 4.3 First-run setup

Three steps, shown once, when no org exists. Every field has a sensible default; `enter` on the last field moves to Next.

**Step 1: you and Duo.** DuoHive lists the CLIs it found and their versions.

```
┌─ Welcome to DuoHive · step 1 of 3 ───────────────────────────────────┐
│                                                                      │
│  DuoHive runs your AI coding agents as a team.                       │
│  Duo, your manager agent, helps you steer it.                        │
│                                                                      │
│  Your name     [ Duong_______________ ]                              │
│  Duo runs on   ( ) Claude Code  (•) Codex  ( ) OpenCode              │
│                   found: claude 2.1 · codex 0.9 · opencode 1.4       │
│                                                                      │
│                                         [ Next → ]   [ Quit ]        │
└──────────────────────────────────────────────────────────────────────┘
```

**Step 2: choose a template.** The preview updates as you move.

```
┌─ Choose a starting org · step 2 of 3 ────────────────────────────────┐
│                                                                      │
│  ▸ Agent Squad      3 generalists that take any task                 │
│    Product Team     Backend · Frontend · QA, one agent each          │
│    Engineering Org  several teams, each Backend + QA                 │
│    Blank            just you and Duo; build it yourself              │
│                                                                      │
│  ┄ Preview ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄        │
│  ⬢ Duo                                                               │
│  └─ ◇ Squad                                                          │
│     ├─ squad-1   ├─ squad-2   └─ squad-3                             │
│                                                                      │
│                            [ ← Back ]   [ Next → ]   [ Quit ]        │
└──────────────────────────────────────────────────────────────────────┘
```

**Step 3: fill in the template.** It asks only what the template needs; for Agent Squad, worktrees default to on.

```
┌─ Set up Agent Squad · step 3 of 3 ───────────────────────────────────┐
│                                                                      │
│  Agents        [ 3 ]  (1–20)                                         │
│  Directory     [ ~/code/my-app____________________ ]  ✓ git repo     │
│  CLI           (•) Claude Code  ( ) Codex  ( ) OpenCode              │
│  Worktrees     [x] new work runs in its own worktree                 │
│  Squad prompt  [ You are a careful engineer. Keep changes small; ]   │
│                [ run the tests before you say you are done.     ]    │
│                                                                      │
│  You can change all of this later.                                   │
│                          [ ← Back ]   [ Create org ]   [ Quit ]      │
└──────────────────────────────────────────────────────────────────────┘
```

### 4.4 Team, role and agent forms

Opened with `n` (new) or `e` (edit), or by right-click → New or Edit.

- **Inherited values:** shown as `inherit: <value>` in `fg.muted`; typing overrides them, and clearing a field restores inheritance.
- **Stacked prompts:** the prompts this agent stacks under are shown read-only above its own prompt, so you see everything it will be told.
- **Validation:** directories are checked as you type (`✓ git repo`, `✕ not found`), and `Save` stays disabled until the form is valid.

```
┌─ Edit agent · Payments › Backend › billing-api ──────────────────────────────┐
│                                                                              │
│  Name        [ billing-api_________ ]                                        │
│  CLI         [ Claude Code ▾ ]             inherited from Backend            │
│  Directory   [ ~/code/billing______________ ]  ✓ git repo                    │
│  Model       [ inherit: sonnet ▾ ]          Effort  [ inherit: high ▾ ]      │
│  Flags       [ ______________________________ ]                              │
│  Worktrees   [ inherit: off ▾ ]                                              │
│                                                                              │
│  Prompt  (stacks under: Org › Payments › Backend)                            │
│  ┊ Org      Ask before you delete anything.                                  │
│  ┊ Backend  API URLs use hyphens. Run make test before done.                 │
│  [ Own: Owns the billing service; invoices and retries.        ]             │
│                                                                              │
│  tab next field · ctrl+s save · esc cancel       [ Save ]  [ Cancel ]        │
└──────────────────────────────────────────────────────────────────────────────┘
```

```
┌─ New role · in Payments ─────────────────────────────────────────────┐
│                                                                      │
│  Name       [ Frontend____________ ]                                 │
│  Defaults for its agents (empty = inherit from Payments)             │
│    CLI        [ inherit: Claude Code ▾ ]                             │
│    Directory  [ ~/code/web_______________ ]                          │
│    Worktrees  [ inherit: off ▾ ]                                     │
│  Role prompt  [ You build React UI. Follow the design system.    ]   │
│  Agents       [ 1 ] create this many now (pay-web-1 …)               │
│                                                                      │
│                                          [ Create ]  [ Cancel ]      │
└──────────────────────────────────────────────────────────────────────┘
```

### 4.5 Give work

`space` on an agent, or on a role. The task box takes multiple lines (`alt+enter`).

- **Where it runs:** the choice is pre-set from the agent's worktree default.
- **To an agent:**

```
┌─ Give work · auth-service ─────────────────────────────────────────────────────────────┐
│ Task  [ Add retry with backoff to the payment client. AC: 3 tries, jitter. ]           │
│ Run   (•) new worktree   ( ) ~/code/auth-service        agent default: worktree        │
│                                             enter give · alt+enter new line · esc      │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

- **To a role:** it shows which free agent will take the task; `tab` picks a different one.

```
┌─ Give work · role Squad (2 of 3 free) ─────────────────────────────────────────────────┐
│ Task  [ Fix the flaky date test in billing/time_test.go_____________________ ]         │
│ Goes to  squad-2 (free 12m) · tab to pick another agent                                │
│ Run   (•) new worktree   ( ) ~/code/my-app                                             │
│                                             enter give · alt+enter new line · esc      │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### 4.6 Duo's approval cards and actions

When Duo proposes an org change, a card appears in the chat, outlined in amber.

- **Approve** (`y`) applies the change.
- **Edit** (`e`) opens the form pre-filled.
- **Reject** (`n`) tells Duo it was declined.
- **Cards wait:** while a card is waiting, Duo waits for your answer, and the card is also listed under Needs you.

```
┌─ ⬢ Duo · ● working ──────────────────────────────────────────────────────────────────────────────┐
│ 10:20 you   split story PAY-88 by role and assign it                                             │
│ 10:21 Duo   PAY-88 splits into 3 subtasks. No Frontend agent in Payments yet.                    │
│             ┌ Proposed change ──────────────────────────────────────────────┐                    │
│             │ + Add agent pay-web in Payments › Frontend (new role)         │                    │
│             │   CLI Claude Code · ~/code/web · worktrees on                 │                    │
│             │ [y] Approve   [e] Edit   [n] Reject                           │                    │
│             └───────────────────────────────────────────────────────────────┘                    │
│             → start auth-service "PAY-88a backend: retry endpoint"      started                  │
│             → start pay-qa "PAY-88c QA: retry cases"                     started                 │
│ ❯ _                                                                                              │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 4.7 Chat suggestions

Typing `@` or `/` opens a list above the input. It filters as you type; `tab` or `enter` takes an item, and `esc` closes the list. Agents show their state, so you can see who is free before messaging.

```
┌─ ⬢ Duo · ● idle ─────────────────────────────────────────────────────────────────────────────────┐
│   ┌──────────────────────────────────────────┐                                                   │
│   │ ▸ @auth-service      ● working 2m        │                                                   │
│   │   @billing-api       ◆ waiting 4m        │                                                   │
│   │   @pay-qa            ○ stopped           │                                                   │
│   └──────────────────────────────────────────┘                                                   │
│ ❯ @a_                                                                                            │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 4.8 Confirmations

Asked only for actions that stop work or remove something. `y` confirms, `n` or `esc` cancels.

```
┌─ Stop agent? ────────────────────────────────────────────────┐
│                                                              │
│  Stop auth-service? It is working (2m) in a worktree.        │
│  The worktree and its branch are kept.                       │
│                                                              │
│                               [ y Stop ]   [ n Cancel ]      │
└──────────────────────────────────────────────────────────────┘
```

### 4.9 Worktrees

`w` opens the list of DuoHive's worktrees. Merged-and-clean ones are removed automatically; the rest stay here until you remove them. Removing a dirty worktree asks first.

```
┌─ Worktrees · 4 · removed automatically once merged and clean ────────────────────────────────────┐
│   Agent          Branch                                  State                                   │
│ ▸ auth-service   duohive/auth-service/retry-loop         ● in use                                │
│   pay-qa         duohive/pay-qa/retry-cases              ✓ merged · removing                     │
│   squad-2        duohive/squad-2/flaky-date              ◆ not merged · 2 files changed          │
│   squad-1        duohive/squad-1/readme                  ✓ merged · dirty, kept                  │
│                                                                                                  │
│ enter open in agent · d remove (asks if dirty) · esc back                                        │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 4.10 Settings

`S` opens Settings. Changes save immediately. Key bindings open their own editor.

```
┌─ Settings ───────────────────────────────────────────────────────────────┐
│                                                                          │
│  Duo                                                                     │
│    Name                 [ Duo______ ]                                    │
│    Runs on              [ Codex ▾ ]                                      │
│  Defaults for new agents                                                 │
│    CLI · model · effort [ Claude Code ▾ ] [ sonnet ▾ ] [ high ▾ ]        │
│    Worktrees            [ off ▾ ]                                        │
│  Attention                                                               │
│    Quiet after          [ 5 ] minutes                                    │
│    Desktop alerts       [x] needs you   [x] finished                     │
│  Look                                                                    │
│    Theme                [ auto ▾ ]   Last reply under agents [x]         │
│  Keys                   [ Edit key bindings → ]                          │
│                                                                          │
│  changes save immediately · esc back                                     │
└──────────────────────────────────────────────────────────────────────────┘
```

### 4.11 Key map

`?` from anywhere. It lists the keys for the current bindings, including any you have changed.

```
┌─ Keys · ? to close ──────────────────────────────────────────────────────────────────────────────┐
│ Everywhere                     Org pane                    Agent pane                            │
│  tab   next pane                ↑↓    move                  typing  to the agent                 │
│  ?     this help                ←→    fold / unfold         ctrl+q  back to tree                 │
│  ctrl+c quit (agents keep on)   enter type into agent       z       zoom                         │
│                                 space give work                                                  │
│ Chat (Duo)                      s     start / stop          Mouse                                │
│  enter send                     n     new team/role/agent    click   select / focus              │
│  @     message an agent         e     edit                  right   menu                         │
│  /     commands                 m     move                  wheel   scroll                       │
│  y n   approve / reject card    d     delete (stopped)      drag    move node                    │
│                                 w     worktrees                                                  │
│                                 v     last reply on/off                                          │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 4.12 Errors and empty states

Errors are shown where they apply, with how to fix them.

**tmux missing (full screen, at start):**

```
┌─ DuoHive needs tmux ─────────────────────────────────────────────────┐
│                                                                      │
│  tmux 3.2 or newer is required. Found: none.                         │
│                                                                      │
│    macOS   brew install tmux                                         │
│    Ubuntu  sudo apt install tmux                                     │
│                                                                      │
│  Agents run in tmux, so they keep working when DuoHive closes.       │
│                                                  [ q Quit ]          │
└──────────────────────────────────────────────────────────────────────┘
```

**An agent that can't start (on its row):**

```
┌─ Org ────────────────────────────────────────────────────────────────┐
│ │  ├─ ◇ Backend                                                      │
│ │  │  ├─ auth-service          ✕ errored                             │
│ │  │  │   codex not found · install it or change this agent's CLI (e)│
│ │  │  └─ billing-api           ○ stopped                             │
└──────────────────────────────────────────────────────────────────────┘
```

**`org.toml` doesn't parse (banner; the last good org stays):**

```
 ⚠ org.toml line 42: unknown CLI "claud" · using your last good org · e to open the editor
┌─ Org ──────────────────────────────────────────┐
│ ⬢ Duo                       ● idle             │
│ ├─ ▣ Payments                                  │
│ …                                              │
└────────────────────────────────────────────────┘
```

**An empty role:**

```
┌─ Org ──────────────────────────────────────────┐
│ ⬢ Duo                       ● idle             │
│ └─ ▣ Payments                                  │
│    └─ ◇ Frontend                               │
│        no agents yet · n to add one, or ask Duo│
└────────────────────────────────────────────────┘
```

## 5. Interaction rules

| Rule | Detail |
|---|---|
| One meaning per key | `enter` acts on the selection; `esc` backs out one level, and never quits; `tab` moves focus; `?` opens help; `y` / `n` answer any question |
| Typing goes where focus is | In the agent pane, every key goes to the agent except `ctrl+q`; in the chat, every key goes to the input |
| Mouse parity | Click selects and focuses. Right-click opens the node's menu. The wheel scrolls the pane under the pointer. Dragging a node moves it, and dragging the chat edge resizes it |
| Nothing destructive by surprise | Stop, delete and remove-dirty-worktree ask first; Duo's org changes always come as approval cards |
| Attention is earned | Amber appears only for waiting, quiet, errored and approval cards; desktop alerts only for those and for finished agents (each can be turned off) |
| Calm | No animation except the `●` working spinner; no sound |

## 6. Accessibility

- **State without colour:** every state has a glyph and a word, and colour-blind safe palettes are tested (deuteranopia, protanopia).
- **Monochrome:** with colours disabled (`NO_COLOR`), the focused pane is shown with a double-line border.
- **Contrast:** every text token meets 4.5:1 on its background in both default themes.
- **No blinking text.**

## 7. Copy

- **Short and specific:** say what happened and what to do next ("codex not found · install it or change this agent's CLI (e)").
- **Name the agent:** use the agent's name, not "the agent".
- **"You" and "Duo":** "you" for the user and "Duo" for the manager agent; never "the system".

# DuoHive — UI/UX Spec

**Status:** Draft v1 · **Last updated:** 2026-10-17
Living document, updated in place. It is the source of truth for what every screen looks like and how it behaves. The [PRD](../product/prd.md) says what DuoHive does and why; the [technical design](../design/technical-design.md) says how it is built.

---

## 1. Design direction

| | |
|---|---|
| **Purpose** | Steer a team of AI agents from one screen |
| **Audience** | A manager who glances at it many times a day; the first question is always *who needs me?* |
| **Tone** | A dense, calm, technical control room. Quiet by default; loud only when someone needs you |
| **Memorable detail** | **Amber means "needs you."** It is the hive's honey colour, and it colours only the glyphs of states that need you (focus is the one accent, §2.3), so the eye goes straight to what is waiting. Duo's glyph is a honeycomb cell, `⬢`. |
| **Constraints** | 80×24 minimum. Colour is never the only signal. Every action works by key and by mouse. The same key does the same thing in every pane. |

**Anti-patterns:**

- **No decoration:** no borders inside borders, no ASCII art, no banners. One stated exception: the `@` / `/` suggestion popup.
- **No motion except the working spinner.**
- **No feature descriptions on screen:** the controls and the `?` map speak for themselves.

## 2. Visual language

### 2.1 Status

Each state has a glyph and a word, so it reads without colour. Rows use the short word ("waiting"); definitions and help use the full one ("waiting on you").

| Glyph | State | Meaning | Colour token | In Needs you |
|---|---|---|---|---|
| `○` | stopped | No live session | `fg.muted` | no |
| `◌` | starting | The CLI is launching | `fg.muted` | no |
| `●` | working | Running a turn. Animated as a spinner (below) | `status.working` (`fg`: no colour) | no |
| `◆` | waiting (on you) | The CLI is asking you something | `status.attention` (amber) | yes |
| `✓` | finished | The turn just ended and you haven't looked. Selecting the agent makes it idle | `status.done` | no (desktop alert) |
| `◦` | idle | At rest with a live session and nothing new | `fg.muted` | no |
| `✕` | errored | Failed to launch, or exited unexpectedly; the reason is on its row | `status.error` | yes |
| `◷` | quiet | Overlay: a working or starting agent with no output and no hook for the quiet threshold. Replaces the working glyph | `status.attention` (amber) | yes |
| `~` | marker before the glyph | The state was read from the screen, not reported by the CLI (`~● working`) | none; glyph only | – |

- **Duo:** goes straight to idle after a turn; it is never shown as finished.
- **Working spinner:** the `●` cycles `◐◓◑◒` at 4 frames a second, only on rows on screen. The word stays fixed. With **Reduce motion** on (Settings), it stays `●`. Mock-ups draw the static `●`.
- **State cell:** `~` marker (or a space), glyph, the word padded to 9 cells, then the time right-aligned in 4 cells (`● working    2m`). Times use `2m`, `4h`, `3d`.

### 2.2 Tree glyphs

| Glyph | Node |
|---|---|
| `⬢` | Duo, at the root of your org |
| `▣` | Team |
| `◇` | Role |
| (none) | Agent: just its name |
| `▾` `▸` | Unfolded / folded group. Used for folding only |
| `▌` | Selected row (left bar, plus the `selection` background) |
| `✉ 1` | Messages waiting for this agent |

- **Focused pane:** always drawn with a heavy border (`┏━┓`); other panes use a light one (`┌─┐`). Both are `border`; the focused pane's title takes the accent (`border.focus`), which is added, never relied on.

### 2.3 Colour tokens

The screen exists to show the agents' code, so DuoHive's own chrome stays quiet:

- **One accent, for focus only.** `border.focus` colours the focused pane's title and the selected row's `▌`. Nothing else uses it, and no other accent exists: Duo is bold, not coloured.
- **Colour only where you're needed.** `status.attention`, `status.error` and `status.done` colour their glyph, never the word or the time (which are `fg.muted`). Working, starting, idle and stopped carry no colour: the spinner already says "working".
- **Neutral lines.** Every border, focused or not, is `border`. Focus is the heavy border shape (§2.2) and the accent title. There is one surface colour: `bg.panel` is `bg`.
- **Restrained colour.** One neutral family per theme; accents below 80% saturation.

There are three themes:

- **`auto`, the default, follows the terminal.** DuoHive paints no background: the screen, the focused pane and the selected row show the terminal's own background, and text is the terminal's own foreground. The other tokens are the terminal's ANSI colours, so the terminal's theme decides their exact shades and DuoHive never clashes with it. The selected row is its `▌` bar and bold text, with no background.
- **`dark` and `light`** paint their own palettes. Every text token meets 4.5:1 on `bg` and `selection` in both (§6 lists the ratios).

| Token | Use | `auto` (default) | `dark` | `light` |
|---|---|---|---|---|
| `fg` / `fg.muted` | Text / secondary text, status words and times, inherited values, worktree states | terminal foreground / terminal foreground, dim | #D4D4D8 / #8E8E99 | #27272A / #5E5E68 |
| `bg` / `bg.panel` | Screen / focused pane (the same) | terminal background (not painted) | #111113 | #FAFAFA |
| `border` | Every pane border, focused or not | terminal foreground, dim | #2A2A2E | #E4E4E7 |
| `border.focus` | The accent: the focused pane's title, the selected row's `▌` | ANSI blue | #7DA3E0 | #2F5BB8 |
| `status.working` | ● (no colour of its own) | `fg` | `fg` | `fg` |
| `status.attention` | ◆ ◷ glyphs, Needs you, approval cards | **ANSI yellow** | **#D9A94E** (amber) | **#835B10** |
| `status.done` | ✓ glyph | ANSI green | #6DB585 | #2A6E43 |
| `status.error` | ✕ glyph, error lines, the `org.toml` banner | ANSI red | #E07A7A | #A8323A |
| Duo | ⬢ and Duo's name | bold `fg` | bold `fg` | bold `fg` |
| `selection` | Selected row background | none: `▌` and bold | #1F2430 | #ECEFF5 |

### 2.4 Text

- **Names:** agent names are monospace identifiers (`auth-service`), never title-cased.
- **Times:** short form (`2m`, `4h`, `3d`); absolute times only in chat (`10:05`).
- **Last reply:** the agent's last reply goes under its row, in quotes.
- **Labels:** sentence case everywhere ("Needs you", "Give work").
- **Inherited values:** always `inherit: <value>`, in `fg.muted`.
- **Truncation:**
  - names are cut in the middle with `…` (`billing-…-worker`);
  - pane titles and breadcrumbs are cut from the left (`… › Backend › billing-api`);
  - last replies are cut at the pane edge with `…`;
  - chat lines wrap with a hanging indent under the message text;
  - when the newest chat entry is taller than the history, its first lines show (friction entry 13).

## 3. Layout

The main screen has three panes (§4.1): the org pane (with Needs you pinned at its bottom), the agent pane, and the chat pane.

| Width × height | Org pane | Agent pane | Chat pane |
|---|---|---|---|
| ≥ 120 × 36 | 50 cols, full tree with last replies | rest | 6 rows, grows to 12 while typing |
| 100–119 × 30 | 50 cols | rest | 6 rows |
| 80–99 × 24 | 32 cols, compact tree (no connectors, no last replies) | rest | 4 rows |
| < 80 × 24 | — | — | — |

Below 80×24, DuoHive shows only "Make the window at least 80×24" and the current Needs you count.

**Mixed sizes:** width and height each pick a row, and the smaller row applies: 130×30 uses the 100–119 × 30 row, and 110×24 the 80–99 × 24 row. Below 80 wide or 24 tall, the too-small screen shows.

- **Shrink order:** on a small terminal, the agent pane loses detail first, then the last-reply lines go, then the tree's connector lines. Needs you and the chat input never go.
- **Focus:** the focused pane has a heavy border and is named in the footer. `tab` moves focus: org → Needs you → agent → chat.
- **Padding:** 1 cell inside every border. The selected-row bar `▌` sits in that cell.
- **Resizing:** drag the chat pane's top edge, or use `ctrl+↑` / `ctrl+↓`.

## 4. Screens

### 4.1 Main screen

Six agents plus Duo. The org pane has focus, and auth-service is selected.

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

- **Header:**
  - the agent count and the counts by state; the "need you" count always equals the Needs you list;
  - clicking a count filters the tree to that state (same as `f`);
  - `? help` opens the key map.
- **Org pane:**
  - the tree, with each agent's state cell and last reply;
  - **roll-ups** on every team and role with a non-zero working or attention count (attention = waiting `◆`, quiet `◷` or errored `✕`; `● 1 ◆ 1`); a `~` leads the roll-up when any counted state is read from the screen (`~● 1 ◷ 1`);
  - **Needs you** is pinned at the bottom of the pane (§4.1.3).
- **Agent pane:** the selected node's content (§4.1.2).
- **Chat pane (Duo):** §4.6 and §4.7.
- **Footer:** the keys for the focused pane.

**Compact layout (80×24):** the tree drops connectors and last replies and folds with `▾` `▸`, the state cell keeps glyph and time, and the chat shrinks to 4 rows.

```
 DuoHive  ● 2 · ◆ 2                                                     ? help 
┏━ Org ━━━━━━━━━━━━━━━━━━━━━━━━┓┌─ auth-service · Claude ──────────────────────┐
┃ ⬢ Duo                ◦   1m  ┃│ ✓ go test ./... (14 passed)                  │
┃ ▾ Payments           ● 1 ◆ 1 ┃│ ✓ Ready for review                           │
┃   ▾ Backend          ● 1 ◆ 1 ┃│                                              │
┃▌   auth-service ✉1   ●   2m  ┃│ ❯                                            │
┃     billing-api      ◆   4m  ┃│                                              │
┃   ▸ QA (1)           ○       ┃│                                              │
┃ ▾ Generalist        ~● 1 ◷ 1 ┃│                                              │
┃     squad-1         ~●   1m  ┃│                                              │
┃     squad-2          ✓   3m  ┃│                                              │
┃     squad-3          ◷   6m  ┃│                                              │
┣━ Needs you (2) ━━━━━━━━━━━━━━┫│                                              │
┃ ◷ squad-3            6m      ┃│                                              │
┃ ◆ billing-api        4m      ┃│                                              │
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└──────────────────────────────────────────────┘
┌─ ⬢ Duo · ◦ idle ─────────────────────────────────────────────────────────────┐
│ 10:05 Duo  billing-api is waiting on approval.                               │
│ ❯ _                                                                          │
└──────────────────────────────────────────────────────────────────────────────┘
 tab pane · ↑↓ move · space give work · ? help
```

#### 4.1.1 Filter

`f` cycles a filter by state (◆ need you, ● working, ✓ finished, off). The tree keeps only matching agents and their parents, and a chip shows in the header. `esc` in the org pane clears it. Clicking a header count sets the same filter; the header has counts for working and need you, so finished is reached by `f` only.

- **What matches:** ◆ need you matches the agents that are waiting, quiet or errored; ● working and ✓ finished match that state. The header's `need you` count also includes pending approval cards, which are not agents: the filter never hides a card, and the chip shows no count. The Needs you list and the chat are not filtered.
- **Nothing matches:** the tree shows `no agent is <state> · esc clears the filter` in `fg.muted`, and the chip stays.
- **The selected row is filtered out:** the selection moves to the first remaining match (the agent pane follows), and returns to nothing special when the filter clears.
- **Width:** the chip is `filtered: ◆ · esc clears` at 100 columns and wider; below that it is `filter ◆`; the counts and `? help` are never dropped.

At 80×24 the header uses the compact counts (§4.1) and the chip:

```
 DuoHive  ● 2 · ◆ 2   filter ◆                ? help
```

```
 DuoHive   6 agents · ● 2 working · ◆ 2 need you   filtered: ◆ · esc clears                 ? help 
┏━ Org ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ ⬢ Duo · your manager agent    ◦ idle       1m  ┃
┃ ├─ ▣ Payments                 ● 1 ◆ 1          ┃
┃ │  └─ ◇ Backend               ● 1 ◆ 1          ┃
┃ │     └─ billing-api          ◆ waiting    4m  ┃
┃ └─ ◇ Generalist              ~● 1 ◷ 1          ┃
┃    └─ squad-3                 ◷ quiet      6m  ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
```

#### 4.1.2 Agent pane

The agent pane has two modes.

| Mode | Enter / leave | Keys |
|---|---|---|
| **Selected** | The pane has focus without typing: reached with `tab`, with `ctrl+q` from Typing, and by a click or `enter` on a stopped or errored agent | DuoHive's keys work: `z` zoom, `tab`, `space` Give work, `PgUp` / `PgDn` or the wheel scroll the agent's scrollback, `enter` starts typing (not for a stopped or errored agent), `esc` goes back to the org pane |
| **Typing** | `enter` in the pane, `enter` on a running agent's row, or one click on a running agent's row or in its pane starts it; `ctrl+q` leaves it | Every key goes to the agent, `ctrl+c` included (it interrupts the agent), except while scrolled back (below). The wheel still scrolls. The title shows `TYPING` and the border is heavy |

To zoom while typing, press `ctrl+q`, then `z`.

**Scrolling back:** the wheel (a touchpad's two-finger scroll) goes to the pane under the pointer, without moving focus or changing mode.

- **When the agent's CLI turns on mouse reporting** (Claude Code full-screen does), the wheel and `PgUp` / `PgDn` go to the agent, in Selected and Typing modes alike, and the CLI scrolls its own view. DuoHive takes no snapshot of it.
- **Otherwise,** the wheel, or `PgUp` in Selected mode, freezes a snapshot of the agent's scrollback (up to 2 000 lines), 3 lines per wheel tick, and the title gains `· scrolled`; the agent keeps running. `PgDn` or the wheel past the bottom, or `enter` to type, returns to the live terminal.
- **Scrolled while typing** (snapshot only): Typing stays, and the title reads `· TYPING · scrolled`. `PgUp` / `PgDn` page the snapshot, `ctrl+q` leaves Typing and stays scrolled, the wheel or `PgDn` past the bottom returns to live (Typing stays), and any other key returns to live and goes to the agent.
- **A stopped agent, or one not shown live yet:** the wheel does nothing. A horizontal wheel does nothing anywhere.

**Right-click and drag in Typing mode** are ignored (P4-Q4's default); the wheel and clicks are as below.

**Clicks:** a click on an agent's row (org pane or Needs you), or inside the agent pane while it shows an agent, unless the agent is stopped or errored, selects and focuses it and starts Typing. It takes effect when the button is released, and only if the press did not become a drag (moving a node) or a text selection, so those still work. On a stopped or errored agent's row a click only selects it and the org pane keeps focus, as before (inside the agent pane it focuses the pane in Selected mode); a click that starts Typing keeps a scrolled-back snapshot (`TYPING · scrolled`), where `enter` returns to live; a finished agent takes input, so a click on it starts Typing (and marks it seen). If the agent's live terminal isn't showing yet, Typing starts when it appears (about half a second at most). Until then the typed keys that would otherwise be DuoHive's (`space`, `z`, `r` and the like, `q` and `ctrl+c` included) are ignored rather than buffered, so a fast typist cannot give work or zoom by accident, and the footer says `opening <agent>…`. `esc`, `tab`, `shift+tab`, the arrows (`←` `→` too), any new mouse press, a dialog or a menu cancel the request instead, and it is dropped at once when nothing can open (the stream is blocked, or DuoHive is reconnecting). The agent pane's own `enter` does not wait: with the terminal not ready it says `<agent> isn't ready yet`, and a second `enter` types; zoom cannot be pressed in that window (`z` is ignored), but a zoom by other means cancels it. While typing, a click inside the pane keeps Typing, and a click elsewhere leaves Typing (as `ctrl+q`), then acts, so a click on another agent's row moves Typing to that agent. `tab`, `shift+tab` and the arrow keys never start Typing, and a key pressed while the button is held cancels the click. The Worktrees list's `enter` (open in agent) goes to the agent in Selected mode. Clicks are DuoHive's own: the click is not sent to the agent. Clicks do nothing while zoomed, and the mouse, wheel included, does nothing on the too-small screen. While a dialog, a confirmation, Settings or Worktrees is open, the wheel does nothing; clicks there do what their key does (§5). Right-click and drag come with §4.4.2.

```
┏━ auth-service · Claude · worktree retry-loop · TYPING ━━━━━━━━━━━━━━━┓
┃ ✓ go test ./... (14 passed)                                          ┃
┃ ✓ Ready for review · 3 files changed                                 ┃
┃                                                                      ┃
┃ ❯ also add a test for the third retry_                               ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
 TYPING · every key goes to auth-service, ctrl+c too · ctrl+q stop typing
```

**Footer in Selected mode** fits what the pane shows:

- a running agent: `enter type · z zoom · PgUp/PgDn scroll · space give work`;
- a stopped agent: `z zoom · space give work`;
- a team or role: `tab pane · esc back`.

**What the pane shows** depends on the selection:

- **A running agent:** its live terminal (§4.1).
- **Duo:** Duo's own live terminal. When Duo's CLI asks a question, Duo is waiting and joins Needs you; you answer here.
- **A pending approval card:** nothing is drawn over the pane; the card waits in Needs you until you open it (§4.6).

```
┌─ ⬢ Duo · OpenCode · your manager agent ──────────────────────────────────────────────────────────┐
│ opencode · ready                                                                                 │
│ ❯                                                                                                │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
┌─ ⬢ Duo · ● working ──────────────────────────────────────────────────────────────────────────────┐
│ 10:15 Duo   squad-1 has been quiet 12m. Proposed: stop it.                                       │
│             → approved: stop squad-1                                 10:16                       │
│ 10:21 Duo   PAY-88 splits into 3 subtasks. No Frontend agent in Payments yet.                    │
│             → give work pay-qa "PAY-88c QA: retry cases"                started                  │
│ ❯ _                                                                                              │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

- **A Claude agent waiting on its folder-trust dialog** (a new worktree path, P2-D23): its live terminal, with the pane's bottom edge saying how to answer. `No, exit` is highlighted, so `enter` alone ends the agent. The hint clears when the agent's state changes; narrow panes get `To trust: ↓ then enter · enter alone exits`.

```
┌─ billing-api · Claude · worktree retry-loop ──────────────────────────────────────┐
│  Quick safety check: Is this a project you created or one you trust?              │
│                                                                                   │
│  ❯ No, exit                                                                       │
│    Yes, I trust this folder                                                       │
│                                                                                   │
│  Enter to confirm · Esc to cancel                                                 │
└─ Claude asks to trust this folder · choose Yes (↓ then enter) · enter alone exits ┘
```

- **A team or role:** a summary of its agents, defaults and prompt.

```
┌─ ◇ Backend · in Payments ────────────────────────────────┐
│ 2 agents · ● 1 working · ◆ 1 waiting                     │
│ Defaults  CLI Claude Code · model inherit: sonnet        │
│           worktrees inherit: off                         │
│ Prompt    API URLs use hyphens. Run make test bef…       │
│                                                          │
│ space give work to this role · e edit                    │
└──────────────────────────────────────────────────────────┘
```

When `Defaults` doesn't fit on one line, `model` goes on its own line under `CLI`, with the same indent as `worktrees`. A value that still doesn't fit is cut with `…`.

- **A stopped agent:**

```
┌─ pay-qa · Claude · ~/code/pay-qa ────────────────────────┐
│                                                          │
│   ○ stopped · space to give work                         │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

- **An empty org:**

```
┌──────────────────────────────────────────────────────────┐
│                                                          │
│   Your org is empty · n to add a team, role or agent,    │
│   or ask Duo to set one up                               │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

- **At start-up,** while DuoHive reattaches to running agents:

```
┌──────────────────────────────────────────────────────────┐
│                                                          │
│   Reconnecting to 6 agents…                              │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

#### 4.1.3 Needs you

- **Holds:** every waiting, quiet and errored agent, and every pending approval card (`◆ Duo  approval  1m  Add pay-web`, §4.6).
- **Order:** oldest first, by time since entering the state.
- **Keys:** it is a `tab` stop; `↑↓` move.
  - `enter` or a click on an agent row selects the agent and starts Typing in the agent pane (Selected mode if the agent is stopped or errored). A click elsewhere in the pane focuses it.
  - On a card row, `enter` or a click opens that card's dialog (§4.6). The selection does not change.

#### 4.1.4 Desktop alerts

- **When:** an agent becomes waiting, quiet or errored; an approval card is raised; an agent other than Duo finishes. Each kind can be turned off in Settings.
- **Content:** the title is "<agent> needs you" (or "<agent> finished"); the body is the last reply. A card's alert follows the `needs you` toggle; its title is "Duo needs you" and its body the card's summary.
- **Click:** selects the agent, where the system's notifier reports clicks (Linux `notify-send --action`). It doesn't raise the terminal window: neither notifier can name the window DuoHive runs in.
- **Once:** one alert per agent per state change.
- **Unavailable:** when the system has no notifier, the footer says "desktop alerts unavailable" once per session.

### 4.2 Zoomed agent

`z` on an agent: its terminal fills the screen, in the same place it runs in the main screen, and the footer still shows the Needs you count. `z` or `esc` restores. `z` zooms an agent whatever its state; a stopped agent's zoomed pane shows its stopped line, and the footer reads `z restore · space give work`. On a team or role, `z` does nothing and the footer says "z zooms an agent".

```
 DuoHive › auth-service (zoomed)                                                         z restore 
┏━ auth-service · Claude · worktree retry-loop ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ (full-screen live terminal of the agent)                                                         ┃
┃                                                                                                  ┃
┃ ✓ go test ./... (14 passed)                                                                      ┃
┃ ✓ Ready for review · 3 files changed                                                             ┃
┃                                                                                                  ┃
┃ ❯                                                                                                ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
 enter type · z restore · PgUp/PgDn scroll · ◆ 2 need you (squad-3, billing-api)
```

### 4.3 First-run setup

Three steps, shown once, when no org exists. Every field has a sensible default; `enter` on the last field moves to Next. `esc` on step 1 asks whether to quit; on later steps it goes back.

**Step 1: you and Duo.** DuoHive lists the supported CLIs it found (Claude Code and OpenCode in v1) and their versions. A CLI that isn't installed has its radio disabled, `(-)`.

```
┌─ Welcome to DuoHive · step 1 of 3 ───────────────────────────────────┐
│                                                                      │
│  DuoHive runs your AI coding agents as a team.                       │
│  Duo, your manager agent, helps you steer it.                        │
│                                                                      │
│▌ Your name     [Duong                       ]                        │
│  Duo runs on   (•) Claude Code   (-) OpenCode                        │
│  found: claude 2.1.0 · opencode not found                            │
│                                                                      │
│  enter next · esc quit (asks first)       [ Next → ]   [ Quit ]      │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

**No CLI at all:** a blocking screen, like the tmux one (§4.12).

**Blank** has no form on step 3: the step shows `You and Duo only. Add teams, roles and agents later with n.`, and its first control is `Create org` (there are no fields, so `enter` creates the org rather than going back).

**What Create org writes:** `[org] cli` is Duo's CLI from step 1 and no other org default; `[duo]` is `Duo` on the same CLI. The template's CLI, directory and worktree choices go on the teams or the Generalist role it creates (TD §7). The app starts its agents only after Create org.

**Replace mode** (Settings → Org → `New org from template…`): the same steps 2 and 3, titled `step 1 of 2` and `step 2 of 2`, with no step 1 (you and Duo are kept). `[ Quit ]` reads `[ Cancel ]`; `[ ← Back ]` is hidden on the template step and returns from the form to the template step; `esc` does what Back does, and from the template step cancels back to Settings. `Create org` opens `Replace your org?` (§4.8) instead of writing at once. If an agent can't be stopped, nothing is written and a banner says `<agent> would not stop · org not replaced`.

**Step 3 errors** (shown under the field, `Create org` disabled until fixed): Agent Squad's directory missing, or not a git repo while worktrees are on (the radio is disabled with the reason, as in §4.4.1); the same for each of Product Team's three directories; a squad size outside 1–100; a duplicate team name or agent name; Engineering Org with no non-empty row (`add at least one team`); a directory that doesn't exist, on that row. A failed write shows `couldn't write org.toml · <reason>` and keeps the dialog open.

```
┌─ DuoHive needs an agent CLI ─────────────────────────────────────────┐
│                                                                      │
│  Neither Claude Code nor OpenCode was found on your PATH.            │
│                                                                      │
│    Claude Code  claude.ai/code                                       │
│    OpenCode     opencode.ai                                          │
│                                                                      │
│  Install at least one, then run duohive again.                       │
│                                                  [ q Quit ]          │
└──────────────────────────────────────────────────────────────────────┘
```

**Step 2: choose a template.** The preview updates as you move.

```
┌─ Choose a starting org · step 2 of 3 ────────────────────────────────┐
│                                                                      │
│▌Agent Squad      a Generalist role; any free agent takes a task      │
│ Product Team     Backend · Frontend · QA, one agent each             │
│ Engineering Org  several teams, each Backend + QA                    │
│ Blank            just you and Duo; build it yourself                 │
│                                                                      │
│  ┄ Preview ┄                                                         │
│  ⬢ Duo                                                               │
│  └─ ◇ Generalist                                                     │
│     ├─ squad-1                                                       │
│     ├─ squad-2                                                       │
│     └─ squad-3                                                       │
│  ↑↓ choose · enter · esc     [ ← Back ]   [ Next → ]   [ Quit ]      │
└──────────────────────────────────────────────────────────────────────┘
```

**Step 3: fill in the template.** It asks only what the template needs.

Agent Squad (worktrees default to on; 1–100 agents):

```
┌─ Set up Agent Squad · step 3 of 3 ───────────────────────────────────┐
│                                                                      │
│▌ Agents        [3                           ]                        │
│  Directory     [~/code/app                  ] ✓ git repo             │
│  CLI           (•) Claude Code   (-) OpenCode                        │
│  Worktrees     [x] own worktree per task                             │
│  Role prompt   [You are a careful engineer. ]                        │
│  You can change all of this later.                                   │
│  tab · enter · esc       [ ← Back ]   [ Create org ]   [ Quit ]      │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

A focused field too long for its box scrolls to keep the cursor in view; an unfocused one shows the text's start. A squad size outside 1–100 shows `✕ agents must be 1–100` after the field.

Product Team:

```
┌─ Set up Product Team · step 3 of 3 ──────────────────────────────────┐
│                                                                      │
│▌ Team name     [Product                     ]                        │
│  CLI           (•) Claude Code   (-) OpenCode                        │
│  Backend dir   [~/code/app                  ] ✓ git repo             │
│  Frontend dir  [                            ] ✕ required             │
│  QA dir        [                            ] ✕ required             │
│  Worktrees     [ ] own worktree per task                             │
│                                                                      │
│  Each role gets one agent and a starter prompt you can edit.         │
│  tab · enter · esc       [ ← Back ]   [ Create org ]   [ Quit ]      │
└──────────────────────────────────────────────────────────────────────┘
```

Engineering Org (one "team name + repo" row per team):

```
┌─ Set up Engineering Org · step 3 of 3 ───────────────────────────────┐
│                                                                      │
│▌ CLI           (•) Claude Code   (-) OpenCode                        │
│  name                 repo                                           │
│  [Payments          ] [~/code/app              ] ✓ git repo          │
│  [Web               ] [~/code/plain            ] ✓ found             │
│    [ + Add team ]  empty rows are ignored                            │
│  Worktrees     [ ] own worktree per task                             │
│                                                                      │
│  Each team gets Backend and QA roles with one agent each.            │
│  tab · enter · esc       [ ← Back ]   [ Create org ]   [ Quit ]      │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

Starting with no teams, the form shows one row; `+ Add team` appends another, and a row with both fields empty is ignored (so is its error).

### 4.4 Org editing

#### 4.4.1 Forms

Opened with `n` (new) or `e` (edit), or from the right-click menu.

- **Inherited values:** shown as `inherit: <value>` in `fg.muted`; typing overrides them, and clearing a field restores inheritance.
- **Stacked prompts:** the prompts this agent stacks under are shown read-only above its own prompt, one line per level; empty levels show "(none)".
- **Moving:** `tab` / `shift+tab` and `↑` / `↓` move to the next and previous field, then `Save`, then `Cancel` (read-only fields are skipped). On the `Save` / `Cancel` row `←` and `→` move between the two. In a text field `←` / `→` move the cursor and in a choice they change it, so they never leave the field; inside an open popup `↑↓` choose in it. **In the Own prompt** (a multi-line box) `↑` / `↓` move the cursor to the line above or below, wrapped lines counting as lines, and keep the column where they can; `↑` on the first line goes to the previous field and `↓` on the last line to the next, wherever the cursor is in that line. A one-line prompt therefore behaves as before, and `tab` / `shift+tab` always leave. **The box grows with its text:** one row while the text fits on one line (so every form looks as it does now), then one more row per line up to 4, wrapping long text at the box's width and showing line breaks as real breaks (no `↵`); past 4 rows it scrolls to keep the cursor in view. Unfocused, it shows the same rows. The form is taller by the extra rows; at the smallest screen (80×24) a form with a 4-row prompt still shows its Save and Cancel row (the form scrolls as it does today when it is too tall).

```
  Prompt  (stacks under: Org › Payments › Backend)
  ┊ Org       Ask before you delete anything.
  [ Own: Owns the billing service; invoices and retries. ]
  [      Run make test before you say you are done.      ]
``` The same keys move between the buttons of a confirmation and of a card (§4.6).
- **Validation:** checked as you type, with an error line under the field. `Save` stays disabled until the form is valid.
- **Flags:** error under Flags: `--dangerously-skip-permissions is never allowed` (or the `--allow-…` twin); the form can't be saved (P3-D31).
- **Renaming:** only while the agent is stopped and has no worktrees; otherwise the name field is read-only with the reason under it.
- **Running agents:** edits are saved now and apply on the agent's next start.

```
┌─ Edit agent · Payments › Backend › billing-api ──────────────────────────────┐
│                                                                              │
│  Name        [ billing-api_________ ]                                        │
│              rename only while stopped with no worktrees                     │
│  CLI         [ inherit: Claude Code ▾ ]                                      │
│  Directory   [ ~/code/billing______________ ]  ✓ git repo                    │
│  Model       [ inherit: sonnet ▾ ]          Effort  [ inherit: high ▾ ]      │
│  Flags       [ ______________________________ ]                              │
│  Worktrees   [ inherit: off ▾ ]                                              │
│                                                                              │
│  Prompt  (stacks under: Org › Payments › Backend)                            │
│  ┊ Org       Ask before you delete anything.                                 │
│  ┊ Payments  (none)                                                          │
│  ┊ Backend   API URLs use hyphens. Run make test before done.                │
│  [ Own: Owns the billing service; invoices and retries.        ]             │
│                                                                              │
│  billing-api is running: changes apply on next start                         │
│  tab next field · ctrl+s save · esc cancel       [ Save ]  [ Cancel ]        │
└──────────────────────────────────────────────────────────────────────────────┘
```

**Model and effort dropdown (phase 4).** The closed field is `[ inherit: sonnet ▾ ]`; `enter` or a click opens a popup under it.

```
  Model    [ son___________________ ▴ ]
           ┌────────────────────────────────────┐
           │▌inherit: sonnet                    │
           │ sonnet                             │
           │ use "son" as typed                 │
           └────────────────────────────────────┘
           type to filter · ↑↓ · enter choose · esc close
```

- **List and filter:** typing filters the list by substring; `↑↓` move, `enter` chooses, `esc` closes without a change. The list shows at most 6 rows and scrolls (a `… N more` line). The first row is `inherit: <value>`; the last is `use "<typed>" as typed`, which sets a model that isn't listed (Claude Code's other models are typed, TD §5.2).
- **Which list:** the CLI the form resolves to, including an inherited one (a role under a team on OpenCode lists OpenCode's models).
- **Not installed:** a CLI that isn't installed reports `✕ couldn't list <cli>'s models`, so its fields stop saying `loading models…`.
- **Loading, empty and failed:** while loading the field reads `loading models…` and typing still works; an empty or failed list shows only the `inherit` and `use "<typed>" as typed` rows, and a failure adds the `✕ couldn't list <cli>'s models · type a name or keep inherit` line.
- **Effort:** the same popup, listing the chosen model's variants (OpenCode, when the CLI lists them; OpenCode 2.1.1 doesn't, so its Effort takes typed text) or the `--effort` choices (Claude Code). On OpenCode with a model of `inherit`, the list is the inherited model's variants when that model is known; when none is (no model set anywhere up the tree), effort is disabled with `OpenCode sets effort per model · choose a model`. Changing the model clears an effort the new model doesn't offer.
- **Role form agent naming:** the New role form's `Agents` field names agents `<team>-<role>-N` (above); first-run templates use their own names (P4-Q6).

**Directory suggestions.** Every Directory field (agent and role forms, first-run step 3 including Product Team's three directories and Engineering Org's repo column) opens a popup under it while you type a path that starts with `~` or `/`; a relative path gets none.

```
  Directory   [ ~/code/bi___________ ]  ✗ missing
              ┌──────────────────────────────┐
              │▌~/code/billing-api   git     │
              │ ~/code/billing-docs          │
              └──────────────────────────────┘
              ↑↓ · tab or enter take · esc close
```

- **List:** the folders inside the typed path's parent whose names start with the typed last segment, ignoring case, sorted by name; folders only, `git` in `fg.muted` after a git repo's name. At most 6 rows, then `… N more`. Hidden folders (`.name`) are listed only once the segment starts with `.`. No match, an unreadable folder or a missing parent: no popup and no error (the `✓` / `✕` note after the field is unchanged).
- **Keys:** the first row is highlighted. `↑↓` move the highlight, `tab` or `enter` take it: the path becomes `<folder>/` and the popup lists that folder's subfolders, so you can go down a level at a time. `esc` closes the popup and keeps the text (the form's `esc` cancels only when no popup is open). With the popup closed, `tab` and `enter` move to the next field as in any text field, and `↑↓` as well. A click on a row takes it.
- **Opens** only when the text changes (a typed key, a paste, backspace), never on focus alone; it closes when the field loses focus.

**Form errors:** a duplicate name; worktrees on a directory that isn't a git repo (that radio is disabled, with the reason); the model list failing (while it loads, the field shows "loading models…").

```
┌─ New agent · Payments › Backend ─────────────────────────────────────────────┐
│                                                                              │
│  Name        [ auth-service________ ]                                        │
│              ✕ an agent named auth-service already exists                    │
│  CLI         [ inherit: Claude Code ▾ ]                                      │
│  Directory   [ ~/notes_____________________ ]  ✓ found                       │
│  Worktrees   (•) agent's directory   (-) new worktree                        │
│              new worktree needs a git repo; ~/notes isn't one                │
│  Model       [ inherit: sonnet ▾ ]                                           │
│              ✕ couldn't list claude's models · type a name or keep inherit   │
│                                                                              │
│  fix 1 error to save                             [ Save ]  [ Cancel ]        │
└──────────────────────────────────────────────────────────────────────────────┘
```

```
┌─ New role · in Payments ─────────────────────────────────────────────┐
│                                                                      │
│  Name         [ Frontend____________ ]                               │
│  Defaults for its agents (empty = inherit from Payments)             │
│    CLI        [ inherit: Claude Code ▾ ]                             │
│    Directory  [ ~/code/web_______________ ]  ✓ git repo              │
│    Worktrees  [ inherit: off ▾ ]                                     │
│  Role prompt  [ You build React UI. Follow the design system.    ]   │
│  Agents       [ 1 ] create this many now (payments-frontend-1 …)     │
│                                                                      │
│                                          [ Create ]  [ Cancel ]      │
└──────────────────────────────────────────────────────────────────────┘
```

**Edit org:** `e` on the Duo row. It edits `org.toml`'s org-level defaults and Duo's name and CLI. Changing Duo's CLI restarts Duo after a confirmation.

```
┌─ Edit org · saved to org.toml ───────────────────────────────────────────────┐
│                                                                              │
│  Duo                                                                         │
│    Name        [ Duo_________________ ]                                      │
│    Runs on     [ OpenCode ▾ ]   changing it restarts Duo (asks first)        │
│  Defaults for every agent                                                    │
│    CLI         [ Claude Code ▾ ]   Model [ sonnet ▾ ]   Effort [ high ▾ ]    │
│    Worktrees   [ off ▾ ]                                                     │
│  Org prompt    [ Ask before you delete anything.                ]            │
│                                                                              │
│  tab next field · ctrl+s save · esc cancel       [ Save ]  [ Cancel ]        │
└──────────────────────────────────────────────────────────────────────────────┘
```

#### 4.4.2 New, move, delete

- **`n`** opens a chooser of what can be created at the selection: on Duo, a team or a role; on a team, a role in it or a team next to it; on a role or an agent, an agent in that role.

```
┌─ New · Payments selected ────────────┐
│▌ Role in Payments                    │
│  Team next to Payments               │
│  Agent: select a role first          │
│                                      │
│ ↑↓ choose · enter open form · esc    │
└──────────────────────────────────────┘
```

- **Right-click** opens the node's menu (placed at the pointer, moved in to stay on screen; `esc`, a click elsewhere or an action closes it), listing every action with its key. Disabled actions show the reason (for example "Delete · stop it first"). Duo's menu has Edit org; team and role menus have Give work (roles only), New here, Edit, Move to and Delete.

```
┌─ pay-qa ───────────────┐
│ Give work      space   │
│ Start          s       │
│ Edit           e       │
│ New here…      n       │
│ Move to…       m       │
│ Delete         d       │
│ Zoom           z       │
│ Worktrees      w       │
│ Hide replies   v       │
└────────────────────────┘
```

- **`m`** opens "Move to…", listing valid targets. When the move changes inherited values, the picker lists them before you confirm.

```
┌─ Move pay-qa to… ────────────────────────────────────────┐
│                                                          │
│  Payments › Backend                                      │
│  Payments › QA          current                          │
│▌ Generalist                                              │
│                                                          │
│  Moving changes what pay-qa inherits:                    │
│    worktrees   off → on                                  │
│    role prompt QA → Generalist                           │
│                                                          │
│  ↑↓ choose · enter move · esc cancel                     │
└──────────────────────────────────────────────────────────┘
```

- **Nothing to move to:** when there is no valid target, `m` and the menu row are disabled with `no other place to move <name>`.
- **Collisions:** agent and team names are unique across the org and sibling roles are unique within a team, so a move can't repeat them; a role moved under a team that already has a role of that name is refused with `<team> already has a role <role>`.
- **On a team,** Move to… lists its positions among the teams (`before <team>` for each other team, and `at the end`); teams don't nest.
- **Drag and drop:** valid targets are highlighted while dragging. An invalid drop (an agent onto a team, a team onto a role) is refused with the reason in the footer. A drop that changes inherited values opens the same list as "Move to…" for confirmation.
- **`d`** deletes. It is disabled on any agent that isn't stopped, with the reason in the footer. Deleting a role or team lists every agent it removes, and is disabled while any of them is running (§4.8).

### 4.5 Give work

`space` on an agent or a role. The task box takes multiple lines (`alt+enter`) and wraps at word boundaries; a word wider than the box breaks at its edge. `enter` is disabled while the task is empty. `space` on a team or on Duo does nothing, and the footer says "space gives work to an agent or a role".

What happens depends on the agent's state:

| State | Give work |
|---|---|
| stopped, errored | Starts a fresh session with the task. "Run" picks the agent's directory or a new worktree, pre-set from the agent's default |
| finished, idle | The task is typed in as the next turn of the existing session, which keeps its context. It runs where the session already runs |
| working, waiting | Refused. The dialog offers "Send as message instead"; the message is typed in when the agent is at rest |

**Delivery is confirmed.** A task typed into a finished or idle agent counts as taken only once the agent leaves rest (a busy hook, or a busy screen read). Until then, at most 5 s:

- a further Give work to that agent is refused with "<agent> is still taking the last task";
- if no sign comes, the footer says "<agent> didn't take the task" and Give work opens again with the task filled in (`enter to try again`). When a dialog or typing is already open, the footer says "· space to try again" instead, and the task waits.

**A stopped agent:**

```
┌─ Give work · pay-qa · ○ stopped ───────────────────────────────────────────────────────┐
│ Task  [ Add retry cases for PAY-88. AC: 3 tries, jitter, no double charge. ]           │
│ Run   ( ) new worktree   (•) ~/code/pay-qa              agent default: directory       │
│ Starts a fresh session.                                                                │
│                                             enter give · alt+enter new line · esc      │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

**A finished or idle agent:**

```
┌─ Give work · squad-2 · ✓ finished ─────────────────────────────────────────────────────┐
│ Task  [ Now make the date test run in UTC as well._______________________ ]            │
│ Run   in worktree flaky-date, as the next turn of squad-2's session                    │
│                                             enter give · alt+enter new line · esc      │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

**A busy agent:**

```
┌─ Give work · auth-service · ● working ─────────────────────────────────────────────────┐
│ Task  [ Also cover the timeout path.______________________________________ ]           │
│ auth-service is working. Give work when it is at rest, or send this as a               │
│ message: it is typed in when auth-service is at rest.                                  │
│                   [ Give work ] (disabled)   ▌[ Send as message instead ]              │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

**A role:** a free agent (stopped, finished or idle, and not still taking a task or getting a worktree ready) takes the task; the dialog shows which one, and `↑↓` in "Goes to" picks another.

```
┌─ Give work · role Generalist (1 of 3 free) ────────────────────────────────────────────┐
│ Task     [ Fix the flaky date test in billing/time_test.go__________________ ]         │
│ Goes to  ▌squad-2 (finished 3m)                        ↑↓ pick another                 │
│ Run      in ~/code/my-app, as the next turn of squad-2's session                       │
│                                             enter give · alt+enter new line · esc      │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

The Run row follows the picked agent: radios, preset from its default, for a stopped or errored pick; the next-turn line for a finished or idle one (P3-D29).

**A role with no free agent:** `enter` proposes a new agent. DuoHive raises an approval card from Duo that holds the task, even while Duo is stopped or busy; approving it creates the agent and gives it the task.

```
┌─ Give work · role Payments › Backend (0 of 2 free) ────────────────────────────────────┐
│ Task     [ Add an idempotency key to POST /payments.________________________ ]         │
│ Goes to  No free agent · enter proposes one                                            │
│          (the approval card holds the task)                                            │
│                                             enter propose one · esc                    │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### 4.6 Duo's approval cards and actions

When Duo proposes an org change, or wants to stop a starting, working or waiting agent, a card is raised. It is answered in a **dialog**: the same centred box as a form (§4.4.1) with the same border, `border.focus` outline, button row and focus marker, so a card is answered the way a form is filled. Its title starts with an amber `◆` (`status.attention`; P3-D4, P2-D27) and reads `◆ Stop billing-api · 1 of 3` (`· 1 of N` only when N is more than 1).

- **It never opens by itself.** A modal would take the keys you are typing to an agent. A raised card shows as a `◆` row in Needs you, in the header count and in the alert, and waits there. The dialog opens from `enter` or a click on the card's row in Needs you; the selection and the agent pane do not change.
- **The queue:** the dialog shows the card you opened; `1 of N` counts all pending cards, oldest first. Answering one shows the next oldest in the same dialog, so a queue is cleared without going back to Needs you. `esc` closes the dialog and every pending card stays pending.
- **Body:** the card's title line and its change rows (below), then the button row `[ Approve ]  [ Edit ]  [ Reject ]` with the keys beside each as `y`, `e`, `n`. The focused button has the form's focus marker. Focus starts on `Reject` so a second `enter` can't approve what you haven't read (a call of the plan, not the author's).
- **Keys:** `←` `→` and `↑` `↓` move between the buttons (the same as in a form's Save and Cancel row, §4.4.1); `enter` presses the focused one; `y`, `e` and `n` press theirs at once; `esc` closes the dialog. The keys are those of a dialog (§4.11); they are not rebindable here except `y` / `e` / `n`, which stay rebindable (card actions, §4.11).
  - **Approve** (`y`) applies the change and shows the next card, or closes the dialog when it was the last.
  - **Edit** (`e`) opens the form pre-filled. Save there approves the edited change and returns to the next card or the screen; `esc` in that form returns to the card, unanswered. On a stop card `e` is disabled and its button is dimmed; it is skipped by the arrow keys. On a new-agent card it opens the agent form; the task the card holds is kept unchanged, and the form shows it under the fields (`Task  <first line>`).
  - **Reject** (`n`) tells Duo it was declined.
- **Size:** the form's: centred, at most 66 cells wide, as tall as its rows, clipped to the screen with the rows cut as a form's are (first line, then the `flags` rows, then the rest, with `… <n> more rows`; the title and the button row always stay). A screen too small for the buttons draws the dialog anyway, since it is modal: `esc` always closes it.
- **Mouse:** a click on a button presses it; a click outside the box does nothing (as for a form).
- **While you are Typing:** a dialog, once opened, takes every key as a form does; Typing resumes when it closes.
- **What a change sets:** under its first line, an org-change card lists every field the change sets or changes, one row each, so nothing is approved unseen. An add shows `+ <field>: <value>` for each field it sets (`every field inherited` when none). An edit shows `~ <field>: <old> → <new>` for each field that differs, including a rename (`~ name: QA → Testing`); `inherit` stands for unset (`unset` for org defaults), unchanged fields are left out, and `no changes` stands alone. Editing the org defaults also lists Duo's name and CLI. A move shows `from <place> → <place>`; a remove shows what goes with it (`with 2 roles, 3 agents`). A prompt shows its first line and, when it has more, `(<n> lines)`. A `flags` row is drawn in `status.attention` and shown in full, wrapping inside the card; other long values are cut with `…`.
- **Several cards:** each is its own Needs you row, oldest first. A row's `enter` or click opens the dialog on that card, which may be a later one than the oldest; answering it shows the oldest pending card next.
- **Answered cards** become one `→` line in the chat with the result ("→ approved: stop squad-1"). The line shows the answer's time at its right (`→ approved: stop squad-1   10:16`) and sits where the answer came in. The chat holds no pending card.
- **Duo isn't blocked:** Duo keeps working while a card waits; your answer reaches Duo later as a message from you. Each card is its own Needs you row.
- **Action lines:** each of Duo's tool calls shows as one `→` line with its result: `typed in`, `queued (… is working)`, `started`, `stopped`, `refused (…)` or `card raised`. Delivery stays visible through `message_status` and the `✉` count. Each action line is `→ <text>   <result>`, with three spaces before the result, in `fg.muted`. If the line wraps, it hangs under the text.

The mock shows the dialog, with focus on `Reject`, and Duo's chat, which holds only the results.

```
          ┌─ ◆ Proposed change · 1 of 2 ───────────────────────────────────┐
          │                                                                │
          │  + Add agent pay-web in Payments › Frontend (new role)         │
          │    + cli: Claude Code                                          │
          │    + directory: ~/code/web                                     │
          │    + worktree: on                                              │
          │                                                                │
          │            [ y Approve ]    [ e Edit ]   ▌[ n Reject ]         │
          └────────────────────────────────────────────────────────────────┘
┌─ ⬢ Duo · ● working ──────────────────────────────────────────────────────────────────────────────┐
│ 10:15 Duo   squad-1 has been quiet 12m. Proposed: stop it.                                       │
│             → approved: stop squad-1                                 10:16                       │
│ 10:21 Duo   PAY-88 splits into 3 subtasks. No Frontend agent in Payments yet.                    │
│             → give work pay-qa "PAY-88c QA: retry cases"                started                  │
│ ❯ _                                                                                              │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

The card's row in Needs you:

```
┌─ Org ──────────────────────────────────────────┐
│ …                                              │
├─ Needs you (3) ────────────────────────────────┤
│ ◷ squad-3      quiet      6m                   │
│ ◆ billing-api  waiting    4m  "Allow edit to…" │
│ ◆ Duo          approval   1m  Add pay-web      │
└────────────────────────────────────────────────┘
```

### 4.7 Chat

- **Input:** always takes typed text. `enter` sends; `alt+enter` adds a line; `PgUp` / `PgDn` scroll the chat history.
- **Plain text** goes to Duo. While Duo is busy it waits, shown as queued, and is typed in when Duo is at rest.
- **`@agent text`** goes straight to that agent:
  - to an agent at rest, it is typed in at once;
  - to a working or waiting agent, it waits: the chat shows `→ … queued (pay-qa is working)` and the agent's row shows `✉ 1`;
  - to a stopped agent, it starts the agent with the message as its task.
- **Messages between agents** are not shown in the chat, only as `✉` on the recipient's row.

**Suggestions:** typing `@` or `/` opens a list above the input. It filters as you type; `tab` or `enter` takes an item, and `esc` closes the list. Agents show their state, so you can see who is free. Items match by prefix, ignoring case (`@a` lists auth-service). `↑`/`↓` move the highlight. Taking an item adds a space after it. `enter` on a command typed in full runs it. `/help` prints the chat's keys and `/agents` lists every agent with its state cell; both are local lines, never stored. `/x` gives `unknown command /x · /help lists them`, and `@nobody` gives `no agent named nobody`; in both cases the text stays in the input.

```
┌─ ⬢ Duo · ◦ idle ─────────────────────────────────────────────────────────────────────────────────┐
│   ┌──────────────────────────────────────────┐                                                   │
│   │▌@auth-service      ● working    2m       │                                                   │
│   │ @billing-api       ◆ waiting    4m       │                                                   │
│   │ @pay-qa            ○ stopped    1d       │                                                   │
│   └──────────────────────────────────────────┘                                                   │
│ ❯ @_                                                                                             │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

```
┌─ ⬢ Duo · ◦ idle ─────────────────────────────────────────────────────────────────────────────────┐
│   ┌──────────────────────────────────────────┐                                                   │
│   │▌/help     commands and keys              │                                                   │
│   │ /agents   every agent and its state      │                                                   │
│   └──────────────────────────────────────────┘                                                   │
│ ❯ /_                                                                                             │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 4.8 Confirmations

Asked only for actions that stop work, remove something or replace the org. Every destructive dialog opens with focus on Cancel; `enter` acts on the focused button, `y` confirms, and `n` or `esc` cancels.

```
┌─ Stop agent? ────────────────────────────────────────────────┐
│                                                              │
│  Stop auth-service? It is working (2m) in a worktree.        │
│  The worktree and its branch are kept.                       │
│                                                              │
│                              [ y Stop ]   ▌[ n Cancel ]      │
└──────────────────────────────────────────────────────────────┘
```

```
┌─ Delete role? ───────────────────────────────────────────────┐
│                                                              │
│  Delete role QA in Payments? It removes 1 agent:             │
│    pay-qa (stopped)                                          │
│  Worktrees are kept and stay listed under w.                 │
│                                                              │
│                            [ y Delete ]   ▌[ n Cancel ]      │
└──────────────────────────────────────────────────────────────┘
```

```
┌─ Replace your org? ──────────────────────────────────────────┐
│                                                              │
│  A new org from Product Team replaces your teams, roles and  │
│  agents (6 agents). Duo and the org defaults are kept.       │
│  Running agents in it are stopped first.                     │
│  Your current org.toml is kept as a backup.                  │
│                                                              │
│                           [ y Replace ]   ▌[ n Cancel ]      │
└──────────────────────────────────────────────────────────────┘
```

### 4.9 Worktrees

`w` opens the list of DuoHive's worktrees. States are words in `fg.muted`, never status glyphs or amber.

- **Automatic removal:** a worktree is removed once its branch is merged (normal, squash or rebase) and it has no uncommitted changes.
- **Checked:** when its agent finishes, after start-up, when this list opens, and every 10 minutes, against `origin`'s default branch (or the local default branch when there is no remote).
- **By hand:** `d` removes a worktree; removing anything not merged and clean asks first. A worktree in use can't be removed: `d` says "<agent> is running in it · stop it first".
- **In use:** its agent is running and its current session was launched there. Once the agent stops, the next check decides.
- **Names:** agent names and branches are cut in the middle with `…` (§2.4).
- **Empty:** `No worktrees yet. An agent gets one when you give it work in a new worktree.`, cut with `…` when narrow, and `esc back`; no columns, check line or row keys.
- **States:** `not checked yet` until the first check; `in use`; `not merged · N files changed`; `merged · uncommitted changes, kept`; `merged · removing`; `couldn't check · <reason>` when git can't tell (never treated as merged).

```
┌─ Worktrees · 4 · removed automatically once merged and clean ────────────────────────────────────┐
│   Agent          Branch                             State                                        │
│▌  auth-service   duohive/auth-service/retry-loop    in use                                       │
│   pay-qa         duohive/pay-qa/retry-cases         merged · removing                            │
│   squad-2        duohive/squad-2/flaky-date         not merged · 2 files changed                 │
│   squad-1        duohive/squad-1/readme             merged · uncommitted changes, kept           │
│                                                                                                  │
│ checked 3m ago against origin/main                                                               │
│ enter open in agent · d remove (asks unless merged and clean) · esc back                         │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 4.10 Settings

`,` opens Settings. Settings are personal and saved to `settings.toml` as you change them. Org-level defaults and Duo live in the org (Edit org, §4.4.1). "New org from template…" replaces the org's teams, roles and agents after a confirmation (§4.8), keeps Duo and the org defaults, and keeps a backup.

```
┌─ Settings · saved to settings.toml ──────────────────────────────────────┐
│                                                                          │
│  You                                                                     │
│    Your name            [ Duong_____ ]                                   │
│  Attention                                                               │
│    Quiet after          [ 5 ] minutes                                    │
│    Desktop alerts       [x] needs you   [x] finished                     │
│  Look                                                                    │
│    Theme                [ auto ▾ ]                                       │
│    Last reply under agents [x]     Reduce motion [ ]                     │
│  Keys                   [ Edit key bindings → ]                          │
│  Org                    [ New org from template… ]                       │
│    org defaults and Duo: e on Duo (Edit org)                             │
│                                                                          │
│  changes save immediately · esc back                                     │
└──────────────────────────────────────────────────────────────────────────┘
```

**Key bindings** (a phase 4 screen): a list of actions and their keys.

```
┌─ Key bindings ───────────────────────────────────────────────────────┐
│                                                                      │
│   Action                 Key                                         │
│▌  Give work              space                                       │
│   Start / stop           s                                           │
│   New                    n                                           │
│   Edit                   e                                           │
│   Settings               ,                                           │
│   …                                                                  │
│                                                                      │
│  enter rebind (press the new key) · backspace default · esc back     │
└──────────────────────────────────────────────────────────────────────┘
```

- **Scrolling:** the list scrolls with `↑↓` and `PgUp/PgDn`, with `↓ more` at the foot when it doesn't fit.
- **Rebindable actions:** the single-key actions in §4.11 (Give work, Start / stop, Resume (v1.1), New, Edit, Move, Delete, Zoom, Last reply, Open org.toml, Settings, Worktrees, Filter, Key map, Approve / Edit / Reject card). Not rebindable (reserved: no action may take them, and the conflict check counts them): `tab`, `shift+tab`, `esc`, `ctrl+c`, `ctrl+q`, `q` (Quit), `ctrl+↑` / `ctrl+↓` (resize chat), arrows, `enter`, `PgUp/PgDn`, the chat's `enter` / `alt+enter` / `@` / `/`, and the keys of forms, dialogs and the text inputs.
- **One key per action:** an action that appears in two panes (`space` Give work and `z` Zoom, in the org and agent panes) has one binding, and rebinding it changes both; the conflict check covers every context the action appears in.
- **Capture:** `enter` shows `press the new key · esc cancels` on the row; the next key is taken (a modifier alone is not). `esc` cancels. A reserved key is refused with `<key> is reserved`.
- **Conflicts:** a key is unique within its context (the card dialog's keys apply only while it is open, so `e` there doesn't clash with `e` in the org pane). The contexts are Everywhere, Org pane, Needs you, Agent pane, Chat and Card dialog; Everywhere conflicts with all of them, and the others don't conflict with each other (`e` is Edit in the org pane and Edit card in the card dialog). A clash is refused with `<key> is already <action> · rebind that first`, shown on the row.
- **Storage:** `settings.toml` `[keys]` (TD §7). An unknown action id or a key that doesn't parse is ignored, and the default applies; the Keys screen shows one line above its footer, `ignored in settings.toml: <id> · using the default` (one line per entry, above the list's footer, with the list keeping room for them).

### 4.11 Key map

`?` from anywhere in Selected mode, zoomed too (the key works; the zoom footer keeps only its own keys). In Typing mode, a form and the chat input, `?` is a character and goes to the agent or the field (P2-D9); leave Typing first (`ctrl+q`). It lists the current bindings, including any you have changed. `?` does nothing while a dialog, Settings or Worktrees is open (they have their own keys; `esc` first). The header (it drops the hint in Typing mode and in forms, where `?` is a character), and the footers of the org pane (`… · ? help`, as the mocks in §4.1 show) and Needs you (`↑↓ move · enter go to agent · ? help`), carry `? help`; the agent pane's, Typing, zoom, dialog, form and chat footers do not.

```
┌─ Keys · ? to close ──────────────────────────────────────────────────────────────────────────────┐
│ Everywhere                       Org pane                         Agent pane                     │
│  tab       next pane/field        ↑↓        move                   enter     start typing        │
│  shift+tab previous pane          ←→        fold / unfold          ctrl+q    stop typing         │
│  ?         this help              enter     type to agent          z         zoom / restore      │
│  ,         settings               space     give work              space     give work           │
│  w         worktrees              s         start / stop           PgUp/Dn   scroll back         │
│  f         filter tree by state   n         new here…                                            │
│  esc       back one level         e         edit (Duo: Edit org)  Chat (Duo)                     │
│  q         quit (agents run on)   m         move to…               enter     send                │
│  ctrl+c    quit if not typing     d         delete (stopped)       alt+enter new line            │
│  ctrl+↑↓   resize chat            z         zoom agent             @ /       agents / commands   │
│                                   v         last reply on/off      PgUp/Dn   scroll history      │
│ Mouse                             o         open org.toml                                        │
│  click     select + type                                           Card dialog                   │
│  right     node menu             Needs you                                                       │
│  wheel     scroll                 ↑↓        move                                                 │
│  drag      move node              enter     go to agent or card      y e n     approve/edit/reject │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Below 100 columns it becomes one scrolling column:

```
┌─ Keys · ? to close ──────────────────────┐
│ Everywhere                               │
│  tab       next pane/field               │
│  shift+tab previous pane                 │
│  ?         this help                     │
│  ,         settings                      │
│  w         worktrees                     │
│  f         filter tree by state          │
│ ↓ more · PgUp/PgDn scroll                │
└──────────────────────────────────────────┘
```

| Where | Key | Action |
|---|---|---|
| Everywhere | `tab` / `shift+tab` | Next / previous pane or field |
| | `?` | Key map |
| | `,` | Settings |
| | `w` | Worktrees |
| | `f` | Filter the tree by state |
| | `esc` | Back one level: from Needs you, the agent pane or the chat to the org pane (the chat keeps its draft); zoomed, restores; in the org pane, clears a filter, else nothing. Never quits (step 1 of first run asks). In Typing mode it goes to the agent; `ctrl+q` leaves Typing |
| | `q` | Quit; agents keep running. Not while typing in the chat, a form or the agent pane, and not in the half second a click or `enter` waits for an agent's terminal (§4.1.2) |
| | `ctrl+c` | Quit, when not typing and not waiting for an agent's terminal (§4.1.2) |
| | `ctrl+↑` / `ctrl+↓` | Resize the chat pane |
| Org pane | `↑` `↓` | Move |
| | `←` `→` | Fold / unfold |
| | `enter` | Agent or Duo: start typing in the agent pane (Selected mode if it is stopped or errored). Team or role: fold / unfold |
| | `space` | Give work (agent or role) |
| | `s` | Stopped agent: start it with no task (it waits, idle). Running agent: stop it (asks first) |
| | `r` | v1.1: resume an old session of a stopped or errored agent (§4.13); a running agent says to stop it first. Also in the agent pane's Selected mode |
| | `n` | New here… |
| | `e` | Edit; on Duo, Edit org |
| | `m` | Move to… |
| | `d` | Delete (stopped agents only) |
| | `z` | Zoom the selected agent |
| | `v` | Last reply on / off |
| | `o` | Open `org.toml` in `$EDITOR` |
| Needs you | `↑` `↓`, `enter` | Move; go to the agent (start typing) or open its card (§4.6) |
| Agent pane | `enter` / `ctrl+q` | Start / stop typing (a click on a running agent also starts it) |
| | `z` | Zoom / restore |
| | `space` | Give work |
| | `r` | v1.1: resume an old session (§4.13), as in the org pane; not while typing |
| | `PgUp` / `PgDn` | Scroll the agent's scrollback (sent to the agent when its CLI takes the mouse, §4.1.2) |
| Chat | `enter` / `alt+enter` | Send / new line |
| | `@` / `/` | Agent / command suggestions |
| | `PgUp` / `PgDn` | Scroll the chat history |
| Card dialog | `y` / `e` / `n`, `←→↑↓`, `enter`, `esc` | Approve / edit / reject; move between buttons; press the focused one; close (§4.6) |
| Dialogs and forms | `↑` `↓`, `tab` / `shift+tab`, `enter`, `esc`, `ctrl+s` | Move between fields and buttons (`↑↓` and `tab` everywhere; `←→` on a button row), act on the focused button, cancel, save. `↑↓` are the popup's inside an open Model, Effort or Directory popup (and `tab` / `enter` take the highlighted folder there), and in the Own prompt they move between its lines and leave only from the first or last line (§4.4.1); `←→` stay a text field's cursor and a choice's options |
| Give work | `↑` `↓` | Pick the agent in "Goes to" |
| Mouse | click, right-click, wheel, drag | Select and focus, and type to a running agent (§4.1.2); node menu; scroll the pane under the pointer, typing too (§4.1.2); move a node or resize the chat. A plain drag in the agent pane, the chat or Needs you selects text and copies it on release; a drag that starts on an org row moves the node, and one on the chat's top edge resizes it. |

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

**Errors on rows:** Duo failing to start; a CLI that isn't installed; a missing directory (only that agent is invalid; the rest of the org works); a crash; a worktree that couldn't be created (a stopped agent stays stopped and an errored one stays errored, with this error as its reason; the task is kept); an untested CLI version (a notice in `fg.muted`, not an error).

```
┌─ Org ────────────────────────────────────────────────────────────────┐
│ ⬢ Duo · your manager agent          ✕ errored    1m                  │
│     opencode not found (code 127) · e Edit org                       │
│ ├─ ▣ Payments                                                        │
│ │  ├─ ◇ Backend                                                      │
│ │  │  ├─ auth-service               ✕ errored    2m                  │
│ │  │  │   opencode not found · install it or change its CLI (e)      │
│ │  │  ├─ billing-api                ○ stopped    1d                  │
│ │  │  │   ✕ ~/code/billing not found · e to fix                      │
│ │  │  ├─ pay-api                    ✕ errored    5m                  │
│ │  │  │   exited unexpectedly (code 1) · space retry · s stop        │
│ │  │  └─ pay-web                    ○ stopped    8m                  │
│ │  │      ✕ couldn't create worktree: branch exists · space          │
│ │  └─ pay-qa                        ● working    3m                  │
│ │      notice: claude 2.3 untested (tested up to 2.1)                │
└──────────────────────────────────────────────────────────────────────┘
```

An errored agent stays errored, and in Needs you, until you give it work or stop it.

**A prompt that ends the agent by default:** Claude's folder-trust dialog in a new worktree is not an error. The agent waits and joins Needs you, and its pane says how to answer (§4.1.2, P2-D23).

**`org.toml` doesn't parse** (a banner in `status.error`; the last good org stays):

```
 ✕ org.toml line 42: unknown CLI "claud" · using your last good org · o open in $EDITOR
┌─ Org ──────────────────────────────────────────┐
│ ⬢ Duo · your manager agent    ◦ idle       1m  │
│ ├─ ▣ Payments                                  │
│ …                                              │
└────────────────────────────────────────────────┘
```

**An empty role:**

```
┌─ Org ──────────────────────────────────────────┐
│ ⬢ Duo · your manager agent    ◦ idle       1m  │
│ └─ ▣ Payments                                  │
│    └─ ◇ Frontend                               │
│        no agents yet · n to add one or ask Duo │
└────────────────────────────────────────────────┘
```

### 4.13 Resume an old session (v1.1)

Not in v1; built for v1.1 on the branch `feat/resume-session`, not released, with the hand check and other open items pending (PRD *Resume an old session*; see [Open items before merge](../plans/v1.1-roadmap.md#open-items-before-merge) in the v1.1 roadmap). `r` on a stopped or errored agent opens it; on a running agent the footer says `<agent> is running · stop it first (s)`, where the key is the active Start / stop binding (default `s`; with Start / stop unbound the note has no key). The engine's own refusal, for a client that does not know the keys, reads `<agent> is running · stop it first` with no key. `s` still starts fresh. On an errored agent whose CLI session is still up (a failed turn, TD §5.5), resuming closes that session first, as `s` does, then starts the chosen one.

**Where `r` appears** (as built):
- **Key table and Keys screen (§4.11, §4.10):** the Org pane row `r` and a Keys screen entry, Resume, right after Start / stop. `r` works in the Org pane and in the Agent pane's Selected mode (the pane focused, not Typing); while Typing, `r` goes to the agent. A dialog takes the key first, so `r` there is not resume.
- **`?` key map popup (§4.11):** `r` · resume in the Org pane column.
- **Footer in Selected mode:** a stopped or errored agent adds ` · r resume` (the key as bound); a running agent's footer is unchanged.
- **Right-click node menu:** an agent's menu lists `Resume… r` after Start / Stop, disabled with the reason `stop it first` while the agent runs, so the picker is reachable by mouse.
- **Key bindings screen:** Resume is a rebindable single-key action (id `resume`) in the Org and Agent pane contexts, one binding for both like Zoom, and the conflict check counts it.

```
┌─ Resume billing-api (1 of 36) ─────────────────────────────────────────────┐
│ ⌕ Search…                                                                  │
│                                                                            │
│ ▌ Retry loop for failed payouts                                            │
│   3 minutes ago · fix/payout-retry · 4.2MB · may be open elsewhere         │
│                                                                            │
│   Payout ledger migration                                                  │
│   1 day ago · main · 7.3MB                                                 │
│                                                                            │
│   Flaky webhook test                                                       │
│   2 days ago · fix/webhook-flake · 610KB                                   │
├────────────────────────────────────────────────────────────────────────────┤
│ enter resume · type to search · ↑↓ move · esc cancel                       │
└────────────────────────────────────────────────────────────────────────────┘
```

- **Entries:** the agent's CLI's sessions for its directory and each worktree it keeps, newest first: the session's title, then its age always, and its git branch and size only when the CLI provides them (Claude Code does; OpenCode does not, spike R2), in `fg.muted`, plus `worktree <task-id>` for a worktree's session. A session with no title shows its last prompt (injected text skipped; a session of slash commands only shows the command, e.g. `/clear`), cut with `…`.
- **In use:** a session live in another DuoHive agent adds `in use by <agent>` in `fg.muted`; `enter` on it does nothing, and the footer says `<agent> is using this session`.
- **Search:** typing filters by title and branch; the count becomes `{position} of {visible}` (below). `esc` clears the search first, then closes.
- **Written in the last 5 minutes:** the entry adds `may be open elsewhere`, and `enter` asks first: `Resume "<title>"? It was active 3 minutes ago and may still be open in another terminal.` with **Cancel** focused and **Resume**.
- **After resuming:** the agent starts on that session, in its directory or worktree, at rest, and the agent pane shows its old conversation; DuoHive types no task (queued messages still arrive at the first rest).
- **As built, where this section was silent:**
  - The count is `{position} of {visible}`: the position of the highlighted row within the entries that match the search, so the mockup's `(1 of 36)` is that format at the start, and `0 of 0` when none match (the list then reads `No sessions match`).
  - An entry shows only the fields it has, in the order age, branch, size, `worktree <task-id>`, `in use by <agent>`, `may be open elsewhere`; the branch is cut at 32 cells, and when the line is still too long the leading fields (age, branch, size, worktree) are cut first so the marks stay visible; only if the marks alone do not fit are they cut. Ages read `just now`, `N minutes ago`, `N hours ago`, `N days ago`.
  - A session with no title, prompt or command shows `(untitled)`.
  - A click on a row does what `enter` does; the wheel moves the highlight; while searching, the hint reads `esc clear search`.
  - The 5-minute confirmation is titled `Resume session?`; **Cancel** returns to the picker with its search and cursor intact.
  - A listing that arrives while another dialog, a right-click menu or the first-run wizard is open, or for an agent that no longer exists, is dropped, so a late answer never steals focus. A second listing while the picker is open is dropped too.
  - `Couldn't read <CLI>'s sessions: <reason>` is a footer note in `status.error`, not a dialog.
  - While the sessions are being read, `r` sets the footer note `reading <CLI>'s sessions…`; the picker opening or the `Couldn't read …` note replaces it, and the next action clears it. `r` is never locked out while it shows.
  - If a `settings.toml` from v1 already binds `r` to another action, that binding wins: Resume has no key until it is rebound (its footer hint, key-map row and menu key are then empty), and the Keys screen says `resume: r is taken by <action> · rebind it in Keys`. A saved `resume = "…"` is always honoured.
- **Empty:** `No <CLI> sessions for <dir>` with `esc close`. **Unreadable** (an untested Claude Code session format, or `opencode session list` failing): `Couldn't read <CLI>'s sessions: <reason>` in `status.error`, and the agent is unchanged.

## 5. Interaction rules

| Rule | Detail |
|---|---|
| One meaning per key | `enter` acts on the selection or the focused button; `esc` backs out one level and never quits; `tab` moves focus between panes and fields only; lists use `↑↓`; `?` opens help |
| Typing goes where focus is | In the agent pane's Typing mode, every key goes to the agent, `ctrl+c` included, until `ctrl+q`; while scrolled back in DuoHive's snapshot, `PgUp` / `PgDn` page it and `ctrl+q` leaves Typing and stays scrolled (§4.1.2). The chat input always takes typed text; `y` / `e` / `n` answer a card only while its dialog is open |
| Quit | `q`, or `ctrl+c` when not typing. Agents keep running. Both are ignored for the half second after a click or `enter` on an agent whose terminal isn't showing yet (§4.1.2) |
| Mouse parity | Every click does what a key does. A click on a pane focuses it (`tab`); on an org row it selects that row (`↑↓`), so an agent's terminal shows in the agent pane; a click on an agent that is not stopped or errored, or in the agent pane showing one, also starts Typing on release, as `enter` does (§4.1.2), and a team or role is selected without folding. Right-click opens the node's menu. The wheel scrolls the pane under the pointer, in Typing mode too (§4.1.2). Dragging a node moves it, and dragging the chat edge resizes it. Clicking a header count filters. In dialogs, forms, lists (Worktrees, Settings, Move to…, pickers, key bindings) and first run, a click on a button or row does what its key does. Dragging across text in the agent pane (zoomed too), the chat or Needs you selects it, drawn reversed, and the release copies it to the clipboard with a `copied N characters` note; the next click or key clears it. A drag that starts on an org row moves the node and one on the chat's top edge resizes it. |
| Nothing destructive by surprise | Stop, delete, remove-unmerged-worktree and replace-org ask first, with focus on Cancel; Duo's org changes, and its stops of starting, working or waiting agents, come as approval cards |
| Attention is earned | Amber appears only for waiting, quiet and approval cards; red only for errors. Desktop alerts only for Needs you entries and finished agents (each can be turned off) |
| Calm | No animation except the working spinner (on-screen rows only; off with Reduce motion); no sound |

## 6. Accessibility

- **State without colour:** every state has a glyph and a word; `~` is a glyph, not a dimmed colour. Colour-blind safety is tested by the rule that no state is shown by colour alone.
- **Monochrome (`NO_COLOR`):** the focused pane keeps its heavy border, and the selected row keeps its `▌` bar.
- **Contrast:** under `auto`, the terminal's theme sets every colour, so its contrast is the terminal theme's; `dim` and the ANSI colours are what terminal themes are designed to keep readable. For `dark` and `light`, computed with the WCAG formula; the lowest ratio of each text token across `bg` and `selection` (`status.working` and Duo are `fg`):

| Token | Dark | Light |
|---|---|---|
| `fg` | 10.50 | 12.93 |
| `fg.muted` | 4.79 | 5.56 |
| `border.focus` | 6.05 | 5.51 |
| `status.attention` | 7.19 | 5.26 |
| `status.done` | 6.35 | 5.35 |
| `status.error` | 5.35 | 5.73 |

- **No blinking text.**

## 7. Copy

- **Short and specific:** say what happened and what to do next ("opencode not found · install it or change its CLI (e)").
- **Name the agent:** use the agent's name, not "the agent".
- **"You" and "Duo":** "you" for the user and "Duo" for the manager agent; never "the system".
- **One verb for work:** "Give work" is the only user-facing term for handing an agent a task.

## 8. UX checks

The screens are checked against the PRD's [UX principles](../product/prd.md#ux-principles) twice:

1. **After phase 2:** during phase 2 the author keeps a **friction log** (anything slow, confusing or missed). At the end of the phase, the UI/UX improvement pass fixes the top items and re-checks every screen.
2. **Before launch:** a second check covers the phase 3–4 screens (Duo's chat and cards, the suggestion popup, task to a role, first-run templates, the no-CLI screen, `Replace your org?`, Settings' Keys row and the Org group's `New org from template…` row, the key-binding editor, the `?` key map, the filter, Move to…, the right-click menu, and the model and effort dropdowns).

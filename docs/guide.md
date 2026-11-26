# DuoHive user guide

Get from install to a working team in five minutes, then pick the use case that fits you. Press `?` in DuoHive at any time for the full key map.

## 1. Start

```sh
duohive          # or: dh
```

First run checks for tmux and for Claude Code or OpenCode, asks your name, then offers a template:

| Template | Pick it when |
|---|---|
| **Agent Squad** | You want several tasks running in parallel and don't care which agent does which |
| **Product Team** | You ship stories with a backend, a frontend and a QA agent |
| **Engineering Org** | You run several projects, each team with its own repo |
| **Blank** | You want to build the org yourself |

Choose the CLI and the directories, then select *Create org*. Everything can be edited later (`e`).

## 2. The screen

- **Org** (top left): your teams, roles and agents, each with a status (grouped boxes in a colour per team, with Duo as the Manager card above them, or the classic tree: switch with `l`, at 100×30 or larger): `●` working, `◆` waiting on you, `✓` finished, `◦` idle, `○` stopped, `✕` errored.
- **Needs you** (below it): everything blocked on you, oldest first. Start here.
- **Agent pane** (right): the live terminal of the selected agent.
- **Duo pane** (bottom): chat with Duo, your manager agent.

`tab` moves between panes, `↑` `↓` move in the tree.

## 3. The daily loop

1. Select an agent and press **`space`**. Type the task. For a stopped agent, choose where it runs: its own directory or a fresh git worktree. A finished or idle agent takes the task as the next turn of its current session, where it already runs; to start in a fresh worktree, stop it first.
2. Watch the status. A click only selects a row, so the arrows keep working. Press **`enter`** to type into the agent, `ctrl+q` to stop typing, `z` to zoom.
3. When something is waiting, it appears in **Needs you** and you get a desktop alert. Select it, answer, move on.
4. Quit with `q`. Agents keep running in tmux; the next start reconnects.

## 4. Use cases

### Run many tasks in parallel (Agent Squad)

Give each task to the **Generalist** role, not to a named agent. A free agent takes it. Worktrees are on by default, so agents never collide in the same directory.

### Ship a story with a team (Product Team)

Paste the story into the Duo pane: *"Story: let users retry a failed payment. AC: …"*. Ask Duo to split it by role. Review the breakdown, then let Duo hand each subtask to the right agent. Each action shows as a `→` line. DuoHive never approves anything for you: commits, pushes and merges follow each CLI's own permission prompts, so an approval you saved ("always allow") can let a matching command run without asking again.

### Find out who is stuck

Ask Duo: *"What's blocking the Payments team?"* or open **Needs you**. A working agent that goes silent shows `◷ quiet` there too.

### Talk to one agent without leaving the screen

In the Duo pane, `@auth-service run the tests again` goes straight to that agent. It waits if the agent is busy and starts it if it is stopped.

### Continue an old session

Started work in Claude Code or OpenCode outside DuoHive? Select the stopped agent and press **`r`**. Pick the session and the agent continues it with its full context. The picker lists only sessions of that agent's CLI and directory (and its kept worktrees), so use an agent set to the same CLI and directory as the original work.

### Change the org

Ask Duo ("add a frontend agent to Payments") or use `n` new, `e` edit, `m` move. Duo's changes arrive as a card in **Needs you**: `enter` opens it, `y` approves, `n` rejects, `e` edits.

## 5. Keys

| Key | Does |
|---|---|
| `↑` `↓` · `tab` | move in the tree · change pane |
| `space` | give work to the selected agent |
| `enter` | type into the selected agent |
| `s` · `r` · `z` | start or stop · resume an old session · zoom |
| `n` · `e` · `m` | new · edit · move |
| `w` · `,` · `o` | worktrees · settings · open `org.toml` |
| `?` · `q` | all keys · quit (agents keep running) |

## 6. When something looks wrong

- **`tmux` not found:** install tmux 3.2 or later and start again.
- **An agent shows `~`:** its state was read from the screen because the CLI reported nothing. Treat it as a good guess.
- **`org.toml` banner:** you hand-edited it and it doesn't parse. DuoHive keeps running and never overwrites it. Fix the file (`o` opens it).
- **See the raw sessions:** `tmux -L duohive ls`.

More: [README](../README.md) · [requirements](product/prd.md) · [UI/UX spec](ux/ui-spec.md)

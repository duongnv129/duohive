# Benchmark results

## Phase 1

Basic `duohive bench` (idle CPU, memory, hook-to-screen time), run as the Phase 1 acceptance signal. Command: `cargo build --release -p duohive && ./target/release/duohive bench --agents 10`.

| | |
|---|---|
| Date | 2026-10-04 |
| Commit | 4b24efe |
| Machine | Apple M4, 10 CPUs, 16 GiB memory |
| macOS | 26.4.1 (build 25E253) |
| tmux | 3.7c |
| Linux | No run: no Linux machine was available. Reported only, not gating |

The machine was lightly loaded by other work (13 terminal sessions open; load average 2.4 to 4.3 on 10 CPUs), so these are not idle-machine figures.

### 10 agents (gating), three runs

#### Run 1

Load average (1/5/15 min) just before the run: 4.34 3.49 3.13

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   12.6 MB  target < 20 MB
hook → screen   p50 27 ms · p95 38 ms · max 38 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control client
tmux server     0.0 % CPU · 4.0 MB (not DuoHive: no target)
samples         24 timed hooks · 10-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 4.34 3.49 3.13 at start, 2.83 3.22 3.04 at the end
exit 0
```

#### Run 2

Load average (1/5/15 min) just before the run: 2.83 3.22 3.04

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   12.5 MB  target < 20 MB
hook → screen   p50 18 ms · p95 37 ms · max 40 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control client
tmux server     0.0 % CPU · 3.9 MB (not DuoHive: no target)
samples         24 timed hooks · 10-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 2.83 3.22 3.04 at start, 2.98 3.21 3.05 at the end
exit 0
```

#### Run 3

Load average (1/5/15 min) just before the run: 2.98 3.21 3.05

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   12.6 MB  target < 20 MB
hook → screen   p50 17 ms · p95 38 ms · max 38 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control client
tmux server     0.1 % CPU · 4.0 MB (not DuoHive: no target)
samples         24 timed hooks · 10-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 2.98 3.21 3.05 at start, 2.42 3.04 2.99 at the end
exit 0
```

### 100 agents (informational preview, not gating)

Load average just before the run: 2.42 3.04 2.99

```text
duohive bench · 100 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 3 %
memory (peak)   13.4 MB  target < 60 MB
hook → screen   p50 20 ms · p95 37 ms · max 39 ms   target < 2000 ms
counted         DuoHive = this process + its tmux control client
tmux server     0.2 % CPU · 5.8 MB (not DuoHive: no target)
samples         242 timed hooks · 100-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 2.42 3.04 2.99 at start, 2.07 2.88 2.94 at the end
exit 0
```

### Summary

| Run | Idle CPU | Memory (peak) | Hook to screen p50 / p95 / max | Result |
|---|---|---|---|---|
| 10 agents, run 1 | 0.0 % | 12.6 MB | 27 / 38 / 38 ms | all targets met |
| 10 agents, run 2 | 0.0 % | 12.5 MB | 18 / 37 / 40 ms | all targets met |
| 10 agents, run 3 | 0.0 % | 12.6 MB | 17 / 38 / 38 ms | all targets met |
| 100 agents (preview) | 0.0 % | 13.4 MB | 20 / 37 / 39 ms | within the 100-agent targets |

Targets at 10 agents: idle CPU under 1 %, memory under 20 MB, hook to screen under 1 s. The bench does not measure screen-read (`~`) freshness or start-up first frame; those are not covered by this run.

## Phase 1 acceptance: done 2026-10-04

The bench above is Step 1 of Task 30. The author ran the daily org (22 agents in 7 teams) in DuoHive and accepted Phase 1 on 2026-10-04.

Author's checklist: move the daily agents into `org.toml` (by form or by hand), run them for a working day, and check each against the state table (technical design 4.2):

- [x] starting, then working, then finished or idle
- [x] waiting on an approval
- [x] `~` appears only on screen-read transitions
- [x] quiet after 5 minutes of silence
- [x] errored on a crash
- [x] stopped on `s`
- [x] correct state after closing and reopening DuoHive

Any wrong status found later is logged as an issue with the agent's CLI version and the hook fixture that reproduces it.

### Carried to Phase 2

- Give work: count a pasted task as taken only when the CLI's prompt-submit hook arrives (OpenCode can drop keys sent soon after launch); task text starting with `/` untested on OpenCode.
- Long agent names are cut in the middle in the 50-column org pane; part of the UI/UX pass.
- A Claude turn that doesn't stop within 150 ms of Escape can restore its prompt before `/exit` (the 5 s grace kill covers it).
- Live check still worth doing: a newline or non-ASCII text in OpenCode's `--prompt=`, and `--effort=` on a real turn.

## Phase 2

The Phase 2 bench (Task 2.18) adds `--view`: the bench holds one live pane stream open on `bench-1`, from the moment every agent has started to the end of the idle window (P2-D4, Review Focus 3), and feeds it into the app's terminal widget. The view's tmux client, found by its own pid, counts as DuoHive next to the control client, in both CPU and memory. Command: `cargo build --release -p duohive && ./target/release/duohive bench --agents 10 --view`.

| | |
|---|---|
| Date | 2026-10-09 |
| Commit | 47df913 on the phase-2 branch (squash-merged to main as b497418), plus the fix-round changes committed with this record (`fix: e2e cleans its tmux server; exact view counts`: exact view counts, `MAX_BACKLOG` 1 MiB) |
| Machine | Apple M4, 10 CPUs, 16 GiB memory |
| macOS | 26.4.1 (build 25E253) |
| tmux | 3.7c |
| Linux | No run: no Linux machine was available. Reported only, not gating |

The machine was lightly loaded by other work (load average 2.7 to 3.7 on 10 CPUs), so these are not idle-machine figures.

### 10 agents with a live view (gating), three runs

#### Run 1

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   18.1 MB  target < 20 MB
hook → screen   p50 22 ms · p95 37 ms · max 39 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control and view clients
live view       1 stream on bench-1 · open to the end · 0.6 KB in 42 chunks · 0 lines from other panes · its client counted
tmux server     0.1 % CPU · 4.0 MB (not DuoHive: no target)
samples         48 timed hooks · 10-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 3.52 3.68 3.39 at start, 3.12 3.54 3.36 at the end
exit 0
```

#### Run 2

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   18.0 MB  target < 20 MB
hook → screen   p50 22 ms · p95 38 ms · max 41 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control and view clients
live view       1 stream on bench-1 · open to the end · 0.6 KB in 44 chunks · 0 lines from other panes · its client counted
tmux server     0.1 % CPU · 4.1 MB (not DuoHive: no target)
samples         48 timed hooks · 10-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 3.12 3.54 3.36 at start, 3.13 3.50 3.35 at the end
exit 0
```

#### Run 3

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   17.9 MB  target < 20 MB
hook → screen   p50 24 ms · p95 38 ms · max 45 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control and view clients
live view       1 stream on bench-1 · open to the end · 0.6 KB in 43 chunks · 0 lines from other panes · its client counted
tmux server     0.1 % CPU · 4.0 MB (not DuoHive: no target)
samples         48 timed hooks · 10-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 3.13 3.50 3.35 at start, 2.75 3.34 3.30 at the end
exit 0
```

### 10 agents without the view, for comparison (same build)

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   13.4 MB  target < 20 MB
hook → screen   p50 22 ms · p95 37 ms · max 41 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control client
tmux server     0.0 % CPU · 4.0 MB (not DuoHive: no target)
samples         48 timed hooks · 10-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 2.75 3.34 3.30 at start, 3.33 3.43 3.33 at the end
exit 0
```

### Summary

| Run | Idle CPU | Memory (peak) | Hook to screen p50 / p95 / max | Live view | Result |
|---|---|---|---|---|---|
| 10 agents + view, run 1 | 0.0 % | 18.1 MB | 22 / 37 / 39 ms | 1 stream, open to the end, 0 lines from other panes | all targets met |
| 10 agents + view, run 2 | 0.0 % | 18.0 MB | 22 / 38 / 41 ms | 1 stream, open to the end, 0 lines from other panes | all targets met |
| 10 agents + view, run 3 | 0.0 % | 17.9 MB | 24 / 38 / 45 ms | 1 stream, open to the end, 0 lines from other panes | all targets met |
| 10 agents, no view | 0.0 % | 13.4 MB | 22 / 37 / 41 ms | none | all targets met |

Targets at 10 agents: idle CPU under 1 %, memory under 20 MB, hook to screen under 1 s. All three are met with the live view open.

Memory is the target to watch. The live view adds about 4.6 MB: the view's own tmux client (about 4 MB) and the terminal emulator behind the pane. That leaves about 2 MB of headroom under 20 MB, against 7 MB in Phase 1. The streamed pane is small here (a fake agent's output, under 1 KB). To keep a stalled UI from using up that headroom, a view stream that falls more than 1 MiB behind is closed (`MAX_BACKLOG`, Task 2.18 fix round; it was 8 MiB). The bench does not measure a real agent's long output.

## Phase 3

### P3-D8: the MCP library and the binary size (Task 3.1, 2026-10-18)

The release binary on macOS (`cargo build --release -p duohive`), at the Task 3.1 tree:

| Build | Size (bytes) | Under 10 MB? |
|---|---|---|
| Without `rmcp` | 8,797,760 | yes |
| With `rmcp =3.5.1` (`server`, `transport-io`), a do-nothing stdio server reachable from `main` | 11,085,488 | no |

`rmcp` adds about 2.3 MB and takes the binary over the PRD's 10 MB target, so P3-D8's fallback applies: no `rmcp`. Task 3.2 writes the helper's MCP server as a hand-written JSON-RPC over stdio. The probe was removed after measuring.


### The `duohive mcp` helper (Task 3.2, 2026-10-18)

Release build, 20 runs of the helper started and asked to initialize.

| Measure | Median | Max |
|---|---|---|
| Resident memory | 2,880 KB | 2,912 KB |
| Start to `initialize` answered | 14.5 ms | 435 ms (the first, cold run) |

Each running agent holds one helper, so 10 agents hold about 29 MB of helpers. They are separate processes and are not counted in DuoHive's memory target.

## Phase 3 acceptance

Run on 2026-10-18 on macOS (aarch64), by the author, on the author's own org with real Duo on Claude Code and on OpenCode. Agent names are masked (`agent A`, `new agent`). Runs made before Duo was restarted on the new build did not count; each result below is from a Duo launched with the helper config (`launched_as_duo`).

### Items 1 to 3, on each CLI

| # | Check | Claude Code Duo | OpenCode Duo |
|---|---|---|---|
| 1 | "What's blocked, then fix it" entirely in the chat | PASS | PASS |
| 2 | A role task reaches a free agent; with none free, a `no_free_agent` card is approved and the new agent takes the task | PASS | PASS |
| 3 | A card answered while Duo was stopped is queued, Duo is not started, and the answer is delivered when Duo next runs | PASS, delivered 80 s later | PASS, delivered 30 s later |

Check 1, the chat lines (both CLIs):

```
you: what's blocked?
→ list agents   N agents
→ read agent A   read
duo: agent A is waiting on a permission prompt…
you: fix it: stop agent A
→ stop agent A   card raised
card: approved: stop agent A
```

Duo then acknowledged. No `tmux attach` and no typing into an agent pane was needed.

Check 2: the first role task went to a free agent. With no agent free in the role, a `no_free_agent` card held the task; approving it added the new agent to the role, which took the task. Duo received the `approved` answer.

Check 3: with Duo stopped, approving the card queued the answer and did not start Duo. When Duo was started, it received the answer and acted on it.

### Item 4: the 10-agent targets with the release binary

| Measure | Result | Target |
|---|---|---|
| Release binary | 9,711,712 B | under 10 MB |
| Idle CPU (`duohive bench --agents 10`) | 0.0 % | under 1 % |
| Memory (peak) | 13.8 MB | under 20 MB |
| Hook to screen p50 / p95 / max | 21 / 36 / 37 ms | under 1 s |

All 10-agent targets are met. The bench's fake agents run no helpers; the MCP helpers (about 2.9 MB each, measured in Task 3.2) are outside DuoHive's own budget.

### Observations (not defects)

- `s` on a running agent asks "Stop agent?" first; the runbook for a stopped-Duo check must say to confirm it.
- A give on a busy agent's row queues a message, not a role task.

### Deferred minor findings from the final review

Follow-ups, none blocking phase 3:

- #6: the helper's 30 s answer timeout is shorter than the 120 s creation wait for `start_agent` and `assign_to_role` in a worktree.
- #7: the helper lingers up to 60 minutes after its stdin ends when Duo's waits were dropped.
- #8: the OpenCode `evaluate` match `/^duohive_[a-z_]+$/` is broader than the 9 tool names.
- #9: `pending_tool_calls` has no cap.
- #10: `tools.rs` logs the call at info level, so message and task text reach the logs.
- #11: a new-agent card is answered `approved` when the task launch failed.
- #12: an agent can message itself.

## Phase 4

First numbers from the reworked bench (end-of-window memory, `~` freshness, first frame on reopen, helpers and exec reported). `make bench-gate` ran three counted runs with `--view` at each tier and took the worst value per target; the targets below are the PRD's.

| | |
|---|---|
| Date | 2026-10-20 / 2026-10-21 |
| Branch | `feat/phase-4-bench` |
| Machine | Apple M4, 10 CPUs, 16 GiB memory |
| macOS | 26.4.1 |
| tmux | 3.7c |
| Binary | 6,348,352 B (9,829,520 B before `[profile.release]`: `strip`, thin LTO, one codegen unit) |

**The load gate was disabled for this run** (`DUOHIVE_BENCH_IGNORE_LOAD=1`): the machine sat at a 1-minute load average of about 3, above the method's ceiling of 2, and the bench discards such runs. These figures are therefore a first reading, not the acceptance measurement; Task 4.10 re-runs the gate on a quiet machine. `make bench-gate` itself never disables the gate, and a discarded run is retried twice before the gate fails.

### 10 agents (worst of three counted runs)

| Measure | Result | Target |
|---|---|---|
| Idle CPU | 0.02 % | under 1 % |
| Memory (end of window) | 18.08 MB | under 20 MB |
| Hook → screen (max) | 67.11 ms | under 1000 ms |
| Screen-read `~` (p95) | 1983.65 ms | under 2000 ms |
| First frame (reopen) | 44.72 ms | under 100 ms |

### 100 agents (worst of three counted runs)

| Measure | Result | Target |
|---|---|---|
| Idle CPU | 0.03 % | under 3 % |
| Memory (end of window) | 12.81 MB | under 60 MB |
| Hook → screen (max) | 62.97 ms | under 2000 ms |
| Screen-read `~` (p95) | 1881.56 ms | under 2000 ms |
| First frame (reopen) | 58.94 ms | under 300 ms |

Every target passed. Two are close enough to treat as at risk in Task 4.2:

- **`~` freshness** sits at 1.98 s against 2 s at 10 agents (and 1.88 s at 100): the screen read waits for the settle mechanism before it can say `~`. Any tuning that speeds settle-read detection helps; a p95 a few tens of milliseconds slower would miss.
- **10-agent memory** is 18.08 MB against 20 MB, while the 100-agent run reports 12.81 MB: the 10-agent window's end caught more of the control client's growth than the 100-agent one. Re-measure on a quiet machine before drawing a conclusion.

The informational runs without `--view` were made but their verdict line printed `unknown`: the script's `sed` used a GNU-only `\|` alternation, fixed after this run. The counts are unaffected; the no-view figures are cosmetic and will be in the next gate output.

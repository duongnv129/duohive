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

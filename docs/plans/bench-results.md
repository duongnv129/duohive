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

### Run 1

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

### Run 2

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

### Run 3

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

The Phase 2 bench (Task 2.18) adds `--view`: the bench holds one live pane stream open on `bench-1`, from the moment every agent has started to the end of the idle window (P2-D4, Review Focus 3), and feeds it into the app's terminal widget. The view's tmux client counts as DuoHive, next to the control client, in both CPU and memory. Command: `cargo build --release -p duohive && ./target/release/duohive bench --agents 10 --view`.

| | |
|---|---|
| Date | 2026-10-09 |
| Commit | the Task 2.18 commit (on db92783) |
| Machine | Apple M4, 10 CPUs, 16 GiB memory |
| macOS | 26.4.1 (build 25E253) |
| tmux | 3.7c |
| Linux | No run: no Linux machine was available. Reported only, not gating |

The machine was lightly loaded by other work (load average 2.3 to 3.8 on 10 CPUs), so these are not idle-machine figures.

### 10 agents with a live view (gating), three runs

### Run 1

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   18.0 MB  target < 20 MB
hook → screen   p50 24 ms · p95 39 ms · max 41 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control and view clients
live view       1 stream on bench-1 · open to the end · 0.6 KB in 43 chunks · 0 lines from other panes · its client counted
tmux server     0.1 % CPU · 4.0 MB (not DuoHive: no target)
samples         48 timed hooks · 10-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 3.84 3.52 3.41 at start, 3.02 3.35 3.35 at the end
exit 0
```

### Run 2

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   18.0 MB  target < 20 MB
hook → screen   p50 21 ms · p95 38 ms · max 39 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control and view clients
live view       1 stream on bench-1 · open to the end · 0.6 KB in 42 chunks · 0 lines from other panes · its client counted
tmux server     0.1 % CPU · 4.0 MB (not DuoHive: no target)
samples         48 timed hooks · 10-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 3.02 3.35 3.35 at start, 2.76 3.22 3.30 at the end
exit 0
```

### Run 3

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   17.9 MB  target < 20 MB
hook → screen   p50 17 ms · p95 38 ms · max 40 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control and view clients
live view       1 stream on bench-1 · open to the end · 0.6 KB in 43 chunks · 0 lines from other panes · its client counted
tmux server     0.0 % CPU · 4.0 MB (not DuoHive: no target)
samples         48 timed hooks · 10-agent targets apply
not measured    screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)
machine         10 cpus · load (1/5/15 min) 2.76 3.22 3.30 at start, 2.26 3.02 3.22 at the end
exit 0
```

### Without the view, for comparison (same build)

Load average just before the run: 2.07 2.95 3.19

```text
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.0 %    target < 1 %
memory (peak)   13.2 MB  target < 20 MB
hook → screen   p50 21 ms · p95 37 ms · max 38 ms   target < 1000 ms
counted         DuoHive = this process + its tmux control client
```

### Summary

| Run | Idle CPU | Memory (peak) | Hook to screen p50 / p95 / max | Live view | Result |
|---|---|---|---|---|---|
| 10 agents + view, run 1 | 0.0 % | 18.0 MB | 24 / 39 / 41 ms | 1 stream, open to the end, 0 lines from other panes | all targets met |
| 10 agents + view, run 2 | 0.0 % | 18.0 MB | 21 / 38 / 39 ms | 1 stream, open to the end, 0 lines from other panes | all targets met |
| 10 agents + view, run 3 | 0.0 % | 17.9 MB | 17 / 38 / 40 ms | 1 stream, open to the end, 0 lines from other panes | all targets met |
| 10 agents, no view | 0.0 % | 13.2 MB | 21 / 37 / 38 ms | none | all targets met |

Targets at 10 agents: idle CPU under 1 %, memory under 20 MB, hook to screen under 1 s. All three are met with the live view open.

Memory is the target to watch. The live view adds about 4.8 MB: the view's own tmux client (about 4 MB) and the terminal emulator behind the pane. That leaves about 2 MB of headroom under 20 MB, against 7 MB in Phase 1. The streamed pane is small here (a fake agent's output, under 1 KB), so a real agent's long scrollback could take more. The bench does not measure that.

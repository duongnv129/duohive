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

## Phase 1 acceptance: pending

The bench above is Step 1 of Task 30. Phase 1 is not done until the author completes Step 2 and the roadmap row is updated.

Author's checklist: move the daily agents into `org.toml` (by form or by hand), run them for a working day, and check each against the state table (technical design 4.2):

- [ ] starting, then working, then finished or idle
- [ ] waiting on an approval
- [ ] `~` appears only on screen-read transitions
- [ ] quiet after 5 minutes of silence
- [ ] errored on a crash
- [ ] stopped on `s`
- [ ] correct state after closing and reopening DuoHive

Log every wrong status as an issue with the agent's CLI version and the hook fixture that reproduces it, and fix those before marking the phase done.

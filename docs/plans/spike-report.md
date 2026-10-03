# DuoHive v1 Phase 0–1: Spike Report

Findings from the phase 0–1 spikes (plan Tasks 3–5). Each section records what was measured, the
log that shows it, and the decision taken. The spike code is throwaway and lives in `spikes/`.
Its large raw logs (`spikes/*/out/`) are not committed, so the key lines are quoted here. The small evidence files the report cites (`out/summary.txt` and the `close-*`, `flags-*` and `selected-*` outputs) are committed.

## S0 tmux activity for silenced panes at 100 windows

Code: `spikes/s0-tmux-activity/` (`run.sh`, `reader.py`, `analyze.py`, `flags.sh`, `close.py`,
`selected.py`). Every figure below comes from `analyze.py` over `out/`; its output is
`out/summary.txt`. Logs are named `out/<version>-<mode>[-<mix>][-r2]-{notifications.log,cpu.csv,bursts.log,windows.txt}`.

### S0.1 Setup

- **Machine:** Apple M4 (Mac16,12, 10 cores), macOS 26.4.1 (25E253).
- **tmux:** 3.2a, built from source into `~/.local/tmux-3.2a`, and the latest release, 3.7c (Homebrew `/opt/homebrew/bin/tmux`).
- **Server:** a throwaway `-L duohive-s0-<pid>` server with `remain-on-exit on`, session `duohive`, a keeper window (`tail -f /dev/null`), then 100 windows:
  - **Noisy (33 windows):** `while :; do date; sleep 0.1; done`.
  - **Bursty (33 windows):** 10 s of `date` every 0.1 s, then 20 s quiet, repeating. Each window starts after its own offset of 8 to 37 s, so bursts are spread out.
  - **Idle (34 windows):** `cat`.
  - **Dead (1 extra window):** `sh -c 'sleep 20; exit 3'`, outside the mix.
- **Client:** one control client, `tmux -L … -C attach -t =duohive`, driven by `reader.py` (Python 3). The reader:
  - timestamps every line it reads;
  - opens an extra noisy window (`late`) at t = 60 s, pauses it at t = 70 s (pause mode only) and kills it at t = 90 s;
  - kills the server at the end, so `%exit` is logged.
- **CPU and memory:** sampled once a second with `ps -o %cpu=,rss=,time=` for the server, the reader and the `tmux -C` client process. CPU % is the change in cumulative CPU time divided by wall time, from 5 s after the start to the end. For the reader in the noisy runs it is `getrusage` user + sys from subscribing to the end.
- **Runs:**
  - **Main runs (120 s, the mix above):** `no-output`, `pause`, `poll` and `none` (no control client, the baseline), on each version.
  - **Cost runs, 100 idle windows (`MIX=idle`, 40 s):** the same four modes on each version.
  - **Cost runs, 100 noisy windows (`MIX=noisy`, 90 s):** the same four modes on each version, then `no-output`, `pause` and `poll` again (`RUN=2`, tagged `-r2`) to check that the order holds. On 3.7c, `no-output` and `pause` ran a third time (`-r3`).
- **Poll mode:** silences panes with `no-output`, adds no subscriptions, and sends `list-windows -F '#{window_id} #{window_activity} #{pane_dead} #{pane_dead_status}'` once a second over the same connection.

### S0.2 Reports for silenced panes

**Ground truth for the delay.** Just before the first `date` of each burst, a bursty window appends its index and the time in milliseconds (`perl Time::HiRes`) to `out/<tag>-bursts.log`. The delay of a burst runs from that time to the first `act` report (or poll reply line) for the window whose value is at least that second. Bursts that start within 1.5 s after subscribing or within 3 s of the end are skipped. `#{window_activity}` has 1 s resolution and tmux checks subscriptions once a second, so 0 to 1 s is the expected range.

**Do reports keep arriving for silenced windows?** Yes, with both methods on both versions. After silencing, the reader saw no `%output` or `%extended-output` from any silenced pane (`leaks: 0` in every run), and `act` reports kept arriving:

| Run (100 windows, 120 s) | Bursts measured | Median delay | p95 delay | Max delay | Noisy windows: reports per window per s, mean (max) | Bursty windows | Idle windows | Shortest gap between reports for one window |
|---|---|---|---|---|---|---|---|---|
| 3.2a `no-output` | 116 (0 missed) | 0.470 s | 0.899 s | 0.976 s | 0.962 (0.983) | 0.339 | 0 | 0.995 s |
| 3.2a `pause` | 112 (0 missed) | 0.470 s | 0.955 s | 1.133 s | 1.000 (1.000) | 0.366 | 0 | 0.992 s |
| 3.7c `no-output` | 113 (0 missed) | 0.510 s | 0.930 s | 0.992 s | 1.000 (1.000) | 0.355 | 0 | 0.994 s |
| 3.7c `pause` | 115 (0 missed) | 0.567 s | 0.974 s | 1.021 s | 1.000 (1.000) | 0.361 | 0 | 0.995 s |
| 3.2a `poll` (comparison) | 112 (0 missed) | 0.533 s | 0.975 s | 1.094 s | 1 line per window per s, every window, idle included | | | 0.951 s |
| 3.7c `poll` (comparison) | 116 (0 missed) | 0.478 s | 0.940 s | 1.015 s | 1 line per window per s, every window, idle included | | | 0.944 s |

- **At most one report per window per second:** confirmed. The shortest gap between two reports for one window was 0.983 to 0.995 s in every subscribe run. The one exception is 3.7c `no-output` r3, at 0.930 s: the reader was delayed by load on the machine, then caught up.
- **Rates at 100 noisy windows (reports per noisy window per second):**
  - 3.2a `no-output`: 0.984 / 1.011 (r1 / r2);
  - 3.2a `pause`: 0.999 / 0.991;
  - 3.7c `no-output`: 0.999 / 0.994 / 0.982 (r1 / r2 / r3);
  - 3.7c `pause`: 1.000 / **0.761** / **0.822**.
- **3.7c `pause` lost reports in two of three runs.** In runs r2 and r3, on many seconds only 40 to 80 of the 100 windows reported, and windows showed 2 s gaps in which their activity advanced by only 1 s, so their output had not been read that second. `no-output` runs made at the same time did not show this. The machine was shared and busy: load average 13.7 on 10 cores when r3 finished (`uptime`). So this is an observation under load, not an isolated cause.
- **Idle windows:** they send nothing after the first report.
- **New windows:** `@*` covers windows created after subscribing. The late window `@102` got 29 to 31 `act` reports in its 30 s of life (`grep 'act $0 @102 '` in each main log).
- **Pause mode, panes stay paused:** yes. Every pause run logged `%pause` for each pane: 103 in the main runs (102 panes at start plus the late one) and 102 in the noisy runs. No run logged `%continue`.
- **Pause mode, a new window must be paused explicitly:** yes. Created at t = 60 s, the late window streamed 83 `%extended-output %102 …` lines on both versions until DuoHive's explicit `refresh-client -A '%102:pause'` at t = 70 s, and none after it. `pause-after=1` never paused it on its own, because the client kept up.
- **No-output mode, new windows:** the late window sent 0 output lines. No command is needed per window.

### S0.3 Cost

Server and reader CPU (% of one core) and RSS at 100 windows. "Reader" is the Python reader. The `tmux -C` client process measured 0.00 % CPU and 1 to 4 MiB in every run, so it is left out.

**Idle: 100 idle windows (`MIX=idle`, 40 s; `ps`, 0.01 s CPU-time resolution, so ±0.03 %)**

| Mode | 3.2a server | 3.2a reader | 3.7c server | 3.7c reader |
|---|---|---|---|---|
| `none` (no client) | 0.00 % | n/a | 0.03 % | n/a |
| subscribe, `no-output` | 0.12 % | 0.09 % | 0.18 % | 0.09 % |
| subscribe, `pause` | 0.12 % | 0.06 % | 0.15 % | 0.08 % |
| `poll` | 0.18 % | 0.27 % | 0.21 % | 0.33 % |

Peak RSS when idle: server 3.2 to 4.2 MiB, reader 11 to 17 MiB.

**The mix: 33 noisy, 33 bursty and 34 idle windows (120 s; `ps`)**

| Mode | 3.2a server | 3.2a reader | 3.7c server | 3.7c reader |
|---|---|---|---|---|
| `none` (no client) | 4.82 % | n/a | 4.85 % | n/a |
| subscribe, `no-output` | 4.54 % | 0.14 % | 5.78 % | 0.19 % |
| subscribe, `pause` | 6.36 % | 0.24 % | 5.79 % | 0.20 % |
| `poll` | 5.65 % | 0.20 % | 5.07 % | 0.19 % |

**Noisy: 100 noisy windows (`MIX=noisy`, 90 s, two runs each; reader by `getrusage`, server by `ps`)**

| Mode | 3.2a server (r1 / r2) | 3.2a reader (r1 / r2, mean) | 3.2a server + reader mean | 3.7c server (r1 / r2) | 3.7c reader (r1 / r2, mean) | 3.7c server + reader mean |
|---|---|---|---|---|---|---|
| `none` (no client) | 10.16 % | n/a | 10.16 % | 10.64 % | n/a | 10.64 % |
| subscribe, `no-output` | 9.29 / 10.09 % | 0.241 / 0.235, **0.238 %** | **9.93 %** | 11.01 / 10.46 % | 0.352 / 0.389, **0.371 %** | **11.11 %** |
| subscribe, `pause` | 10.81 / 11.37 % | 0.282 / 0.385, **0.334 %** | **11.42 %** | 11.52 / 10.86 % | 0.367 / 0.317, **0.342 %** | **11.53 %** |
| `poll` | 11.42 / 9.53 % | 0.201 / 0.239, **0.220 %** | 10.70 % | 11.11 / 9.29 % | 0.216 / 0.244, **0.230 %** | 10.43 % |

Peak RSS at 100 noisy windows: server 6.9 to 11.5 MiB, reader 9.7 to 16 MiB.

**Third 3.7c runs (r3).** These were added after the anomaly above, under a load average of about 14:

| Mode | Server | Reader |
|---|---|---|
| `no-output` | 9.25 % | 0.461 % |
| `pause` | 9.78 % | 0.248 % (it received fewer reports) |

With r3 included, the 3.7c means are:
- server + reader: `no-output` 10.64 %, `pause` 11.03 %;
- reader: `no-output` 0.401 % and `pause` 0.311 %, both still above `poll`'s 0.230 %.

The conclusions below do not change.

**Noise.** Other work was running on the machine during the spike. Server CPU varies by about ±1 point between identical runs (for example, 3.2a `poll` measured 11.42 % and 9.53 %), so server differences under 1 point are not significant.

What the tables show:

- **Server CPU is set by the panes:** about 10 % of a core for 100 noisy panes, with or without a client. The control client adds no more than the noise between runs, about ±1 point.
- **Reader CPU is small either way:** under 0.4 % of a core.
- **Subscribe vs poll, 100 noisy windows:** both send about 100 lines a second (9,123 against 9,465 lines in the 3.7c noisy runs). Subscription lines are about twice as long (600 KB against 319 KB of log), and the subscribe reader measured more CPU than the poll reader on both versions, on the mean of the two runs. In every run but one, subscribe was higher. The exception is 3.2a run 2, where `no-output` measured 0.235 % against poll's 0.239 %, which is within noise.
- **Subscribe vs poll, idle:** subscribe sends nothing, so its reader CPU is 3 to 4 times lower than poll's.

### S0.4 Formats

Identical on 3.2a and 3.7c unless noted. Each line is quoted from a main-run log, without its timestamp.

**Window subscription** (`refresh-client -B 'act:@*:#{window_activity}'`). Fields are `name session-id window-id window-index pane-id : value`, and the pane is `-` for a window subscription:

```
%subscription-changed act $0 @1 1 - : 1791044589
```

**Pane subscription** (`refresh-client -B 'dead:%*:#{pane_dead} #{pane_dead_status}'`). While the pane is alive, `#{pane_dead_status}` is empty, so the value ends in a space:

```
%subscription-changed dead $0 @1 1 %1 : 0 
%subscription-changed dead $0 @101 101 %101 : 1 3
```

- **The `exit 3` window:** `pane_dead_status` arrives. The second line above was logged at t = 20.01 to 20.03 s on both versions, by subscription, and at 20.02 to 20.05 s by poll.

**Poll reply** (`list-windows -F '#{window_id} #{window_activity} #{pane_dead} #{pane_dead_status}'`). A live pane's line also ends in a space:

```
%begin 1791044832 570 0
@0 1791044830 0 
@1 1791044832 0 
@101 1791044832 1 3
%end 1791044832 570 0
```

**Window close.** A window going away is reported as `%unlinked-window-close @N`, never `%window-close`, on both versions. `close.py` (`out/close-3.2a.txt`, `out/close-3.7c.txt`) checked four ways for a window in the attached session to go away; each logged `%unlinked-window-close` on both versions:
- its process exits with `remain-on-exit off`;
- `kill-window` from another client;
- `kill-window` sent by the control client itself;
- a window in another session is killed.

The main runs' `kill-window` of `@102` shows the same: `%unlinked-window-close @102`. The `%window-close` line documented in the man page was never seen, so DuoHive must treat both lines as "window gone".

**Exit.** When the server is killed, the control client reads the following, then EOF:

```
%sessions-changed
%exit
```

`%exit` carries no reason here.

**Pause mode lines:**

```
%pause %1
%extended-output %102 0 : Sat Oct  3 23:26:10 +07 2026\015\012
```

`pause-after` switches output for the whole client from `%output` to `%extended-output`.

**Client flags for `refresh-client -f`** (`flags.sh`, `out/flags-*.txt`). Each flag was checked by reading back `#{client_flags}`:

| Flag | 3.2a | 3.7c |
|---|---|---|
| `active-pane` | set | set |
| `ignore-size` | set | set |
| `no-output` | set | set |
| `pause-after=N` | set | set |
| `read-only` | set | set |
| `wait-exit` | set | set |
| `no-detach-on-destroy` | not set (ignored) | set |
| `!flag` | clears the flag (`flags-*.txt`; `!no-output` also in `selected-*.txt`) | clears the flag (same) |

- **Unknown flags:** an unknown flag (`bogus-flag`) is ignored without `%error`, and so is `no-detach-on-destroy` on 3.2a. A typo fails silently, so check `#{client_flags}` if it matters.
- **No-output has no per-pane exception** (`selected.py`, `out/selected-*.txt`, both versions):
  - With `no-output` set, `refresh-client -A '%0:on'` and `'%0:continue'` produce no output for `%0`.
  - Output returns only after `refresh-client -f '!no-output'`, and then for every pane, until the others are paused with `-A '%N:pause'`.
- **Socket file:** 3.2a leaves its socket file behind after `kill-server`. The spike scripts delete it.

### S0.5 Decision

**Mechanism: `poll`.** The brief's rule, applied literally:

- **The delay condition holds** on both versions: reports arrive for silenced windows with a median delay of at most 1.1 s (0.470 to 0.567 s, S0.2).
- **The CPU condition fails** on both versions. At 100 noisy windows, the subscribe reader's CPU is above `poll`'s:
  - 3.2a: `no-output` 0.238 % and `pause` 0.334 %, against `poll` 0.220 %;
  - 3.7c: `no-output` 0.371 % and `pause` 0.342 %, against `poll` 0.230 %;
  - the order holds in both runs, except 3.2a run 2 (0.235 against 0.239), which is within noise.

So: not `subscribe`, but `poll`.

**Silencing (D25): `no-output`.** Reports keep arriving under both methods on both versions. At 100 noisy windows, server + reader CPU is lower with `no-output`: 9.93 % against 11.42 % on 3.2a, and 11.11 % against 11.53 % on 3.7c. With the 3.7c r3 runs included, it is 10.64 % against 11.03 %. The gap is within run-to-run noise, and a tie would also give `no-output`. Two more points favor `no-output`:
- it needs nothing per new window;
- on 3.7c, `pause` lost reports under load in two of three runs (S0.2).

**Commands DuoHive sends** over its one control connection (`tmux -L duohive -C attach-session -t =duohive`):

1. **On attach, and again after every reattach:**
   ```
   refresh-client -f no-output,ignore-size
   ```
2. **Once a second, for every agent at once:**
   ```
   list-windows -F '#{window_id} #{window_activity} #{pane_dead} #{pane_dead_status}'
   ```
   Each reply line is `@<id> <activity epoch s> <0|1> <status or empty>`. An agent's activity changed when its `window_activity` differs from the previous reply. A window absent from the reply has gone.
3. **For a new window:** nothing. `no-output` covers it (0 output lines from the late window), and it appears in the next `list-windows` reply.
4. **For a closed window:** treat `%unlinked-window-close @N` and `%window-close @N` the same, and also treat a window missing from the reply as gone. **For server loss:** `%exit` then EOF.

This is the fallback the TD already named, now the primary mechanism. No `refresh-client -B` or `-A` commands are needed.

**Separators.** The `-F` separators are free to choose. The spike used spaces. Plan Task 19 uses tabs (`list-windows -t =duohive -F '#{window_id}\t#{window_activity}\t#{pane_dead}\t#{pane_dead_status}'`), and tabs are better: a live pane's `#{pane_dead_status}` is empty, so with spaces its line ends in a trailing space that is easy to mis-split. With tabs the last field is simply empty.

### S0.6 TD changes

S0.5 differs from TD §4.1 (`poll` instead of subscriptions; silencing named as `no-output`), so `docs/design/technical-design.md` was edited:

- **§3.4, `duohive-tmux` row:** "control mode, windows, flow control, subscriptions" → "control mode, windows, no-output silencing, activity polling".
- **§4.1, tmux-activity row:**
  - silencing names `no-output` (spike S0), and notes that it silences every pane on the connection, so a live view of the selected pane (phase 2) needs its own path;
  - activity comes from one `list-windows -F '#{window_id} #{window_activity} #{pane_dead} #{pane_dead_status}'` per second for all agents, in place of the `refresh-client -B` subscription.
- **§4.1, S0 bullet:** records the result (subscriptions worked, at the same delay, but cost more reader CPU than one poll at 100 noisy windows) and the window-close line.
- **§4.3, "No per-agent polling" bullet:** now says one activity query a second for all agents, not "pushed by tmux".
- **§4.3, "One full screen at a time" bullet (review fix):** replaced with "No pane streams in phases 0–1". Every pane is silenced by `no-output`, and the phase-2 live view of the selected agent needs its own path.
- **§12, phase 4 row (review fix):** "flow-control tuning" → "silencing and activity-poll tuning".
- **§9, tmux runtime row:** "activity subscriptions" → "activity polling".
- **§10, phase 1 bullet:** "silencing (no-output or paused flow control, chosen by spike S0)" → "`no-output` silencing (spike S0)".
- **§13, settle-mechanism row:** rewritten for the poll mechanism. Its risk is now that the poll is too slow or costs too much at 100 windows; the mitigation is that spike S0 measured it, and the bench re-measures it.
- **§13, silencing row:** names `no-output`, with the same mitigation.

## S1 Claude Code

(Task 4)

## S2 OpenCode

(Task 5)

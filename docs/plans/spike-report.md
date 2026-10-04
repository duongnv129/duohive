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

**Separators.** The `-F` separators are free to choose. The spike used spaces. Plan Task 19 uses tabs (`list-windows -t =duohive -F "#{window_id}\t#{window_activity}\t#{pane_dead}\t#{pane_dead_status}"`), and tabs are better: a live pane's `#{pane_dead_status}` is empty, so with spaces its line ends in a trailing space that is easy to mis-split. With tabs the last field is simply empty. Write the format in double quotes: tmux turns `\t` into a real tab only there, and inside single quotes it passes a literal backslash-t. This was checked on 3.2a and 3.7c.

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

Code: `spikes/s1-claude/` (`launch.sh`, `record-hook.sh`, `mcp-dummy.py`, `cap.sh`, `keys.sh`, `export.py`). `launch.sh <agent> [task]` writes `settings.json` (hooks), `mcp.json` and `command.txt` to `/tmp/duohive-spike/run/<agent>/` and starts `claude` in a window of `tmux -L duohive-spike` (120×40, `remain-on-exit on`, a clean environment with no `CLAUDE*` variables), with `DUOHIVE_TOKEN` set on the window. Every hook runs `record-hook.sh <agent> <ClaudeEvent>`, which saves stdin to `hooks/<ClaudeEvent>-<n>.json` and a line to `hooks/index.log`. `export.py` copies the raw captures into the fixtures with home paths changed to `/HOME`, emails and the user name replaced, and thinking signatures (opaque blobs that embed account ids) blanked; nothing else is changed.

Fixtures: `crates/duohive-cli/tests/fixtures/claude/2.1.288/` (paths below are relative to it). Screens are raw `capture-pane -p -J` output, padding included (`.editorconfig` keeps it).

Scenarios, all with `--model haiku` in the throwaway directory `/private/tmp/duohive-spike/work` (new to Claude Code at the start), on 2026-10-04:

| Agent | Scenario | Hooks (`hooks/index.log`) |
|---|---|---|
| a1 | start with no task; trust dialog answered "Yes" | `SessionStart-1` |
| a1 | sentinel turn | `UserPromptSubmit-1`, `Stop-1` |
| a1 | write `hello.txt`, approval answered Yes | `UserPromptSubmit-2`, `PreToolUse-1`, `PermissionRequest-1`, `Notification-1`, `PostToolUse-1`, `Stop-2` |
| a1 | write `no.txt`, approval answered No | `UserPromptSubmit-3`, `PreToolUse-2`, `PermissionRequest-2`, `Notification-2`, then nothing |
| a1 | two-line task pasted with `paste-buffer -p`, then `Enter` | `UserPromptSubmit-4`, `Stop-3` |
| a1 | quit with `/exit` `Enter` | `SessionEnd-1` |
| a2 | start with a task (`--effort low`) | `SessionStart-2`, `UserPromptSubmit-5`, `Stop-4` |
| a2 | turn, then `kill -9` of the `claude` process mid-turn | `UserPromptSubmit-6`, then nothing |

### S1.1 Version

`claude --version` → `2.1.288 (Claude Code)`. Regex: `^(\d+)\.(\d+)\.(\d+) \(Claude Code\)$`; the first two groups are the tested major.minor (`2.1`). The binary is `~/.local/share/claude/versions/2.1.288`, reached through `claude` on `PATH`. The `SessionStart` payload does not carry the version; the transcript entries do (`"version":"2.1.288"`, `replies/transcript-a1.jsonl`).

### S1.2 Standing instructions

`--append-system-prompt "<text>"`, appended to Claude Code's own system prompt (the default prompt, tools and the user's CLAUDE.md stay). Verified by the sentinel: instructions "If you are asked for the sentinel, reply with the single word HIVE-SENTINEL and nothing else", task "What is the sentinel?", reply `HIVE-SENTINEL` (`replies/Stop-1.last_assistant_message.txt`, `screens/at-rest-after-turn.txt`; again as a first prompt in `replies/Stop-4.last_assistant_message.txt`). a1 ran in a directory with no earlier session, so the answer did not come from a memory plugin.

### S1.3 Hooks

Configured by one per-launch file, `--settings <run>/settings.json`, in the standard shape `{"hooks": {"<Event>": [{"matcher": "", "hooks": [{"type": "command", "command": "<cmd> <agent> <Event>"}]}]}}`. Each hook gets its JSON payload on **stdin**, so `duohive hook` reads it there, as TD §4.1 assumes. `DUOHIVE_TOKEN`, set on the tmux window, reached every hook (`token=set` on every line of `hooks/index.log`). The hook's parent process is the `claude` process, which is the pane process. Every payload has `session_id`, `transcript_path`, `cwd` and `hook_event_name`; all but `SessionStart` also have `prompt_id`.

| Claude Code hook (matcher) | DuoHive event | Fires | Payload fields used | Fixture |
|---|---|---|---|---|
| `SessionStart` (`""`; payload `source` is `startup`) | `session-start` | once the input box is ready, after the trust dialog if one is shown | `model` (resolved id, e.g. `claude-haiku-4-5-20251001`), `transcript_path` | `hooks/SessionStart-1.json`, `hooks/SessionStart-2.json` |
| `UserPromptSubmit` (`""`) | `busy` | when a prompt is submitted, a first-prompt task included | `prompt` (multi-line kept) | `hooks/UserPromptSubmit-1.json` … `-6.json` |
| `PreToolUse` (`""`) | `busy` | before each tool call, before its approval prompt | `tool_name`, `tool_input` | `hooks/PreToolUse-1.json`, `-2.json` |
| `PostToolUse` (`""`) | `busy` | after a tool ran; the first hook after an approval answered Yes | `tool_name` | `hooks/PostToolUse-1.json` |
| `PermissionRequest` (`""`) | `needs-input` | at once when the approval prompt is shown (same second as `PreToolUse`) | `tool_name`, `tool_input` (the prompt text for the card) | `hooks/PermissionRequest-1.json`, `-2.json` |
| `Notification` (`notification_type` = `permission_prompt`) | not used (registered in the spike with matcher `""`; the adapter does not register it) | about 6 s after the approval prompt is shown | `message` ("Claude needs your permission") | `hooks/Notification-1.json`, `-2.json` |
| `Stop` (`""`) | `turn-end` | when a turn ends normally | `last_assistant_message` (S1.4) | `hooks/Stop-1.json` … `-4.json` |
| `StopFailure` (`""`) | `turn-end` | registered; did not fire (it is for turns ended by an API error) | — | none |
| `SessionEnd` (`""`) | `session-end` | on `/exit` | `reason` (`prompt_input_exit`) | `hooks/SessionEnd-1.json` |

`Notification` is dropped: `PermissionRequest` fires at once for the same prompt and carries the tool, while `Notification` comes about 6 s later with only a fixed message; a late one after the user has already answered Yes would mark a working agent waiting again. The adapter therefore registers `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PermissionRequest`, `Stop`, `StopFailure` and `SessionEnd`, all with matcher `""`.

Also registered and never fired: `PermissionDenied` (it reports denials by auto mode's classifier, not the user's "No"). `Notification` with `idle_prompt` did not fire within 90 s at rest (a2, after `Stop-4`), so it is not used. The `PermissionRequest` hook prints nothing, so it makes no decision and the prompt still shows (`screens/approval-write.txt`); the adapter must never return a decision from it (TD §2).

**Coverage:** all five DuoHive events have a hook (`HookCoverage` complete, `last_reply` from a hook), with one exception that applies to every CLI: no hook reports the user's answer to an approval prompt, so while an agent is waiting, settled screen reads apply whatever the coverage (at rest → finished/idle, busy → working; plan D2, TD §4.2). Three gaps are not events but transitions with no hook, and go to the other sources (S1.13):

- **Approval answered No:** the turn ends with "User rejected write to …" and the prompt returns, but no `Stop`, `PostToolUse` or `PermissionDenied` fires (`hooks/index.log` after `Notification-2`; `screens/after-approve-no.txt`). The agent stays waiting until a settle read finds it at rest, which the waiting exception above allows despite complete coverage. An answer of Yes is reported only by the tool's `PostToolUse`, when the tool ends, so a long-running tool also needs the screen read (busy → working).
- **Trust dialog:** appears before `SessionStart` (S1.8), so a new directory sits in `starting` with no hook.
- **Crash:** `kill -9` fires no `SessionEnd`; tmux reports the death (S1.9).

Global settings still apply: `--settings` adds to the user's settings, so the user's own hooks and plugins run too (here a memory plugin's `SessionStart` output and "4 async Stop hooks completed" appear on screen, `screens/at-rest-no-task.txt`, `screens/approval-write.txt`). DuoHive's hooks are additional and do not depend on them.

### S1.4 Last reply

- **Hook:** `Stop` carries the final assistant text as `last_assistant_message` (`hooks/Stop-2.json`: "File created at `/private/tmp/duohive-spike/work/hello.txt` with content \"hi\"."). Extracted as-is to `replies/Stop-<n>.last_assistant_message.txt`. This is the primary source; no file read is needed.
- **Transcript (fallback):** `transcript_path`, `~/.claude/projects/<cwd with / and . as ->/<session_id>.jsonl`, one JSON object per line. Many entry types (`attachment`, `mode`, `permission-mode`, `last-prompt`, `ai-title`, `system`, `file-history-snapshot`) surround the conversation; only `type: "user"` and `type: "assistant"` matter (`replies/transcript-a1.jsonl` keeps only those). An assistant message is split over several entries, one per content block (`thinking`, `text`, `tool_use`), sharing `message.id`.
- **Extraction rule (transcript):** skip entries with `isSidechain: true`; a turn starts at a `user` entry whose `message.content` is a string (tool results are arrays); the reply is the `text` blocks of the last `assistant` message (`message.id`) in that turn that has any, joined. Applied to `replies/transcript-a1.jsonl`, it gives exactly `last_assistant_message` for all three completed turns (`HIVE-SENTINEL`, the `hello.txt` line, `OK`). The denied turn has no text; its tool result entry carries `"toolDenialKind": "user-rejected"`.

### S1.5 MCP

`--mcp-config <run>/mcp.json` with `{"mcpServers": {"duohive": {"type": "stdio", "command": "python3", "args": [".../mcp-dummy.py"]}}}`. The `/mcp` dialog lists it as `✔ duohive  1 tool` (`screens/mcp-list.txt`), with no approval dialog (unlike a project `.mcp.json`). It is not written anywhere: the project's entry in `~/.claude.json` keeps `mcpServers: {}` (S1.12). The tool's name for Claude is `mcp__duohive__ping`; it was not called (that would show an approval prompt, like any MCP tool). `--strict-mcp-config` exists to drop the user's other servers; DuoHive does not pass it, so the user's servers stay available.

### S1.6 Model and effort

- **Flags:** `--model <alias|full id>` (aliases in `--help`: `fable`, `opus`, `sonnet`; `haiku` also works and resolved to `claude-haiku-4-5-20251001`, `hooks/SessionStart-1.json`). `--effort <low|medium|high|xhigh|max>`.
- **Effort on a model without it:** `--effort low` with Haiku 4.5 starts with no error; the `/model` picker says "Effort not supported for Haiku 4.5" (`screens/model-picker.txt`) and, on Opus 5.5, "Low effort" (`screens/model-picker-opus.txt`), so the flag applies where supported and is ignored otherwise.
- **Listing:** no non-interactive model listing ("no listing"). The only list is the `/model` picker, whose Enter "becomes the default for new sessions", a write to the user's settings, so DuoHive never drives it (it was closed with `Esc`; `~/.claude/settings.json` unchanged, `screens/model-closed.txt`). Fallback: read the `--model` aliases and the `--effort` choices from `claude --help` at runtime, and let the user type any other model.

### S1.7 Screens

Rules match the screen with trailing blank rows dropped and each line's trailing whitespace trimmed. The last two lines are the status line (here the user's own custom one) and the mode line (`⏸ manual mode on`), so rules must not use the last line; they anchor on the input box, a `❯` line between two full-width `─` rules.

| State | Distinguishing text | Fixture |
|---|---|---|
| Trust dialog (start-up) | `Quick safety check: Is this a project you created or one you trust?` and `Yes, I trust this folder`; the default choice is `❯ No, exit`, so `Enter` alone quits | `screens/startup-trust.txt` |
| Working | a line just above the input box matching `^\S (\S.*)… \(\d+s` (spinner glyph, verb, `…`, elapsed time), e.g. `✽ Elucidating… (1s)`, `· Architecting… (2s · thinking)` | `screens/working-1.txt`, `screens/working-2.txt`, `screens/working-3.txt` |
| Approval prompt | no input box; `Do you want to …?` then `❯ 1. Yes` … `3. No` and `Esc to cancel · Tab to amend` | `screens/approval-write.txt`, `screens/approval-write-2.txt` |
| At rest | input box present, no working line; after a turn the line above it is `✻ <Verb>ed for <n>s · done <time>` (no `…`) | `screens/at-rest-no-task.txt`, `screens/at-rest-after-turn.txt`, `screens/after-approve-yes.txt`, `screens/after-approve-no.txt`, `screens/with-task-done.txt` |
| Exited | `Resume this session with:` and `claude --resume <id>`, then tmux's `Pane is dead (status 0, …)` | `screens/after-exit.txt` |
| Killed | the last frame, then tmux's `Pane is dead (signal kill, …)` | `screens/after-kill9.txt` |

Other screens: a slash-command menu while `/exit` is typed (`screens/exit-typed.txt`), the `/mcp` and `/model` dialogs (`screens/mcp-list.txt`, `screens/model-picker.txt`), and the multi-line paste in the input box (`screens/paste-before-enter.txt`). 1.5 s after a launch with a task the screen was still blank (`screens/startup-with-task-1s.txt`).

### S1.8 Ready

- **Without a first prompt:** `SessionStart` (`source: startup`) fires when the input box is drawn, about 1–2 s after launch in a trusted directory. Typing before that is not needed: DuoHive sends no keys until then.
- **With a first prompt** (`claude … "<task>"`): `SessionStart` then `UserPromptSubmit` for the task one second later (`hooks/index.log` 00:46:30 → 00:46:31); the task is submitted by Claude Code itself, with no keys. **`--` before the task:** accepted, and required for a task that starts with `-`: `claude --model haiku -p -- "-rf reply with the single word OK"` replied `OK` (exit 0), while the same command without `--` failed with `Error: --resume requires a valid session ID …` (`-rf` read as `-r f…`, exit 1). Checked in `-p` mode, which shares the option parser with the interactive launch; so the adapter puts `--` before the task.
- **Start-up prompts:** the workspace-trust dialog, shown the first time Claude Code runs in a directory (`screens/startup-trust.txt`). It comes before `SessionStart`, so no hook fires until it is answered; with a first prompt, the task waits behind it. Answering "Yes" records `hasTrustDialogAccepted: true` for that directory in `~/.claude.json` (Claude Code's own write, S1.12); later launches there show no dialog (a2). It is an approval, so DuoHive does not answer it: the screen rule marks the agent waiting `~` and the user answers in the pane. In the spike it was answered with `Down`, `Enter`. Also seen: a one-line tmux notice above the `/mcp` dialog, "tmux focus-events off · add 'set -g focus-events on' …" (`screens/mcp-list.txt`), which is not a prompt; DuoHive's own tmux config can set `focus-events on`. Login and update prompts did not appear (the account was signed in, and the pinned version was current); their text is not captured here.

### S1.9 Quit

`/exit` typed (`send-keys -l /exit`), then `Enter`: exit status 0 (`#{pane_dead_status}` 0) and `SessionEnd` with `reason: "prompt_input_exit"` (`hooks/SessionEnd-1.json`, `screens/after-exit.txt`). The slash-command menu that opens while typing does not change what `Enter` runs (`screens/exit-typed.txt`). Quit keys: `Text("/exit")`, `Enter`. Killed with `kill -9`: `#{pane_dead}` 1 and `#{pane_dead_status}` empty, so a signal death has no exit status (the spike ran tmux 3.7c, where `#{pane_dead_signal}` also gave `kill`; that format exists only from tmux 3.3, and DuoHive supports 3.2, so it is not relied on); no `SessionEnd` (`screens/after-kill9.txt`, `hooks/index.log`), and Claude Code records `lastGracefulShutdown: false` for the project.

### S1.10 Paste

`load-buffer` of a two-line task, `paste-buffer -p` (bracketed paste), then `Enter`: both lines land in the input box without submitting (`screens/paste-before-enter.txt`), and `Enter` submits them as one turn: one `UserPromptSubmit` whose `prompt` is `"This is a two-line task.\nReply with the single word OK and use no tools."` (`hooks/UserPromptSubmit-4.json`), reply `OK` (`replies/Stop-3.last_assistant_message.txt`). Submit key: `Enter`.

### S1.11 Approvals untouched

The launch command (`/tmp/duohive-spike/run/<agent>/command.txt`) is `claude --append-system-prompt … --settings … --mcp-config … --model haiku [--effort low] [task]`: no `--dangerously-skip-permissions`, `--allow-dangerously-skip-permissions`, `--permission-mode` or `--allowedTools`. Every payload has `permission_mode: "default"`, and a file write prompted both times (`screens/approval-write.txt`, `screens/approval-write-2.txt`, `hooks/PermissionRequest-1.json`). Option 2 of the prompt ("Yes, and switch to accept edits … for this session") is the user's to choose.

### S1.12 Global config untouched

| File | Before | After a full cycle (2 launches, 6 turns, `/exit`, `kill -9`) |
|---|---|---|
| `~/.claude/settings.json` | `347192aa21dc3e027cbd40f7f1812787cd0d0d65` | `347192aa21dc3e027cbd40f7f1812787cd0d0d65` (equal) |
| `~/.claude.json` | `17702bf40022d9cc7ddb1276f7b921ce02e40fd1` | `59551d442f544281ce910ac47f2d7a62d32ceb96` (changed) |

There is no `~/.claude/settings.local.json`. `~/.claude.json` is Claude Code's own state file, rewritten by every running session: it changed between two checksums taken before any spike launch (`5057922…` → `17702bf…`) while other sessions ran. A key-level diff of before and after shows, from the spike's launches:

- `projects["/private/tmp/duohive-spike/work"]` added: `hasTrustDialogAccepted: true` (from answering the trust dialog), the last session's cost, tokens, durations and id, `lastGracefulShutdown: false` (after the `kill -9`), and empty `allowedTools`, `mcpServers`, `enabledMcpjsonServers`;
- counters: `numStartups`, `tipsHistory`, `tipLifetimeShownCounts`, `pluginUsage.*`.

The rest of the diff (another project's stats, cache slots) came from other sessions. All of it is Claude Code's normal bookkeeping for any launch; nothing DuoHive passes per launch (hooks, MCP server, instructions, model, effort) was persisted. So the S1.12 check for the adapter is: `~/.claude/settings.json` byte-equal, and in `~/.claude.json` the launched directory's `allowedTools` and `mcpServers` stay empty.

### S1.13 Fallbacks and TD changes

No mechanism is missing: instructions, hooks, MCP, model and effort are all per-launch flags, so no temporary file in `run/<agent>/` is needed beyond the `--settings` and `--mcp-config` files themselves. The gaps and their fallbacks:

| Gap | Fallback | Fixture |
|---|---|---|
| Approval answered No: no hook leaves waiting | tmux activity, then the settle read: input box, no working line → at rest; applied while waiting whatever the coverage (D2) | `hooks/index.log`, `screens/after-approve-no.txt` |
| Approval answered Yes: no hook at the answer itself | `PostToolUse` (or `Stop`) when the tool ends; before that, a settled busy screen while waiting → working (D2) | `hooks/PostToolUse-1.json` |
| Trust dialog before `SessionStart` | screen rule → waiting `~`; the user answers it (`startup_prompts` true) | `screens/startup-trust.txt` |
| Crash: no `SessionEnd` | pane death from the activity poll (tmux 3.7c in the spike); a signal death has an empty `#{pane_dead_status}`, so it is errored "exited unexpectedly" with no status; the signal can be named only where tmux ≥ 3.3 provides `#{pane_dead_signal}` | `screens/after-kill9.txt` |
| No model listing | `--model` aliases and `--effort` choices from `claude --help`; other models typed | `screens/model-picker.txt` |

TD edits (`docs/design/technical-design.md`):

- **§5.2 Claude Code row:** hooks column names the hook events; "effort setting" → `--effort`; replies column → `last_assistant_message` in the `Stop` payload, with the transcript as fallback.
- **§5.2 notes:** a "Spike S1" bullet (no fallback file needed; the three gaps above and where they are covered; no model listing and the picker writes the user's default); the model-and-effort bullet says where Claude Code's lists come from.
- **§6.5 Replies:** Claude Code's reply comes from `last_assistant_message` in the `Stop` payload; the transcript is the fallback.
- **§4.1 tmux-activity row:** the poll format is unchanged; it notes that a CLI killed by a signal leaves `#{pane_dead_status}` empty, has no exit status and is errored "exited unexpectedly".
- **§4.2:** the intro adds the waiting exception (settled screen reads apply while waiting whatever the hook coverage); the unexpected-exit row says "non-zero status or no status (killed by a signal)".
- **§5.5 Failed launch or crash:** a signal death has no exit status and shows "exited unexpectedly", naming the signal only where tmux ≥ 3.3 provides `#{pane_dead_signal}`.

Plan edits (`docs/plans/phase-0-1-plan.md`, review fixes): D2 and Task 12's screen-read rules and test descriptions add the waiting exception; D22 and Task 20 take Claude Code's reply from `last_assistant_message` in `parse_hook`, with the transcript read by `enrich_hook` only when the field is missing.

## S2 OpenCode

(Task 5)

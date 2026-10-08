# Spike S3: a live view of one pane at 100 agents (throwaway)

Throwaway code for spike S3 (phase-2 plan Task 2.1). Nothing in `crates/` imports or runs it.
Results and the decision are in `docs/plans/spike-report.md`, section S3. Every tmux run uses a
throwaway `-L duohive-s3-…` server started with `-f /dev/null`, killed on exit.

- `run.sh <tmux-binary> <a|b|c|d>`: S0's generator (a keeper window and 100 windows running
  `while :; do date; sleep 0.1; done`), an echo window (`stty -icanon; exec cat >/dev/null`) and
  a history window (3,000 coloured lines), then `view.py`. `NOISY=1` keeps one noisy window (the
  viewed one) to isolate the view's own cost; `HEAVY=1` makes it print about 1,000 lines a second.
- `view.py`: DuoHive's main connection (S0.5: `no-output,ignore-size`, `list-windows` once a
  second), then, per candidate (P2-D2): a baseline phase; a view of the echo pane with 200
  `send-keys -l Z`, each timed until its echo arrives; a view of a noisy pane, with CPU over the
  baseline and output lines per pane on both connections; `capture-pane` costs (S3.6).
  - (a) second control client, `no-output`, `refresh-client -A '%P:on'` (and `:continue`);
  - (b) second control client, `!no-output`, every other pane `off`, all on one command line;
  - (c) `pipe-pane -O -t %P 'exec cat > <fifo>'`, read from the FIFO;
  - (d) `capture-pane -e -p -t %P` every 50 ms, changed frames only.
- `bench.sh`, `bench-isolated.sh`: the runs in the report. `analyze.py`: `out/summary.txt`.
- `record.sh <tmux-binary> claude|opencode <out-dir> <work-dir>`: records a CLI's raw pane bytes
  at 80x20 with `pipe-pane -O` from its first byte, with checkpoints (stream so far,
  `capture-pane -e -p`, `capture-pane -e -N -p`, `capture-pane -p`, cursor and alternate screen).
- `export.py`: copies the recordings to `fixtures/` (the fixtures for Task 2.4), replacing only
  the user's OpenCode model and provider names with strings of the same length.
- `fidelity/`: a Rust tool on `vt100 =0.16.2` that replays `stream.bin` and compares the screen
  with tmux's (text, cell colours and attributes, cursor, alternate screen), and lists the
  sequences `vt100` does not handle. `cargo run --release -- ../fixtures/*/*/`.
- `resize.sh <tmux-binary> claude|opencode <work-dir>`: S3.5, with DuoHive's server options.
- `trust.sh <trusted-git-repo> <out-dir>`: S3.7. Adds a detached worktree (removed after),
  starts `claude` in it and in a fresh directory, and never answers the dialog.
- `probe-blocks.sh <tmux-binary>`: one `%begin`/`%end` block per command on a multi-command line.

Logs go to `out/` (not committed, except `out/summary.txt`).

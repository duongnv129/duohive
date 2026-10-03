# Spike S0: tmux activity for silenced panes (throwaway)

Throwaway code for spike S0. Nothing in `crates/` imports or runs it; it can be deleted once
`duohive-tmux` exists. Results and the decision are in `docs/plans/spike-report.md`, section S0.

- `run.sh <tmux-binary> <windows> <seconds> <silence>`: starts a throwaway server
  (`-L duohive-s0-<pid>`, killed on exit), a keeper window, `<windows>` windows (a third noisy,
  a third bursty, a third idle) and a `sh -c 'sleep 20; exit 3'` window, then runs one control
  client. `<silence>` is `no-output` or `pause` (subscribe), `poll` (no-output silencing,
  `list-windows` once a second) or `none` (no control client, cost baseline). `MIX=idle` makes
  every window idle. Logs go to `out/<version>-<silence>[-idle]-*` (not committed).
- `reader.py`: the control client. Timestamps every line it reads; opens, pauses and kills a
  late window at t = 60, 70 and 90 s.
- `flags.sh <tmux-binary>`: which `refresh-client -f` flags a control client accepts.
- `analyze.py [out-dir]`: delays, report rates, output leaks, formats and CPU per run.

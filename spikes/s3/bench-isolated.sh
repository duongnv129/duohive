#!/usr/bin/env bash
# The view's own cost, without the 99 other noisy panes' noise: NOISY=1, the S0 noisy pane and
# a heavy pane, b, c and d on both versions. Logs to out/bench-isolated.log.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
old="${TMUX_32A:-$HOME/.local/tmux-3.2a/bin/tmux}"
new="${TMUX_LATEST:-tmux}"
for heavy in "" 1; do
  for bin in "${old}" "${new}"; do
    for c in b c d; do NOISY=1 HEAVY="${heavy}" BASE=40 VIEW=40 "${here}/run.sh" "${bin}" "${c}"; done
  done
done

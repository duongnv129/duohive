#!/usr/bin/env bash
# Runs every S3 candidate on both tmux versions, two rounds for b, c and d. Logs to out/bench.log.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
old="${TMUX_32A:-$HOME/.local/tmux-3.2a/bin/tmux}"
new="${TMUX_LATEST:-tmux}"
for bin in "${old}" "${new}"; do "${here}/run.sh" "${bin}" a; done
for r in 1 2; do
  for bin in "${old}" "${new}"; do
    for c in b c d; do RUN="${r}" "${here}/run.sh" "${bin}" "${c}"; done
  done
done

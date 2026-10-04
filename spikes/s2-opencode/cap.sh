#!/usr/bin/env bash
# cap.sh <agent> <name>: save capture-pane -p -J of the agent's pane to raw/screens/<name>.txt
set -euo pipefail
tmux -L duohive-spike capture-pane -p -J -t "spike:$1" >"${SPIKE_RAW_DIR:-/tmp/duohive-spike/raw-oc}/screens/$2.txt"
cat "${SPIKE_RAW_DIR:-/tmp/duohive-spike/raw-oc}/screens/$2.txt"

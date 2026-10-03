#!/usr/bin/env bash
# cap.sh <agent> <name>: save capture-pane -p -J of the agent's pane to raw/screens/<name>.txt
set -euo pipefail
tmux -L duohive-spike capture-pane -p -J -t "spike:$1" >"/tmp/duohive-spike/raw/screens/$2.txt"
cat "/tmp/duohive-spike/raw/screens/$2.txt"

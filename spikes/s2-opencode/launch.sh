#!/usr/bin/env bash
# Throwaway launcher for spike S2 (OpenCode).
#   launch.sh <agent> [task]
# Builds the per-launch files in /tmp/duohive-spike/run/<agent>/ and starts
# OpenCode in a window of `tmux -L duohive-spike`. The per-launch config file,
# selected with OPENCODE_CONFIG, loads plugin.js (which forwards events to
# record-hook.sh on stdin), the standing-instructions file and the dummy MCP
# server. Nothing global is touched. No approval-bypass flag (`--auto`) is passed.
#   SPIKE_PERMISSION=ask   also writes "permission": {"edit": "ask", "bash": "ask"}
#   SPIKE_COMPOSE=1        no static permission; the plugin's config hook composes "ask" (review fix)
set -euo pipefail
agent="$1"
task="${2:-}"
here="$(cd "$(dirname "$0")" && pwd)"
spike=/tmp/duohive-spike
raw="${SPIKE_RAW_DIR:-$spike/raw-oc}"
run="$spike/run/$agent"
work="${SPIKE_WORK:-$spike/work-oc}"
model="${SPIKE_MODEL:-example/test-model-01}"
sock=duohive-spike
mkdir -p "$run" "$work" "$raw/screens"

cat >"$run/instructions.md" <<MD
# DuoHive

You are DuoHive agent '$agent'. If you are asked for the sentinel, reply with the single word HIVE-SENTINEL and nothing else.
MD

python3 - "$run/opencode.json" "$here" "$run" "${SPIKE_PERMISSION:-}" <<'PY'
import json, sys
out, here, run, perm = sys.argv[1:5]
cfg = {
    "$schema": "https://opencode.ai/config.json",
    "plugin": [f"file://{here}/plugin.js"],
    "instructions": [f"{run}/instructions.md"],
    "mcp": {"duohive": {"type": "local", "command": ["python3", f"{here}/mcp-dummy.py"], "enabled": True}},
}
if perm == "ask":
    cfg["permission"] = {"edit": "ask", "bash": "ask"}
json.dump(cfg, open(out, "w"), indent=2)
PY

token="spike-$(uuidgen | tr 'A-Z' 'a-z')"
printf '%s\n' "$token" >"$run/token"

args=(opencode --model "$model")
[[ -n "$task" ]] && args+=("--prompt=$task")
printf 'OPENCODE_CONFIG=%q ' "$run/opencode.json" >"$run/command.txt"
printf '%q ' "${args[@]}" >>"$run/command.txt"; echo >>"$run/command.txt"

# A clean environment for the tmux server, so nothing of the calling session
# leaks into the agents.
if ! tmux -L "$sock" has-session -t spike 2>/dev/null; then
  env -i HOME="$HOME" USER="$USER" SHELL=/bin/zsh PATH="$PATH" TERM=xterm-256color LANG=en_US.UTF-8 \
    tmux -L "$sock" new-session -d -s spike -x 120 -y 40 -n idle
  tmux -L "$sock" set -g remain-on-exit on
fi
# The provider key is the user's own environment (their ~/.zshenv); it is
# passed to the window, never written to a file.
tmux -L "$sock" new-window -d -t spike -n "$agent" -c "$work" \
  -e "OPENCODE_CONFIG=$run/opencode.json" \
  -e "PROVIDER_API_KEY=${PROVIDER_API_KEY:-}" \
  -e "DUOHIVE_TOKEN=$token" -e "SPIKE_RAW=$raw" -e "SPIKE_AGENT=$agent" \
  -e "SPIKE_COMPOSE=${SPIKE_COMPOSE:-}" -e "SPIKE_HOOK=$here/record-hook.sh" -- "${args[@]}"
echo "started $agent"

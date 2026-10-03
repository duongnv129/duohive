#!/usr/bin/env bash
# Throwaway launcher for spike S1 (Claude Code).
#   launch.sh <agent> [task]
# Builds the per-launch files in /tmp/duohive-spike/run/<agent>/ and starts
# Claude Code in a window of `tmux -L duohive-spike`. Every hook points at
# record-hook.sh. Nothing global is touched: hooks, MCP and instructions are
# passed per launch. No approval-bypass flag is passed.
set -euo pipefail
agent="$1"
task="${2:-}"
here="$(cd "$(dirname "$0")" && pwd)"
spike=/tmp/duohive-spike
run="$spike/run/$agent"
work="${SPIKE_WORK:-$spike/work}"
model="${SPIKE_MODEL:-haiku}"
sock=duohive-spike
mkdir -p "$run" "$work"

# Per-launch hook settings. Every event Claude Code 2.1.288 knows that could map
# to a DuoHive event is registered, so the spike sees which ones fire.
python3 - "$run/settings.json" "$here/record-hook.sh" "$agent" <<'PY'
import json, shlex, sys
out, hook, agent = sys.argv[1:4]
events = ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse",
          "PermissionRequest", "PermissionDenied", "Notification", "Stop",
          "StopFailure", "SessionEnd"]
cmd = lambda e: f"{shlex.quote(hook)} {shlex.quote(agent)} {e}"
hooks = {e: [{"matcher": "", "hooks": [{"type": "command", "command": cmd(e)}]}] for e in events}
json.dump({"hooks": hooks}, open(out, "w"), indent=2)
PY

cat >"$run/mcp.json" <<JSON
{"mcpServers": {"duohive": {"type": "stdio", "command": "python3", "args": ["$here/mcp-dummy.py"]}}}
JSON

instructions="You are DuoHive agent '$agent'. If you are asked for the sentinel, reply with the single word HIVE-SENTINEL and nothing else."
token="spike-$(uuidgen | tr 'A-Z' 'a-z')"
printf '%s\n' "$token" >"$run/token"

args=(claude
  --append-system-prompt "$instructions"
  --settings "$run/settings.json"
  --mcp-config "$run/mcp.json"
  --model "$model")
[[ -n "${SPIKE_EFFORT:-}" ]] && args+=(--effort "$SPIKE_EFFORT")
[[ -n "$task" ]] && args+=("$task")
printf '%q ' "${args[@]}" >"$run/command.txt"; echo >>"$run/command.txt"

# A clean environment for the tmux server, so no CLAUDE* variable of the
# calling session leaks into the agents.
if ! tmux -L "$sock" has-session -t spike 2>/dev/null; then
  env -i HOME="$HOME" USER="$USER" SHELL=/bin/zsh PATH="$PATH" TERM=xterm-256color LANG=en_US.UTF-8 \
    tmux -L "$sock" new-session -d -s spike -x 120 -y 40 -n idle
  tmux -L "$sock" set -g remain-on-exit on
fi
tmux -L "$sock" new-window -d -t spike -n "$agent" -c "$work" \
  -e "DUOHIVE_TOKEN=$token" -e "SPIKE_RAW=$spike/raw" -- "${args[@]}"
echo "started $agent"

#!/usr/bin/env bash
# Throwaway hook recorder for spike S1. Claude Code runs it as
#   record-hook.sh <agent> <claude-hook-event>
# with the hook payload on stdin. It writes the raw payload to
# $SPIKE_RAW/hooks/<event>-<n>.json and one line to $SPIKE_RAW/hooks/index.log.
# It prints nothing: stdout from some hooks (SessionStart, UserPromptSubmit)
# is added to the model's context.
set -euo pipefail
agent="$1"
event="$2"
raw="${SPIKE_RAW:-/tmp/duohive-spike/raw}/hooks"
mkdir -p "$raw"
payload="$(cat)"
# Per-event counter; a mkdir lock keeps parallel hooks from sharing a number.
lock="$raw/.lock"
until mkdir "$lock" 2>/dev/null; do sleep 0.01; done
n=1
while [[ -e "$raw/$event-$n.json" ]]; do n=$((n + 1)); done
printf '%s\n' "$payload" >"$raw/$event-$n.json"
token_state="unset"
[[ -n "${DUOHIVE_TOKEN:-}" ]] && token_state="set"
printf '%s\t%s\t%s\t%s-%s.json\ttoken=%s\tppid=%s\n' \
  "$(date +%H:%M:%S)" "$agent" "$event" "$event" "$n" "$token_state" "$PPID" >>"$raw/index.log"
rmdir "$lock"
exit 0

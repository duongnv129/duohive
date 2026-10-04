#!/usr/bin/env bash
# merge-check.sh: how a per-launch OPENCODE_CONFIG "permission" merges with a user's
# config (review fix for S2.11). The user's config is simulated in a temporary
# XDG_CONFIG_HOME, so the real ~/.config/opencode is never read or written. No model
# call: it reads `opencode debug config` and `opencode debug agent build` only.
# Output: the resolved build-agent rules for edit, bash and "*" in each case.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
T="$(mktemp -d /tmp/duohive-spike-merge.XXXXXX)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/proj" "$T/empty/opencode" "$T/deny/opencode" "$T/agent/opencode" "$T/string/opencode"
echo '{}' >"$T/empty/opencode/opencode.json"
echo '{"permission":{"edit":"deny","bash":{"*":"allow","rm *":"deny","git push*":"ask"},"webfetch":"deny"},"agent":{"build":{"permission":{"bash":{"git commit*":"deny"}}}}}' >"$T/deny/opencode/opencode.json"
echo '{"agent":{"build":{"permission":{"edit":"allow","bash":{"git status":"allow"}}}}}' >"$T/agent/opencode/opencode.json"
echo '{"permission":"allow"}' >"$T/string/opencode/opencode.json"
echo '{"permission":{"edit":"ask","bash":"ask"}}' >"$T/static.json"
echo '{"permission":{"edit":{"*":"ask"},"bash":{"*":"ask"}}}' >"$T/object.json"
echo "{\"plugin\":[\"file://$here/plugin.js\"]}" >"$T/compose.json"

rules() {
  python3 -c '
import json, sys
d = json.load(sys.stdin)
print("   ", [(r["permission"], r["pattern"], r["action"]) for r in d["permission"] if r["permission"] in ("edit", "bash", "*")])'
}
cd "$T/proj"
for user in empty deny agent string; do
  echo "== user config: $(cat "$T/$user/opencode/opencode.json")"
  echo "  user only:"
  XDG_CONFIG_HOME="$T/$user" opencode debug agent build 2>/dev/null | rules
  echo "  + static {edit: ask, bash: ask}:"
  XDG_CONFIG_HOME="$T/$user" OPENCODE_CONFIG="$T/static.json" opencode debug agent build 2>/dev/null | rules
  echo "  + object {edit: {*: ask}, bash: {*: ask}}:"
  XDG_CONFIG_HOME="$T/$user" OPENCODE_CONFIG="$T/object.json" opencode debug agent build 2>/dev/null | rules
  echo "  + plugin config hook (compose):"
  SPIKE_RAW="$T/raw" SPIKE_COMPOSE=1 XDG_CONFIG_HOME="$T/$user" OPENCODE_CONFIG="$T/compose.json" \
    opencode debug agent build 2>/dev/null | rules
done

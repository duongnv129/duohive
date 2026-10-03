#!/usr/bin/env bash
# Throwaway: which `refresh-client -f` flags a control client accepts on <tmux-binary>.
set -euo pipefail
tmux_bin="${1:?usage: flags.sh <tmux-binary>}"
sock="duohive-s0-$$"
trap '"${tmux_bin}" -L "${sock}" kill-server 2>/dev/null || true; rm -f "/private/tmp/tmux-$(id -u)/${sock}"' EXIT
"${tmux_bin}" -L "${sock}" new-session -d -s duohive 'tail -f /dev/null'
echo "# $("${tmux_bin}" -V)"
for flag in no-output ignore-size pause-after=1 'no-output,ignore-size' 'pause-after=1,ignore-size' \
  active-pane read-only wait-exit no-detach-on-destroy '!no-output' bogus-flag; do
  # Flags are applied silently (no %error even for unknown names), so read back #{client_flags}.
  reply="$(printf 'display-message -p "#{client_flags}"\nrefresh-client -f %s\ndisplay-message -p "#{client_flags}"\n' "${flag}" |
    "${tmux_bin}" -L "${sock}" -C attach -t =duohive 2>&1 |
    grep -v '^%begin\|^%end\|^%session-changed\|^%exit\|^%output' | tr '\n' '|')"
  echo "${flag} => before|after: ${reply}"
done

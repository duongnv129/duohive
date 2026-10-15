#!/usr/bin/env bash
# Probe: how many %begin/%end blocks a control client gets for one line holding two commands.
set -euo pipefail
tmux_bin="${1:?tmux}"; sock="duohive-s3-probe-$$"
cleanup() { "${tmux_bin}" -L "${sock}" kill-server 2>/dev/null || true; rm -f "/private/tmp/tmux-$(id -u)/${sock}"; }
trap cleanup EXIT
"${tmux_bin}" -L "${sock}" -f /dev/null new -d -s duohive 'cat'
{ echo "display -p a ; display -p b"; sleep 0.5; } | "${tmux_bin}" -L "${sock}" -f /dev/null -C attach -t =duohive | cat -v

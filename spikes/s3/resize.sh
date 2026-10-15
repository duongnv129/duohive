#!/usr/bin/env bash
# Throwaway spike S3.5: does resize-window on the viewed window hold, with DuoHive's server
# options (duohive-tmux ensure_server: default-size 160x48, window-size left at its default),
# and does the CLI redraw?
# usage: resize.sh <tmux-binary> claude|opencode <work-dir>
set -euo pipefail
tmux_bin="${1:?tmux}"
cli="${2:?claude|opencode}"
work="${3:?work-dir}"
sock="duohive-s3-rs-$$"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/duohive-s3-rs.XXXXXX")"
t() { "${tmux_bin}" -L "${sock}" -f /dev/null "$@"; }
cleanup() {
  exec 3>&- 4>&- 2>/dev/null || true
  "${tmux_bin}" -L "${sock}" kill-server 2>/dev/null || true
  rm -f "/private/tmp/tmux-$(id -u)/${sock}"
  rm -rf "${tmp}"
}
trap cleanup EXIT INT TERM
case "${cli}" in
  claude) cmd='exec claude --model haiku' ;;
  opencode) cmd='exec opencode' ;;
  *) exit 2 ;;
esac

env -i HOME="${HOME}" USER="${USER}" SHELL=/bin/zsh PATH="${PATH}" TERM=xterm-256color \
  COLORTERM=truecolor LANG=en_US.UTF-8 \
  "${tmux_bin}" -L "${sock}" -f /dev/null new-session -d -s duohive -n duohive-home -x 160 -y 48 'tail -f /dev/null'
for kv in "remain-on-exit on" "default-shell /bin/sh" "default-size 160x48"; do
  # shellcheck disable=SC2086
  t set-option -g ${kv}
done
# DuoHive's main connection (TD 4.1 source 2).
mkfifo "${tmp}/main.in"
t -C attach-session -f no-output,ignore-size -t =duohive <"${tmp}/main.in" >"${tmp}/main.log" &
exec 3>"${tmp}/main.in"
echo "refresh-client -C 160x48" >&3
sleep 0.5

t new-window -d -n cli -c "${work}" "${cmd}"
sleep 8
size() { t display-message -p -t =duohive:cli '#{window_width}x#{window_height}'; }
wsz() { t show-options -wv -t =duohive:cli window-size 2>/dev/null || echo "(unset)"; }
# The CLI's drawing: the widest line and the last non-empty row of the screen.
drawn() {
  t capture-pane -p -t =duohive:cli | python3 -I -c '
import sys
rows = [r.rstrip() for r in sys.stdin.read().split("\n")]
last = max([i + 1 for i, r in enumerate(rows) if r] or [0])
print(f"widest={max(len(r) for r in rows)} last_row={last}", end="")'
}
echo "tmux $("${tmux_bin}" -V | awk '{print $2}') ${cli}"
echo "start: size=$(size) window-size(window)=$(wsz) window-size(global)=$(t show-options -gwv window-size) drawn: $(drawn)"
t resize-window -t =duohive:cli -x 100 -y 30
echo "resize 100x30: size=$(size) window-size(window)=$(wsz)"
sleep 2
echo "  after 2 s: size=$(size) drawn: $(drawn)"
echo "refresh-client -C 160x48" >&3
t new-window -d -n other 'cat'
sleep 1
echo "  after main refresh -C and a new window: size=$(size)"

# A second control client, as candidate b's viewer, with and without ignore-size.
mkfifo "${tmp}/view.in"
t -C attach-session -f no-output,ignore-size -t =duohive <"${tmp}/view.in" >"${tmp}/view.log" &
exec 4>"${tmp}/view.in"
echo "refresh-client -C 120x40" >&4
sleep 1
echo "  after a viewer client (ignore-size, -C 120x40): size=$(size)"
exec 4>&-
sleep 0.5

t resize-window -t =duohive:cli -x 80 -y 20
sleep 2
echo "resize 80x20: size=$(size) drawn: $(drawn)"
t capture-pane -p -t =duohive:cli >"${tmp}/80x20.txt"
t resize-window -t =duohive:cli -x 120 -y 36
sleep 2
echo "resize 120x36: size=$(size) drawn: $(drawn)"
echo "main errors: $(grep -c '^%error' "${tmp}/main.log" || true)"
# Quit the CLI the way DuoHive does (C-c, /exit, Enter, 150 ms apart).
t send-keys -t =duohive:cli C-c; sleep 0.15
t send-keys -t =duohive:cli -l /exit; sleep 0.15
t send-keys -t =duohive:cli Enter; sleep 2

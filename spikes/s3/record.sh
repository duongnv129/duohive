#!/usr/bin/env bash
# Throwaway spike S3.4: record a CLI's raw pane output at 80x20, with checkpoints.
# usage: record.sh <tmux-binary> claude|opencode <out-dir> <work-dir>
# The pane's output goes to <out-dir>/stream.bin from its first byte (pipe-pane -O, attached
# while the window sleeps 1 s before exec). At each checkpoint, <out-dir>/<name>/ gets the
# stream so far, capture-pane -e -p, capture-pane -p and meta.txt
# (width height cursor_x cursor_y cursor_flag alternate_on): the input of fidelity/.
set -euo pipefail
tmux_bin="${1:?tmux}"
cli="${2:?claude|opencode}"
out="${3:?out-dir}"
work="${4:?work-dir}"
sock="duohive-s3-rec-$$"
mkdir -p "${out}" "${work}"
out="$(cd "${out}" && pwd)"
t() { "${tmux_bin}" -L "${sock}" -f /dev/null "$@"; }
cleanup() {
  "${tmux_bin}" -L "${sock}" kill-server 2>/dev/null || true
  rm -f "/private/tmp/tmux-$(id -u)/${sock}"
}
trap cleanup EXIT INT TERM

case "${cli}" in
  claude) cmd='exec claude --model haiku' ;;
  opencode) cmd='exec opencode' ;;
  *) echo "bad cli" >&2; exit 2 ;;
esac

# A clean environment, as in S1 and S2, plus COLORTERM as a user's terminal sets it.
env -i HOME="${HOME}" USER="${USER}" SHELL=/bin/zsh PATH="${PATH}" TERM=xterm-256color \
  COLORTERM=truecolor LANG=en_US.UTF-8 \
  "${tmux_bin}" -L "${sock}" -f /dev/null new-session -d -s rec -n keeper -x 80 -y 20 'tail -f /dev/null'
t set-option -g remain-on-exit on
t set-option -g default-size 80x20
t new-window -d -n cli -c "${work}" "echo \"TERM=\$TERM COLORTERM=\$COLORTERM\" > '${out}/env.txt'; sleep 1; ${cmd}"
t pipe-pane -O -t =rec:cli "exec cat > '${out}/stream.bin'"

checkpoint() {
  local name="$1" dir="${out}/$1" i a b
  mkdir -p "${dir}"
  for i in 1 2 3 4 5 6 7 8 9 10; do
    sleep 0.3
    a="$(t capture-pane -e -p -t =rec:cli)"
    cp "${out}/stream.bin" "${dir}/stream.bin"
    sleep 0.05
    b="$(t capture-pane -e -p -t =rec:cli)"
    [ "${a}" = "${b}" ] && break
  done
  t capture-pane -e -p -t =rec:cli >"${dir}/capture-e.txt"
  t capture-pane -e -N -p -t =rec:cli >"${dir}/capture-eN.txt"
  t capture-pane -p -t =rec:cli >"${dir}/capture.txt"
  t display-message -p -t =rec:cli '#{pane_width} #{pane_height} #{cursor_x} #{cursor_y} #{cursor_flag} #{alternate_on}' >"${dir}/meta.txt"
  echo "checkpoint ${name}: $(cat "${dir}/meta.txt") stable-after=${i}"
}
keys() { t send-keys -t =rec:cli "$@"; }

if [ "${cli}" = claude ]; then
  sleep 8; checkpoint at-rest-start
  keys -l 'Reply with the single word OK and use no tools.'; checkpoint typed
  keys Enter; sleep 1.2; checkpoint working
  sleep 12; checkpoint at-rest-after-turn
  keys -l '/'; sleep 1; checkpoint slash-menu
  keys Escape; sleep 0.5; keys C-c; sleep 0.3; keys -l '/exit'; sleep 0.3; keys Enter
  sleep 3; checkpoint after-exit
else
  sleep 8; checkpoint at-rest-start
  keys -l 'hello from the spike'; checkpoint typed
  keys C-u; sleep 0.3; keys -l '/'; sleep 1; checkpoint slash-menu
  keys Escape; sleep 0.5; keys C-u; sleep 0.3; keys -l '/exit'; sleep 0.3; keys Enter
  sleep 3; checkpoint after-exit
fi
echo "stream: $(wc -c <"${out}/stream.bin") bytes; $(cat "${out}/env.txt")"

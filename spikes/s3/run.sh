#!/usr/bin/env bash
# Throwaway spike S3. Not used by crates/.
# usage: run.sh <tmux-binary> <candidate>
#   <candidate>: a | b | c | d   (P2-D2; see README.md)
#   RUN=2 run.sh ...             repeat a run without overwriting it
#   BASE=60 VIEW=60              seconds of the baseline phase and of the noisy-view phase
#   NOISY=1                      noisy windows (default 100): NOISY=1 isolates the view's own cost
#   HEAVY=1                      the viewed window (noisy-1) prints about 1,000 lines a second
# Starts a throwaway server (-L duohive-s3-<pid>, -f /dev/null) with S0's noisy generator in
# 100 windows, one echo window and one history window, then runs view.py.
set -euo pipefail

tmux_bin="${1:?usage: run.sh <tmux-binary> <candidate>}"
cand="${2:?candidate}"
case "${cand}" in a | b | c | d) ;; *) echo "bad candidate: ${cand}" >&2; exit 2 ;; esac

here="$(cd "$(dirname "$0")" && pwd)"
out="${here}/out"
mkdir -p "${out}"
version="$("${tmux_bin}" -V | awk '{print $2}')"
tag="${version}-${cand}"
noisy_n="${NOISY:-100}"
[ "${noisy_n}" = 100 ] || tag="${tag}-n${noisy_n}"
[ -z "${HEAVY:-}" ] || tag="${tag}-heavy"
[ -z "${RUN:-}" ] || tag="${tag}-r${RUN}"
sock="duohive-s3-$$"
run_dir="$(mktemp -d "${TMPDIR:-/tmp}/duohive-s3.XXXXXX")"
t() { "${tmux_bin}" -L "${sock}" -f /dev/null "$@"; }

cleanup() {
  "${tmux_bin}" -L "${sock}" kill-server 2>/dev/null || true
  # tmux 3.2a leaves its socket file behind after kill-server.
  rm -f "${TMUX_TMPDIR:-/tmp}/tmux-$(id -u)/${sock}" "/private/tmp/tmux-$(id -u)/${sock}"
  rm -rf "${run_dir}"
}
trap cleanup EXIT INT TERM

# S0's noisy window.
noisy='while :; do date; sleep 0.1; done'
t new-session -d -s duohive -n keeper -x 80 -y 24 'tail -f /dev/null'
t set-option -g remain-on-exit on
heavy='while :; do seq -f "heavy line %g" 1 100; sleep 0.1; done'
for i in $(seq 1 "${noisy_n}"); do
  if [ "${i}" = 1 ] && [ -n "${HEAVY:-}" ]; then
    t new-window -d -n "noisy-${i}" "${heavy}"
  else
    t new-window -d -n "noisy-${i}" "${noisy}"
  fi
done
# Echo window: the tty echoes each key; cat discards it, so only the echo reaches the screen.
t new-window -d -n echo 'stty -icanon; exec cat >/dev/null'
# History window for S3.6: 3,000 coloured lines, then idle.
t new-window -d -n hist "i=0; while [ \$i -lt 3000 ]; do i=\$((i+1)); printf '\\033[3%dmline %05d \\033[1m%s\\033[0m\\n' \$((i % 8)) \$i 'the quick brown fox jumps over the lazy dog 0123456789'; done; exec cat"
sleep 2
t list-windows -t =duohive -F '#{window_index} #{window_id} #{pane_id} #{window_name}' >"${out}/${tag}-windows.txt"
server_pid="$(t display-message -p '#{pid}')"
echo_pane="$(awk '$4 == "echo" {print $3}' "${out}/${tag}-windows.txt")"
noisy_pane="$(awk '$4 == "noisy-1" {print $3}' "${out}/${tag}-windows.txt")"
hist_pane="$(awk '$4 == "hist" {print $3}' "${out}/${tag}-windows.txt")"

python3 -I "${here}/view.py" "${tmux_bin}" "${sock}" "${cand}" "${server_pid}" \
  "${echo_pane}" "${noisy_pane}" "${hist_pane}" "${run_dir}" "${out}/${tag}" "${BASE:-60}" "${VIEW:-60}"
echo "done: ${tag}"

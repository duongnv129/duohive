#!/usr/bin/env bash
# Throwaway spike S0. Not used by crates/.
# usage: run.sh <tmux-binary> <windows> <seconds> <silence>
#   <silence>: no-output | pause   (subscribe, with that silencing method)
#              poll                (no-output silencing, list-windows once a second, no subscriptions)
#              none                (no control client at all: cost baseline)
#   MIX=idle run.sh ...            every window in the mix runs `cat` (idle cost runs)
#   MIX=noisy run.sh ...           every window in the mix is noisy (noisy cost runs)
set -euo pipefail

tmux_bin="${1:?usage: run.sh <tmux-binary> <windows> <seconds> <silence>}"
windows="${2:?windows}"
seconds="${3:?seconds}"
silence="${4:?silence}"
mix="${MIX:-mix}"
case "${silence}" in no-output | pause | poll | none) ;; *) echo "bad silence: ${silence}" >&2; exit 2 ;; esac

here="$(cd "$(dirname "$0")" && pwd)"
out="${here}/out"
mkdir -p "${out}"
version="$("${tmux_bin}" -V | awk '{print $2}')"
tag="${version}-${silence}"
[ "${mix}" = "mix" ] || tag="${tag}-${mix}"
[ -z "${RUN:-}" ] || tag="${tag}-r${RUN}"   # RUN=2 ...: repeat a run without overwriting it
sock="duohive-s0-$$"
t() { "${tmux_bin}" -L "${sock}" "$@"; }

sampler_pid=""
reader_pid=""
cleanup() {
  [ -n "${sampler_pid}" ] && kill "${sampler_pid}" 2>/dev/null || true
  [ -n "${reader_pid}" ] && kill "${reader_pid}" 2>/dev/null || true
  "${tmux_bin}" -L "${sock}" kill-server 2>/dev/null || true
  # tmux 3.2a leaves its socket file behind after kill-server.
  rm -f "${TMUX_TMPDIR:-/tmp}/tmux-$(id -u)/${sock}" "/private/tmp/tmux-$(id -u)/${sock}"
}
trap cleanup EXIT INT TERM

notif="${out}/${tag}-notifications.log"
cpu="${out}/${tag}-cpu.csv"
bursts="${out}/${tag}-bursts.log"
wins="${out}/${tag}-windows.txt"
pids="${out}/${tag}-pids.txt"
: >"${bursts}"
: >"${pids}"

noisy='while :; do date; sleep 0.1; done'
# 10 s of output every 30 s, first burst after a per-window offset; logs each burst start (ms).
bursty() {
  local idx="$1" offset="$2"
  printf '%s' "sleep ${offset}; while :; do perl -MTime::HiRes=time -e 'printf \"%s %.3f\\n\", \$ARGV[0], time' ${idx} >>'${bursts}'; i=0; while [ \$i -lt 100 ]; do date; sleep 0.1; i=\$((i+1)); done; sleep 20; done"
}

t new-session -d -s duohive -n keeper -x 80 -y 24 'tail -f /dev/null'
t set-option -g remain-on-exit on
for i in $(seq 1 "${windows}"); do
  if [ "${mix}" = "idle" ] || [ "${mix}" = "noisy" ]; then
    kind="${mix}"
  else
    case $((i % 3)) in 1) kind=noisy ;; 2) kind=bursty ;; 0) kind=idle ;; esac
  fi
  case "${kind}" in
    noisy) cmd="${noisy}" ;;
    bursty) cmd="$(bursty "${i}" $(((i * 7) % 30 + 8)))" ;;
    idle) cmd='cat' ;;
  esac
  t new-window -d -n "${kind}-${i}" "${cmd}"
done
t new-window -d -n dead "sh -c 'sleep 20; exit 3'"
t list-windows -t =duohive -F '#{window_index} #{window_id} #{pane_id} #{window_name}' >"${wins}"
server_pid="$(t display-message -p '#{pid}')"
echo "server ${server_pid}" >>"${pids}"

if [ "${silence}" != "none" ]; then
  python3 "${here}/reader.py" "${tmux_bin}" "${sock}" "${silence}" "${seconds}" "${notif}" "${pids}" &
  reader_pid=$!
else
  : >"${notif}"
fi

# Sample every second: t, role, pid, %cpu, rss KiB, cumulative cpu time.
(
  echo "t,role,pid,pcpu,rss_kib,cputime"
  while :; do
    now="$(perl -MTime::HiRes=time -e 'printf "%.3f", time')"
    while read -r role pid; do
      line="$(ps -o %cpu=,rss=,time= -p "${pid}" 2>/dev/null || true)"
      [ -n "${line}" ] && echo "${now},${role},${pid},$(echo "${line}" | awk '{print $1","$2","$3}')"
    done <"${pids}"
    sleep 1
  done
) >"${cpu}" &
sampler_pid=$!

if [ -n "${reader_pid}" ]; then
  wait "${reader_pid}" || true
  reader_pid=""
else
  sleep "${seconds}"
fi
echo "done: ${tag} (windows=${windows} seconds=${seconds})"

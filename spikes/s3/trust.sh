#!/usr/bin/env bash
# Throwaway spike S3.7: does Claude Code show its workspace-trust dialog in a new worktree path?
# usage: trust.sh <trusted-git-repo> <out-dir>
# Adds a detached worktree of <trusted-git-repo> (a repo Claude Code already trusts) at a new
# path shaped like TD 5.3's (.../duohive/worktrees/<agent>/<task-id>), starts claude there in a
# throwaway tmux server, saves the screen, and kills the server WITHOUT answering the dialog.
# The worktree is removed afterwards. A fresh non-git directory is the control.
set -euo pipefail
repo="${1:?trusted-git-repo}"
out="${2:?out-dir}"
mkdir -p "${out}"
base="$(mktemp -d /private/tmp/duohive-s3-trust.XXXXXX)"
wt="${base}/duohive/worktrees/a1/t1"
plain="${base}/plain"
sock="duohive-s3-trust-$$"
t() { tmux -L "${sock}" -f /dev/null "$@"; }
cleanup() {
  tmux -L "${sock}" kill-server 2>/dev/null || true
  rm -f "/private/tmp/tmux-$(id -u)/${sock}"
  git -C "${repo}" worktree remove --force "${wt}" 2>/dev/null || true
  git -C "${repo}" worktree prune
  rm -rf "${base}"
}
trap cleanup EXIT INT TERM

trusted() {
  python3 -I -c '
import json, os, sys
p = json.load(open(os.path.expanduser("~/.claude.json"))).get("projects", {})
for d in sys.argv[1:]:
    print(d, "->", p.get(d, {}).get("hasTrustDialogAccepted", "(no entry)"))
' "$@"
}

mkdir -p "$(dirname "${wt}")" "${plain}"
git -C "${repo}" worktree add --detach "${wt}" HEAD >/dev/null 2>&1
echo "claude $(claude --version)"
echo "before:"; trusted "${repo}" "${wt}" "${plain}"
env -i HOME="${HOME}" USER="${USER}" SHELL=/bin/zsh PATH="${PATH}" TERM=xterm-256color LANG=en_US.UTF-8 \
  tmux -L "${sock}" -f /dev/null new-session -d -s trust -n keeper -x 100 -y 30 'tail -f /dev/null'
t new-window -d -n wt -c "${wt}" 'exec claude --model haiku'
t new-window -d -n plain -c "${plain}" 'exec claude --model haiku'
sleep 8
for w in wt plain; do
  t capture-pane -p -t "=trust:${w}" >"${out}/trust-${w}.txt"
  if grep -q 'trust' "${out}/trust-${w}.txt"; then shown=yes; else shown=no; fi
  echo "${w}: trust dialog shown: ${shown}: $(grep -m1 -E 'safety check|trust' "${out}/trust-${w}.txt" | sed 's/^ *//' || true)"
done
tmux -L "${sock}" kill-server
sleep 1
echo "after (not answered):"; trusted "${repo}" "${wt}" "${plain}"

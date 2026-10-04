#!/usr/bin/env bash
# keys.sh <agent> <tmux send-keys args...>
a="$1"; shift
exec tmux -L duohive-spike send-keys -t "spike:$a" "$@"

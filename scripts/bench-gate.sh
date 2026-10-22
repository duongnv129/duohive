#!/bin/sh
# The PRD gate: three counted runs at 10 and 100 agents, each holding the live
# view open, then one informational run of each without it. A run the bench
# discards for machine load (exit 3) is retried twice; if it is still discarded,
# the script exits 3. The verdict is the worst value per target over the counted
# runs. Exits 0 when every target passes, 1 when one misses.
#
#   DUOHIVE_BIN=target/release/duohive
#   make bench-gate
set -u

BIN=${DUOHIVE_BIN:-target/release/duohive}
RUNS=3
RETRIES=2

tmp=${TMPDIR:-/tmp}/duohive-gate.$$
mkdir -p "$tmp" || exit 1
trap 'rm -rf "$tmp"' EXIT INT TERM

# Prints the value of target `$2` in the JSON file `$1`.
target_value() {
    sed -n "s/.*\"$2\":{\"pass\":[^}]*\"value\":\([^,}]*\)}.*/\1/p" "$1"
}

# Prints `true` or `false`: whether target `$2` passed in the file `$1`.
target_pass() {
    sed -n "s/.*\"$2\":{\"pass\":\([a-z]*\),.*/\1/p" "$1"
}

# Prints target `$2`'s goal in the file `$1`.
target_goal() {
    sed -n "s/.*\"$2\":{\"pass\":[^,]*,\"target\":\([^,]*\),.*/\1/p" "$1"
}

# One counted run: `$1` agents, writing the JSON to `$2`. Exits 0 on a run,
# 3 when the bench kept discarding it for load, 1 on any other failure.
counted_run() {
    attempt=0
    while :; do
        "$BIN" bench --agents "$1" --view --json >"$2" 2>"$2.err"
        code=$?
        if [ "$code" -eq 0 ]; then
            return 0
        fi
        if [ "$code" -ne 3 ]; then
            echo "bench-gate: agents $1: bench exited $code" >&2
            cat "$2.err" >&2
            return 1
        fi
        attempt=$((attempt + 1))
        if [ "$attempt" -gt "$RETRIES" ]; then
            echo "bench-gate: agents $1: discarded for load $attempt times" >&2
            return 3
        fi
        echo "bench-gate: agents $1: discarded for load, retrying" >&2
    done
}

# The worst (largest) value of target `$2` over the counted files of `$1`.
worst_value() {
    for file in "$tmp/$1-run-"*.json; do
        target_value "$file" "$2"
    done | awk 'BEGIN { worst = "" } $1 != "null" { if (worst == "" || $1 + 0 > worst + 0) worst = $1 } END { if (worst == "") print "n/a"; else printf "%.2f\n", worst }'
}

# Whether target `$2` passed in every counted run of `$1`.
all_passed() {
    for file in "$tmp/$1-run-"*.json; do
        target_pass "$file" "$2"
    done | grep -q 'false' && return 1
    return 0
}

# Target `$2`'s goal, from the first counted run of `$1`.
goal_of() {
    target_goal "$tmp/$1-run-1.json" "$2"
}

label() {
    case $1 in
        idle_cpu) echo "idle CPU" ;;
        memory) echo "memory" ;;
        hook) echo "hook -> screen" ;;
        tilde) echo "screen-read (~)" ;;
        first_frame) echo "first frame" ;;
    esac
}

failed=1
for agents in 10 100; do
    run=1
    while [ "$run" -le "$RUNS" ]; do
        counted_run "$agents" "$tmp/$agents-run-$run.json"
        case $? in
            0) ;;
            3) exit 3 ;;
            *) failed=0; break ;;
        esac
        run=$((run + 1))
    done
    [ "$failed" -eq 1 ] || exit 1

    # The same machine, without the live view: information only.
    "$BIN" bench --agents "$agents" --json >"$tmp/$agents-info.json" 2>/dev/null || true
done

status=0
for agents in 10 100; do
    echo "$agents agents"
    for target in idle_cpu memory hook tilde first_frame; do
        worst=$(worst_value "$agents" "$target")
        goal=$(goal_of "$agents" "$target")
        if all_passed "$agents" "$target"; then
            verdict=PASS
        else
            verdict=FAIL
            status=1
        fi
        printf '  %-14s %8s   target %s   %s\n' "$(label "$target")" "$worst" "$goal" "$verdict"
    done
    info=$(sed -n 's/.*"all_pass":\([a-z]*\).*/\1/p' "$tmp/$agents-info.json" 2>/dev/null)
    echo "  (no live view: all_pass ${info:-unknown}, information only)"
done

exit "$status"

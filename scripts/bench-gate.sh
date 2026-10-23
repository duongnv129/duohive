#!/bin/sh
# The PRD gate. The method lives in the binary: `duohive bench --gate` runs
# three counted runs with the live view at 10 and at 100 agents (a run the
# bench discards for machine load is retried twice), takes the worst value per
# target, then one informational run of each without the view. This script only
# finds the binary and passes its arguments on.
#
# Exit 0 every target met, 1 a target missed (or any other failure), 3 discarded
# for load, 130 interrupted.
#
#   DUOHIVE_BIN=target/release/duohive
#   make bench-gate
exec "${DUOHIVE_BIN:-target/release/duohive}" bench --gate "$@"

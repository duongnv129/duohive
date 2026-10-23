#!/bin/sh
# Release check (P5-D1, P5-D7): the tag is `v` plus the workspace version, and
# `docs/release-gates/<version>.json` is a passing gate run made on macOS arm64
# with a binary under the size limit, on code that the tagged commit still has
# (the gate's git_sha is the tagged commit, or differs from it only by commits
# that touch nothing outside docs/). Needs only sh, sed, tr, git.
#
#   scripts/check-release-version.sh v1.0.0
#
# RELEASE_CHECK_ROOT overrides the repository root (the tests use it).
set -u

ROOT=${RELEASE_CHECK_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}
LIMIT=10000000

fail() {
    echo "check-release-version: $1" >&2
    exit 1
}

[ $# -eq 1 ] || fail "usage: check-release-version.sh <tag>"
tag=$1

version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)
[ -n "$version" ] || fail "no workspace version in Cargo.toml"
[ "$tag" = "v$version" ] || fail "tag $tag does not match the workspace version $version"

file="$ROOT/docs/release-gates/$version.json"
[ -f "$file" ] || fail "no gate result at docs/release-gates/$version.json"
# One line, no spaces: the gate writes compact JSON, a person may pretty-print it.
flat=$(tr -d ' \n\t' <"$file")

# serde_json sorts keys, so the top-level all_pass is the first key.
all_pass=$(printf '%s' "$flat" | sed -n 's/^{"all_pass":\([a-z]*\),.*/\1/p')
[ "$all_pass" = "true" ] || fail "the gate run did not pass (all_pass is '${all_pass:-missing}')"

platform=$(printf '%s' "$flat" | sed -n 's/.*"platform":"\([^"]*\)".*/\1/p')
[ "$platform" = "macos-aarch64" ] || fail "the gate ran on '${platform:-unknown}', not macos-aarch64"

bytes=$(printf '%s' "$flat" | sed -n 's/.*"binary_bytes":\([0-9]*\).*/\1/p')
[ -n "$bytes" ] || fail "the gate result has no binary_bytes"
[ "$bytes" -lt "$LIMIT" ] || fail "binary is $bytes B, not under $LIMIT B"

dirty=$(printf '%s' "$flat" | sed -n 's/.*"git_dirty":\([a-z]*\).*/\1/p')
[ "$dirty" = "false" ] || fail "the gate ran on a checkout with uncommitted changes (git_dirty is '${dirty:-missing}')"

sha=$(printf '%s' "$flat" | sed -n 's/.*"git_sha":"\([0-9a-f]*\)".*/\1/p')
[ -n "$sha" ] || fail "the gate result has no git_sha"
head=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null) || fail "not a git checkout"
if [ "$sha" != "$head" ]; then
    git -C "$ROOT" cat-file -e "$sha^{commit}" 2>/dev/null || fail "gate commit $sha is not in this history"
    changed=$(git -C "$ROOT" diff --name-only "$sha" "$head" | grep -v '^docs/' || true)
    [ -z "$changed" ] || fail "code changed since the gated commit $sha: $(echo "$changed" | head -n 3 | tr '\n' ' ')"
fi

echo "check-release-version: $tag ok (gate $sha, $bytes B)"

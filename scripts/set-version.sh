#!/bin/sh
# Sets the release version in one place (P5-D7): [workspace.package].version
# and the `version = "…"` pin of every duohive-* path dependency in
# [workspace.dependencies]. The lock file is left alone; `cargo build` updates
# it in the release commit.
#
#   scripts/set-version.sh 1.0.0-rc.1
#
# SET_VERSION_ROOT overrides the repository root (the tests use it).
set -eu

ROOT=${SET_VERSION_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}
FILE="$ROOT/Cargo.toml"

[ $# -eq 1 ] || { echo "usage: set-version.sh <version>" >&2; exit 2; }
version=$1
if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.+-]+)?$'; then
    echo "set-version: '$version' is not a semantic version" >&2
    exit 2
fi

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT INT TERM
# The first `version = "…"` after [workspace.package], and every duohive-* pin.
awk -v v="$version" '
    /^\[/ { in_pkg = ($0 == "[workspace.package]") }
    in_pkg && !done && /^version *= *"/ { print "version = \"" v "\""; done = 1; next }
    /^duohive-[a-z]+ *= *\{ *path *=/ { sub(/version = "[^"]*"/, "version = \"" v "\"") }
    { print }
' "$FILE" >"$tmp"
# Every pin and the package version must have moved; if the manifest has another
# shape, change nothing and say so.
pins=$(grep -c '^duohive-[a-z]* *= *{ *path' "$tmp" || true)
moved=$(grep '^duohive-[a-z]* *= *{ *path' "$tmp" | grep -c "version = \"$version\"" || true)
pkg=$(sed -n '/^\[workspace.package\]/,/^\[/p' "$tmp" | grep -c "^version *= *\"$version\"" || true)
if [ "$pins" -eq 0 ] || [ "$pins" -ne "$moved" ] || [ "$pkg" -ne 1 ]; then
    echo "set-version: Cargo.toml has an unexpected shape ($moved of $pins pins, package version set $pkg times); nothing changed" >&2
    exit 1
fi
cat "$tmp" >"$FILE"
echo "set-version: $version"

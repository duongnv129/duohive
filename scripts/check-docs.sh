#!/bin/sh
# Docs check, run by CI: the documents a newcomer reads stay true.
#   1. Every relative link in the core docs points at a file that exists.
#   2. CHANGELOG.md has an entry for the workspace version.
#   3. README.md names every release target that release-binaries.yml builds.
# Needs only sh, grep, sed and mktemp.
#
#   scripts/check-docs.sh
#
# CHECK_DOCS_ROOT overrides the repository root (the tests use it).
set -u

ROOT=${CHECK_DOCS_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}
cd "$ROOT" || exit 2

problems=$(mktemp)
trap 'rm -f "$problems"' EXIT INT TERM

FILES="README.md CONTRIBUTING.md SECURITY.md CHANGELOG.md docs/README.md docs/release-runbook.md docs/plans/status.md docs/plans/v1-roadmap.md"

for f in $FILES; do
    [ -f "$f" ] || continue
    dir=$(dirname "$f")
    grep -o '](\([^)]*\))' "$f" | sed 's/^](//; s/)$//' | while IFS= read -r target; do
        case "$target" in
            http://* | https://* | mailto:* | '#'* | '') continue ;;
        esac
        path=${target%%#*}
        path=${path%% *}
        [ -e "$dir/$path" ] || echo "$f: broken link ($target)" >>"$problems"
    done
done

version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n 1)
if [ -z "$version" ]; then
    echo "Cargo.toml: no workspace version" >>"$problems"
elif ! grep -qF "## [$version]" CHANGELOG.md; then
    echo "CHANGELOG.md: no '## [$version]' entry for the workspace version" >>"$problems"
fi

workflow=.github/workflows/release-binaries.yml
if [ -f "$workflow" ]; then
    for t in $(sed -n 's/^ *- target: *//p' "$workflow"); do
        grep -qF "$t" README.md || echo "README.md: does not list the release target $t" >>"$problems"
    done
fi

if [ -s "$problems" ]; then
    sed 's/^/check-docs: /' "$problems" >&2
    exit 1
fi
echo "check-docs: ok (version $version)"

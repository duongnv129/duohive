#!/bin/sh
# Build DuoHive in release mode and install it on this machine.
#
#   ./install.sh                  install to ~/.local/bin (duohive + dh alias)
#   ./install.sh --prefix /usr/local
#   ./install.sh --no-alias       skip the `dh` alias
#   ./install.sh --uninstall      remove what this script installed
set -eu

prefix="${PREFIX:-$HOME/.local}"
alias_name="dh"
with_alias=1
uninstall=0

usage() {
    sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix) [ $# -ge 2 ] || { usage >&2; exit 2; }; prefix="$2"; shift 2 ;;
        --prefix=*) prefix="${1#--prefix=}"; shift ;;
        --no-alias) with_alias=0; shift ;;
        --uninstall) uninstall=1; shift ;;
        -h | --help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done

bin_dir="$prefix/bin"
target="$bin_dir/duohive"
link="$bin_dir/$alias_name"

# The alias is ours only when it is a symlink to duohive.
alias_is_ours() {
    [ -L "$link" ] && [ "$(readlink "$link")" = "duohive" ]
}

if [ "$uninstall" -eq 1 ]; then
    removed=0
    if [ -e "$target" ]; then rm -f "$target"; echo "removed $target"; removed=1; fi
    if alias_is_ours; then rm -f "$link"; echo "removed $link"; removed=1; fi
    [ "$removed" -eq 1 ] || echo "nothing to remove in $bin_dir"
    echo "your org, settings and agents' data are kept (~/.config/duohive, ~/.local/share/duohive)"
    exit 0
fi

command -v cargo >/dev/null 2>&1 || {
    echo "cargo not found · install Rust from https://rustup.rs" >&2
    exit 1
}

# tmux 3.2 or newer is needed at run time, not to build; warn only.
if command -v tmux >/dev/null 2>&1; then
    version="$(tmux -V | awk '{print $2}')"
    major="${version%%.*}"
    minor="$(printf '%s' "${version#*.}" | tr -cd '0-9' | cut -c1-2)"
    if [ "${major:-0}" -lt 3 ] || { [ "${major:-0}" -eq 3 ] && [ "${minor:-0}" -lt 2 ]; }; then
        echo "warning: tmux $version found · DuoHive needs tmux 3.2 or newer" >&2
    fi
else
    echo "warning: tmux not found · DuoHive needs tmux 3.2 or newer to run" >&2
fi

repo="$(cd "$(dirname "$0")" && pwd)"
echo "building duohive (release)…"
cargo build --release --locked -p duohive --manifest-path "$repo/Cargo.toml"

mkdir -p "$bin_dir"
install -m 0755 "$repo/target/release/duohive" "$target"
echo "installed $target"

if [ "$with_alias" -eq 1 ]; then
    if [ -e "$link" ] && ! alias_is_ours; then
        echo "skipped the $alias_name alias: $link already exists and isn't DuoHive's" >&2
    else
        ln -sf duohive "$link"
        echo "installed $link → duohive"
    fi
fi

case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) echo "note: $bin_dir is not on your PATH · add: export PATH=\"$bin_dir:\$PATH\"" ;;
esac

"$target" --help >/dev/null 2>&1 || {
    echo "the installed binary didn't run · check $target" >&2
    exit 1
}
echo "done · run: duohive"

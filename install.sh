#!/bin/sh
# Install DuoHive's dependencies, build it in release mode and install it.
#
#   ./install.sh                  install deps, then duohive + dh alias to ~/.local/bin
#   ./install.sh --prefix /usr/local
#   ./install.sh --no-alias       skip the `dh` alias
#   ./install.sh --deps-only      only install the dependencies (tmux, Rust)
#   ./install.sh --no-deps        don't install dependencies, only check them
#   ./install.sh --uninstall      remove what this script installed (not the deps)
set -eu

prefix="${PREFIX:-$HOME/.local}"
alias_name="dh"
with_alias=1
uninstall=0
install_deps=1
deps_only=0
tmux_min_major=3
tmux_min_minor=2

usage() {
    sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix) [ $# -ge 2 ] || { usage >&2; exit 2; }; prefix="$2"; shift 2 ;;
        --prefix=*) prefix="${1#--prefix=}"; shift ;;
        --no-alias) with_alias=0; shift ;;
        --no-deps) install_deps=0; shift ;;
        --deps-only) deps_only=1; shift ;;
        --uninstall) uninstall=1; shift ;;
        -h | --help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done

if [ "$install_deps" -eq 0 ] && [ "$deps_only" -eq 1 ]; then
    echo "--no-deps and --deps-only can't be used together" >&2
    exit 2
fi

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
    echo "tmux and Rust are kept; remove them with your package manager and rustup if you want"
    exit 0
fi

# --- package manager ------------------------------------------------------

# Run as root directly, otherwise through sudo when there is one.
as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    elif command -v sudo >/dev/null 2>&1; then
        sudo "$@"
    else
        echo "need root to run: $* · run as root or install sudo" >&2
        return 1
    fi
}

# Install or upgrade the named packages with the first package manager found.
pkg_install() {
    if command -v brew >/dev/null 2>&1; then
        for pkg in "$@"; do
            if brew list --formula "$pkg" >/dev/null 2>&1; then
                brew upgrade "$pkg" || true
            else
                brew install "$pkg"
            fi
        done
    elif command -v apt-get >/dev/null 2>&1; then
        as_root apt-get update && as_root apt-get install -y "$@"
    elif command -v dnf >/dev/null 2>&1; then
        as_root dnf install -y "$@"
    elif command -v yum >/dev/null 2>&1; then
        as_root yum install -y "$@"
    elif command -v pacman >/dev/null 2>&1; then
        as_root pacman -S --needed --noconfirm "$@"
    elif command -v zypper >/dev/null 2>&1; then
        as_root zypper --non-interactive install "$@"
    elif command -v apk >/dev/null 2>&1; then
        as_root apk add "$@"
    else
        if [ "$(uname -s)" = "Darwin" ]; then
            echo "Homebrew not found · install it from https://brew.sh, then rerun" >&2
        else
            echo "no supported package manager found (apt, dnf, yum, pacman, zypper, apk)" >&2
        fi
        return 1
    fi
}

# --- tmux -----------------------------------------------------------------

# Prints the installed tmux version, e.g. 3.4 or 3.2a; nothing when absent.
tmux_version() {
    command -v tmux >/dev/null 2>&1 || return 0
    tmux -V 2>/dev/null | awk '{print $2}'
}

# True when the installed tmux is new enough. Builds such as `master` that
# carry no number are taken as new enough.
tmux_ok() {
    version="$(tmux_version)"
    [ -n "$version" ] || return 1
    numeric="$(printf '%s' "$version" | sed 's/^[^0-9]*//')"
    [ -n "$numeric" ] || return 0
    major="$(printf '%s' "$numeric" | cut -d. -f1 | tr -cd '0-9')"
    minor="$(printf '%s' "$numeric" | cut -s -d. -f2 | tr -cd '0-9' | cut -c1-2)"
    major="${major:-0}"
    minor="${minor:-0}"
    [ "$major" -gt "$tmux_min_major" ] ||
        { [ "$major" -eq "$tmux_min_major" ] && [ "$minor" -ge "$tmux_min_minor" ]; }
}

ensure_tmux() {
    need="tmux $tmux_min_major.$tmux_min_minor or newer"
    if tmux_ok; then
        echo "tmux $(tmux_version) found"
        return 0
    fi
    found="$(tmux_version)"
    if [ "$install_deps" -eq 0 ]; then
        if [ -n "$found" ]; then
            echo "warning: tmux $found found · DuoHive needs $need" >&2
        else
            echo "warning: tmux not found · DuoHive needs $need to run" >&2
        fi
        return 0
    fi
    if [ -n "$found" ]; then
        echo "tmux $found is too old · upgrading…"
    else
        echo "installing tmux…"
    fi
    pkg_install tmux || {
        echo "couldn't install tmux · install $need yourself, then rerun" >&2
        return 1
    }
    hash -r 2>/dev/null || true
    if tmux_ok; then
        echo "tmux $(tmux_version) installed"
    else
        echo "warning: your package manager has tmux ${found:-$(tmux_version)}; DuoHive needs $need" >&2
        echo "  build a newer one: .github/scripts/install-tmux.sh $(cat "$repo/.github/tmux-latest" 2>/dev/null || echo 3.7c)" >&2
        echo "  then put \$HOME/.local/tmux-<version>/bin first on your PATH" >&2
    fi
}

# --- Rust -----------------------------------------------------------------

cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"

ensure_rust() {
    if command -v cargo >/dev/null 2>&1; then
        echo "cargo found ($(cargo --version 2>/dev/null || echo unknown))"
        return 0
    fi
    # rustup may be installed but not on this shell's PATH yet.
    if [ -x "$cargo_bin/cargo" ]; then
        PATH="$cargo_bin:$PATH"
        export PATH
        echo "cargo found in $cargo_bin"
        return 0
    fi
    if [ "$install_deps" -eq 0 ]; then
        echo "cargo not found · install Rust from https://rustup.rs" >&2
        return 1
    fi
    if ! command -v curl >/dev/null 2>&1; then
        echo "installing curl…"
        pkg_install curl || { echo "couldn't install curl · install Rust from https://rustup.rs" >&2; return 1; }
    fi
    # rust-toolchain.toml picks the exact toolchain on the first cargo call,
    # so rustup installs none here. Shell profiles are left alone.
    echo "installing Rust (rustup)…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
        sh -s -- -y --no-modify-path --profile minimal --default-toolchain none
    PATH="$cargo_bin:$PATH"
    export PATH
    command -v cargo >/dev/null 2>&1 || { echo "rustup installed but cargo isn't in $cargo_bin" >&2; return 1; }
    echo "Rust installed in $cargo_bin"
    case ":${ORIGINAL_PATH:-}:" in
        *":$cargo_bin:"*) ;;
        *) echo "note: $cargo_bin is not on your PATH · add: export PATH=\"$cargo_bin:\$PATH\"" ;;
    esac
}

# --- agent CLIs -----------------------------------------------------------

# DuoHive drives Claude Code or OpenCode; which one is the user's choice, so
# only report.
check_agent_clis() {
    if command -v claude >/dev/null 2>&1 || command -v opencode >/dev/null 2>&1; then
        return 0
    fi
    echo "note: neither claude (Claude Code) nor opencode found · install at least one to run agents"
}

ORIGINAL_PATH="$PATH"
repo="$(cd "$(dirname "$0")" && pwd)"

ensure_tmux
ensure_rust
check_agent_clis

if [ "$deps_only" -eq 1 ]; then
    echo "dependencies done"
    exit 0
fi

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

case ":$ORIGINAL_PATH:" in
    *":$bin_dir:"*) ;;
    *) echo "note: $bin_dir is not on your PATH · add: export PATH=\"$bin_dir:\$PATH\"" ;;
esac

"$target" --help >/dev/null 2>&1 || {
    echo "the installed binary didn't run · check $target" >&2
    exit 1
}
echo "done · run: duohive"

#!/usr/bin/env bash
# Install tmux <version> from source into $HOME/.local/tmux-<version>.
set -euo pipefail

version="${1:?usage: install-tmux.sh <version>}"
prefix="${HOME}/.local/tmux-${version}"

case "$(uname -s)" in
  Linux)
    sudo apt-get update
    sudo apt-get install -y libevent-dev libncurses-dev bison pkg-config
    ;;
  Darwin)
    brew install libevent ncurses pkg-config bison
    # These formulae are keg-only: expose them to configure and make.
    for formula in libevent ncurses bison; do
      keg="$(brew --prefix "${formula}")"
      export PATH="${keg}/bin:${PATH}"
      export PKG_CONFIG_PATH="${keg}/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
    done
    ;;
esac

if [ ! -x "${prefix}/bin/tmux" ]; then
  workdir="$(mktemp -d)"
  trap 'rm -rf "${workdir}"' EXIT
  curl -fsSL "https://github.com/tmux/tmux/releases/download/${version}/tmux-${version}.tar.gz" \
    | tar -xz -C "${workdir}"
  cd "${workdir}/tmux-${version}"
  ./configure --prefix="${prefix}"
  make -j"$(getconf _NPROCESSORS_ONLN)"
  make install
fi

echo "${prefix}/bin" >> "${GITHUB_PATH:-/dev/null}"

actual="$("${prefix}/bin/tmux" -V)"
echo "${actual}"
if [ "${actual}" != "tmux ${version}" ]; then
  echo "expected 'tmux ${version}', got '${actual}'" >&2
  exit 1
fi

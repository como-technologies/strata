#!/usr/bin/env bash
# Install the pinned OpenTofu for your user, and test its checksum. The live example runs `tofu`.
#
# The script puts the one binary in BIN_DIR (default ~/.local/bin), so the install needs no root.
# It does nothing when that version is already there. `tofu-install.sh --uninstall` removes the
# binary that this script installed.
#
#   TOFU_VERSION=1.12.6 examples/live/tofu-install.sh
#   examples/live/tofu-install.sh --uninstall
set -euo pipefail

version=${TOFU_VERSION:?set TOFU_VERSION}
bin=${BIN_DIR:-$HOME/.local/bin}
cache=${CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/strata/tofu}
dest=$bin/tofu

live=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
script=tofu-install
. "$live/common.sh"
note() { echo "tofu-install: $*" >&2; }

# installed: the version of the binary at $dest, or nothing. `tofu version` prints
# "OpenTofu v1.12.6" on its first line.
installed() {
  [ -x "$dest" ] || return 0
  # the answer is read whole first: `tofu version | head -n1` leaves tofu writing into a closed
  # pipe, and `set -o pipefail` then makes SIGPIPE the answer of the pipeline
  local said
  said=$("$dest" version 2>/dev/null) || return 0
  printf '%s' "${said%%$'\n'*}" | sed -n 's/^OpenTofu v\([0-9][^ ]*\).*/\1/p'
}

if [ "${1:-}" = --uninstall ]; then
  if [ -z "$(installed)" ]; then
    note "$dest is not an OpenTofu binary; nothing to remove"
    exit 0
  fi
  rm -f "$dest"
  note "removed $dest"
  exit 0
fi

if [ "$(installed)" = "$version" ]; then
  note "$dest is already OpenTofu $version"
  exit 0
fi

command -v curl >/dev/null || die "curl is not installed"
command -v unzip >/dev/null || die "unzip is not installed"

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=darwin ;;
  *) die "no OpenTofu build for $(uname -s)" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=amd64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *) die "no OpenTofu build for $(uname -m)" ;;
esac

base=https://github.com/opentofu/opentofu/releases/download/v$version
archive=tofu_${version}_${os}_${arch}.zip
sums=tofu_${version}_SHA256SUMS

mkdir -p "$cache" "$bin"
note "download OpenTofu $version for $os/$arch"
curl -fsSL --retry 3 -o "$cache/$sums" "$base/$sums" || die "cannot read $base/$sums. Check TOFU_VERSION."
curl -fsSL --retry 3 -o "$cache/$archive.part" "$base/$archive" || die "cannot read $base/$archive"
mv "$cache/$archive.part" "$cache/$archive"

want=$(awk -v f="$archive" '$2 == f || $2 == "*" f { print $1; exit }' "$cache/$sums")
[ -n "$want" ] || die "$sums gives no checksum for $archive"
if command -v sha256sum >/dev/null; then
  got=$(sha256sum "$cache/$archive" | cut -d' ' -f1)
else
  got=$(shasum -a 256 "$cache/$archive" | cut -d' ' -f1)
fi
[ "$want" = "$got" ] || die "$archive does not match the published checksum"

# Unzip into a temporary directory, so a failed extraction puts no partial binary on PATH.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
unzip -q -o "$cache/$archive" tofu -d "$tmp"
install -m 0755 "$tmp/tofu" "$dest"

[ "$(installed)" = "$version" ] || die "$dest does not report version $version"
note "installed $dest (OpenTofu $version), checksum correct"
case ":$PATH:" in
  *":$bin:"*) ;;
  *) note "$bin is not on PATH. Add it: export PATH=\"$bin:\$PATH\"" ;;
esac

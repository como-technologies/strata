#!/usr/bin/env bash
# Install one strata release, with its CI scripts, and check every file against SHA256SUMS.
#
#   scripts/ci/install-strata.sh [--version vX.Y.Z] [--dir DIR] [--repo OWNER/REPO]
#                                [--target TARGET] [--bin-only]
#
# Downloads the binary for this machine, SHA256SUMS, and the CI scripts pr-comment.sh,
# merge-gate.sh and verify-report.sh into DIR (default .strata), checks the checksums, and prints
# the path of the binary. A job then sets STRATA_BIN to that path and runs DIR/<script>.
#
# --bin-only keeps the binary alone, for a person installing into a directory on their PATH. The
# checksums are still checked before anything is installed.
#
# --version defaults to STRATA_VERSION, and latest means the latest release. A private repository
# needs a token that can read it in STRATA_TOKEN, GITHUB_TOKEN or GH_TOKEN; a public one needs no
# token. The release assets carry the same names on every host, so this runs on GitHub, on GitLab,
# and on a laptop.
#
# Needs curl, jq, and sha256sum or shasum.
set -euo pipefail

die() { echo "install-strata: $*" >&2; exit 2; }
note() { [ -n "${STRATA_QUIET:-}" ] || echo "install-strata: $*" >&2; }

version="${STRATA_VERSION:-latest}"
dir=".strata"
repo="${STRATA_REPO:-como-technologies/strata}"
target=""
bin_only=""
while [ $# -gt 0 ]; do
  case "$1" in
    --version) version=$2; shift 2 ;;
    --dir) dir=$2; shift 2 ;;
    --repo) repo=$2; shift 2 ;;
    --target) target=$2; shift 2 ;;
    --bin-only) bin_only=1; shift ;;
    -h|--help) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option $1" ;;
  esac
done

# The release carries one binary for each target it builds.
if [ -z "$target" ]; then
  case "$(uname -s) $(uname -m)" in
    "Linux x86_64") target=x86_64-unknown-linux-musl ;;
    "Linux aarch64"|"Linux arm64") target=aarch64-unknown-linux-musl ;;
    "Darwin arm64") target=aarch64-apple-darwin ;;
    "Darwin x86_64") target=x86_64-apple-darwin ;;
    *) die "no release binary for $(uname -s) $(uname -m); pass --target" ;;
  esac
fi

token="${STRATA_TOKEN:-${GITHUB_TOKEN:-${GH_TOKEN:-}}}"
api="${STRATA_API:-https://api.github.com}"
get() { # get URL OUTPUT [header...]
  local url=$1 out=$2
  shift 2
  if [ -n "$token" ]; then
    curl -fsSL -o "$out" -H "Authorization: Bearer $token" \
      -H "X-GitHub-Api-Version: 2022-11-28" "$@" "$url"
  else
    curl -fsSL -o "$out" "$@" "$url"
  fi
}

mkdir -p "$dir"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The release lists its assets; a private repository serves each one by id, never by its page URL.
case "$version" in
  latest) release_url="$api/repos/$repo/releases/latest" ;;
  *) release_url="$api/repos/$repo/releases/tags/$version" ;;
esac
get "$release_url" "$work/release.json" || die "could not read the release $version of $repo; a private repository needs STRATA_TOKEN"
tag=$(jq -r '.tag_name // empty' "$work/release.json")
[ -n "$tag" ] || die "the answer for $version is not a release: $(head -c 200 "$work/release.json")"

# SHA256SUMS comes down either way: it is what every file is checked against. The bash on macOS is
# 3.2, where "${empty[@]}" under set -u is an error, so an empty list expands through the + form.
scripts=(pr-comment.sh merge-gate.sh verify-report.sh)
[ -z "$bin_only" ] || scripts=()
want=("strata-$target" SHA256SUMS ${scripts[@]+"${scripts[@]}"})
for name in "${want[@]}"; do
  id=$(jq -r --arg n "$name" '.assets[] | select(.name == $n) | .id' "$work/release.json" | sed -n 1p)
  [ -n "$id" ] || die "the release $tag has no $name"
  get "$api/repos/$repo/releases/assets/$id" "$work/$name" -H "Accept: application/octet-stream"
done

# Check before anything is installed, and check the names the release signed.
(
  cd "$work"
  if command -v sha256sum > /dev/null; then
    sha256sum --check --ignore-missing --quiet SHA256SUMS
  elif command -v shasum > /dev/null; then
    shasum -a 256 --check --ignore-missing --quiet SHA256SUMS
  else
    die "need sha256sum or shasum to check SHA256SUMS"
  fi
) || die "the files of $tag do not match SHA256SUMS"

mv "$work/strata-$target" "$dir/strata"
chmod +x "$dir/strata"
for name in ${scripts[@]+"${scripts[@]}"}; do
  mv "$work/$name" "$dir/$name"
  chmod +x "$dir/$name"
done
# A person installing onto their PATH gets the binary and nothing beside it.
[ -n "$bin_only" ] || mv "$work/SHA256SUMS" "$dir/SHA256SUMS"

note "$tag installed in $dir, checksums verified"
echo "$dir/strata"

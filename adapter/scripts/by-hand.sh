#!/usr/bin/env bash
# Run the strata command and the platform port of knurl side by side, against the playground.
#
# It serves one chapter of the playground on this machine, runs each command of the two sides,
# and stops the server. It needs no tenant and no key, and nothing on the PATH but cargo, just,
# and jq. The strata that it runs is the one this checkout builds.
#
# Usage:
#   adapter/scripts/by-hand.sh [cluster-release|candidate]      just adapter-by-hand does the same
#
#   cluster-release   a profile version, the cluster that runs it, and who uses it
#   candidate         the identity of a definition, and a leaf. This chapter serves the
#                     registries, and cluster-release does not.
#
# On the strata side, `root` and `contentDigest` are bare hex. On the knurl side, `exact` and
# `content` are the same two values with the prefix `sha256:`.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

chapter="${1:-cluster-release}"
case "$chapter" in
  cluster-release | candidate) ;;
  *) echo "by-hand: the chapter is cluster-release or candidate, and not '$chapter'" >&2; exit 2 ;;
esac

just build examples stand-in >/dev/null
export CARGO_NET_GIT_FETCH_WITH_CLI=true
cargo build --quiet --manifest-path adapter/Cargo.toml --target-dir target/adapter --example observe

work=$(mktemp -d)
target/debug/xtask playground "$chapter" >"$work/server.log" 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null || true; rm -rf "$work"' EXIT
until grep -q 'Stop with Ctrl-C' "$work/server.log"; do
  kill -0 "$server" 2>/dev/null || { cat "$work/server.log" >&2; exit 2; }
  sleep 0.2
done
# The four export lines that the playground prints, without their comments.
eval "$(grep '^export PALETTE' "$work/server.log" | sed 's/ *#.*//')"
echo "by-hand: the chapter '$chapter' is served at $PALETTE_API"

strata() { target/release/strata "$@"; }
observe() { target/adapter/debug/examples/observe "$@"; }
# Prints a command, runs it, and prints its exit code. An exit code that is not 0 is an answer
# too: `consumers` gives 1 when a cluster uses the version.
run() {
  printf '\n$ %s\n' "$*"
  local code=0
  "$@" || code=$?
  echo "[exit $code]"
}

if [ "$chapter" = cluster-release ]; then
  echo; echo "=== the strata side ==="
  run strata snapshot aws-stack@1.1.0 -o "$work/record.json" --quiet
  run jq -c '{root, contentDigest}' "$work/record.json"
  run strata consumers aws-stack@1.1.0 --quiet
  run strata verify "$work/record.json" --quiet --summary
  echo; echo "=== the knurl side, through the adapter ==="
  run observe unit intermediate aws-stack 1.1.0
  run observe instance payments
  run observe pinned-by aws-stack 1.1.0
  run observe verify aws-stack 1.1.0
else
  echo; echo "=== the strata side ==="
  run strata snapshot aws-stack@1.1.1 -o "$work/published.json" --quiet
  run strata candidate aws-stack@1.1.1 -o "$work/candidate.yaml" --quiet
  run strata snapshot --from-candidate "$work/candidate.yaml" -o "$work/candidate.json" --quiet
  run jq -c '{from: .source.template, root, contentDigest}' "$work/published.json" "$work/candidate.json"
  echo; echo "=== the knurl side, through the adapter ==="
  run observe identity aws-stack 1.1.1
  run observe unit leaf cert-manager 1.21.1
  echo
  echo "# The next one is a failure on purpose: this tenant has the version in two kinds of"
  echo "# registry, with two archives, and the address has no digest."
  run observe unit leaf csi-aws-ebs 1.65.0
  echo
  echo "# The failure names each archive. The address with the digest of one chooses it."
  archive=$({ observe unit leaf csi-aws-ebs 1.65.0 2>&1 || true; } | grep -o 'sha256:[0-9a-f]\{64\}' | head -1 || true)
  run observe unit leaf csi-aws-ebs 1.65.0 "$archive"
fi

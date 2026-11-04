#!/usr/bin/env bash
# Run the observer on this machine against the stand-in for Palette, and serve its page.
#
#   scripts/observer-playground.sh [CHAPTER-N]
#
# The records come from state 1 of the chapter, before the edit in place. The page then shows state N.
# The default is observer-2, the state with one finding and the newer versions. OBSERVER_CLUSTER names
# the cluster that the page watches. The default is payments. OBSERVER_LISTEN gives the address.
# Needs just, jq, and the binaries of strata and strata-observer: just observer-playground builds them.
set -euo pipefail
root=${STRATA_ROOT:-$(git rev-parse --show-toplevel)}
strata=${STRATA_BIN:-$root/target/release/strata}
observer_bin=${STRATA_OBSERVER_BIN:-$root/target/observer/strata-observer}
listen=${OBSERVER_LISTEN:-127.0.0.1:8080}
cluster=${OBSERVER_CLUSTER:-payments}
name=${1:-observer-2}
chapter=${name%-[0-9]}
work=$(mktemp -d)
pg=
cleanup() {
  [ -z "$pg" ] || kill -- "-$pg" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
records_revision=playground
# shellcheck source=scripts/observer-lib.sh
source "$root/scripts/observer-lib.sh"

[ "$chapter" != "$name" ] || name=$chapter-1
for binary in "$strata" "$observer_bin"; do
  [ -x "$binary" ] || { echo "no binary at $binary. Run: just build observer-build" >&2; exit 2; }
done

# The records are the profiles that the clusters run, as the pipeline of a team would write them.
start "$chapter-1"
cd "$work"
for ref in aws-stack@1.0.0 aws-stack@1.1.0 platform-addons@1.0.0; do
  "$strata" snapshot "$ref" -o "records/${ref/@/_}.json" -q
done
records_dir playground records/*.json
stop

start "$name"
uid=$("$strata" list | sed -n "s/.*cluster:\\([0-9a-f]\\{24\\}\\) *$cluster .*/\\1/p" | head -n 1)
[ -n "$uid" ] || { echo "the tenant of $name has no cluster named $cluster" >&2; exit 2; }
mkdir -p "$work/cycle" "$work/state-observer"
echo "The page of cluster $cluster is at http://$listen. Stop with Ctrl-C."
"$observer_bin" serve --strata "$strata" --cluster-uid "$uid" --records "$work/records-playground" \
  --work "$work/cycle" --state "$work/state-observer" --listen "$listen"

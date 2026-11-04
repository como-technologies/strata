#!/usr/bin/env bash
# Run the commands of the observer against the stand-in for Palette, with the strata binary of this
# commit, and give the documents to the allow-list of the observer.
#
#   scripts/observer-contract.sh [--write]
#
# --write also copies the documents to observer/internal/docs/testdata/, for the tests with no binary.
# Needs just, go, and target/release/strata.
set -euo pipefail
root=${STRATA_ROOT:-$(git rev-parse --show-toplevel)}
observer=${STRATA_OBSERVER:-$root/observer}
strata=${STRATA_BIN:-$root/target/release/strata}
work=$(mktemp -d)
docs=$work/docs
mkdir -p "$docs"
pg=
cleanup() {
  [ -z "$pg" ] || kill -- "-$pg" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

start() {
  local log=$work/$1.log
  (cd "$root" && exec setsid just playground "$1") >"$log" 2>&1 &
  pg=$!
  for _ in $(seq 300); do
    grep -q 'export PALETTE_API=' "$log" && break
    sleep 1
  done
  grep -q 'export PALETTE_API=' "$log" || { cat "$log" >&2; echo "the stand-in did not start: $1" >&2; exit 2; }
  sleep 1
  # shellcheck disable=SC1090
  source <(sed 's/\x1b\[[0-9;]*m//g' "$log" | grep -E '^export [A-Z_]+=')
  export STRATA_REPORT=0 XDG_STATE_HOME=$work/state STRATA_QUIET=1
}
stop() {
  kill -- "-$pg" 2>/dev/null || true
  wait "$pg" 2>/dev/null || true
  pg=
  sleep 1
}
# The exit code of a strata command that gives an answer is 0 or 1.
run() { "$strata" "$@" --json --compact -q || [ "$?" -le 1 ]; }

cd "$work"

start edited-in-place-1
"$strata" snapshot aws-stack@1.1.1 -o drift-record.json -q
stop
start edited-in-place-2
run verify drift-record.json --summary --who >"$docs/verify-summary-drift.json"
stop

start behind-2
"$strata" snapshot aws-stack@1.0.0 -o behind-record.json -q
"$strata" behind behind-record.json --no-impact --json --compact -q >"$docs/behind.json" || [ "$?" -eq 1 ]
registry=5eecc89d0b150045ae661cef
run compare "pack:$registry/kubernetes@1.35.3" "pack:$registry/kubernetes@1.36.2" >"$docs/move-kubernetes.json"
run compare "pack:$registry/cert-manager@1.20.3" "pack:$registry/cert-manager@1.21.1" >"$docs/move-cert-manager.json"
run compare "pack:$registry/cni-cilium-oss@1.19.6" "pack:$registry/cni-cilium-oss@1.20.1" >"$docs/move-cni-cilium-oss.json"
stop

start cluster-release
uid=$("$strata" list | sed -n 's/.*cluster:\([0-9a-f]\{24\}\) *payments.*/\1/p')
"$strata" snapshot "cluster:$uid" --raw-dir raw -o "$docs/cluster-payments.json" -q
"$strata" snapshot aws-stack@1.1.0 -o "$docs/record-aws-stack.json" -q
"$strata" snapshot platform-addons@1.0.0 -o record-platform-addons.json -q
for profile in aws-stack platform-addons; do
  record=record-$profile.json
  [ "$profile" != aws-stack ] || record=$docs/record-aws-stack.json
  "$strata" snapshot "cluster:$uid/$profile" --from-raw raw -o "side-$profile.json" -q
  run compare "$record" "side-$profile.json" >"$docs/compare-payments-$profile.json"
done
stop

# Palette does not answer. A snapshot writes a plain line, so the kind comes from the row of a verify.
PALETTE_API=http://127.0.0.1:9 STRATA_RETRIES=0 "$strata" verify "$docs/record-aws-stack.json" --summary --json --compact -q \
  >"$docs/verify-summary-down.json" || [ "$?" -eq 2 ]

# This cluster has the value team-payments in a variable, so the document holds a clear value.
start cluster-values
uid=$("$strata" list | sed -n 's/.*cluster:\([0-9a-f]\{24\}\) *payments.*/\1/p')
"$strata" snapshot aws-stack@1.1.1 -o values-record.json -q
run compare values-record.json "cluster:$uid/aws-stack" >"$docs/compare-values.json"
stop

STRATA_BIN=$strata STRATA_DOCS=$docs go -C "$observer" test ./internal/docs/ -count=1 -run 'Schema|DocumentsOfTheBinary|ClearValue|KindOfAFailedCheck'
if [ "${1:-}" = --write ]; then
  mkdir -p "$observer/internal/docs/testdata"
  cp "$docs/verify-summary-drift.json" "$docs/behind.json" "$docs"/move-*.json "$observer/internal/docs/testdata/"
fi

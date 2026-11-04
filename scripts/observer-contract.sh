#!/usr/bin/env bash
# Run the commands of the observer against the stand-in for Palette, with the strata binary of this
# commit, and give the documents to the allow-list of the observer.
#
#   scripts/observer-contract.sh [--write]
#
# --write also copies the documents to observer/internal/docs/testdata/, for the tests with no binary.
# It also builds strata-observer and runs `once` against the seeds, with the real binary of strata.
# Needs just, go, jq, and target/release/strata.
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

observer_bin=$work/strata-observer
go -C "$observer" build -o "$observer_bin" .

# records_dir NAME FILE... writes the records of a baseline as *.json.gz, with a meta.json.
records_dir() {
  local dir=$work/records-$1
  shift
  mkdir -p "$dir"
  for file in "$@"; do gzip -c "$file" >"$dir/$(basename "$file" .json).json.gz"; done
  echo '{"baseline":"approved","revision":"contract"}' >"$dir/meta.json"
}

# once NAME CLUSTER-UID RECORDS-NAME runs `strata-observer once` against the stand-in that runs now.
# It keeps the summary in once-NAME.json and the log in once-NAME.err. A run that gives a document exits
# 0, 1 for a finding, or 2 for a failed cycle. The state directory is the same for the same NAME.
# With OBSERVER_CONTRACT_SHOW set, it prints one line for each run, with no time and no value.
once() {
  local code=0
  mkdir -p "$work/once-work"
  "$observer_bin" once --strata "$strata" --cluster-uid "$2" --records "$work/records-$3" --work "$work/once-work" \
    --state "$work/once-state-$1" >"$work/once-$1.json" 2>"$work/once-$1.err" || code=$?
  [ "$code" -le 2 ] || { echo "once $1 exited $code" >&2; exit 2; }
  [ -z "$(ls -A "$work/once-work")" ] || { echo "the work directory is not empty after once $1" >&2; exit 2; }
  if [ -n "${OBSERVER_CONTRACT_SHOW:-}" ]; then
    echo "once $1: exit $code, work empty, $(jq -c '{page, findings: [.findings[] | {kind, profile, packs, alsoOnCluster, who: (.who | length)}], content: (.content | {failedRuns, lastError})}' "$work/once-$1.json")"
  fi
}
# expect NAME FILTER fails when the jq filter is not true for the summary of once NAME.
expect() {
  jq -e "$2" "$work/once-$1.json" >/dev/null || { echo "once $1: not true: $2" >&2; jq -c . "$work/once-$1.json" >&2; exit 2; }
}

cd "$work"

start edited-in-place-1
"$strata" snapshot aws-stack@1.1.1 -o drift-record.json -q
stop
start edited-in-place-2
run verify drift-record.json --summary --who >"$docs/verify-summary-drift.json"

# The record of the first seed against the second seed: the published version changed, with who.
records_dir edited drift-record.json
once edited "$("$strata" list | sed -n 's/.*cluster:\([0-9a-f]\{24\}\) *payments .*/\1/p')" edited
expect edited '(.findings | length) == 1 and .findings[0].kind == "published-changed" and (.findings[0].who | length) >= 1'
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

# The seed serves no registry, so the versions cycle fails. The content cycle gives the answer.
records_dir release "$docs/record-aws-stack.json" record-platform-addons.json
for name in payments ledger; do
  once "release-$name" "$("$strata" list | sed -n "s/.*cluster:\([0-9a-f]\{24\}\) *$name .*/\1/p")" release
done
expect release-payments '.page == "ok" and .counts.findings == 0 and .content.failedRuns == 0 and .cluster.name == "payments"'
expect release-ledger '.content.failedRuns == 0 and (.findings | length) == 1 and .findings[0].kind == "cluster-differs" and .findings[0].packs == ["csi/csi-aws-ebs"]'
stop

# Palette does not answer. A snapshot writes a plain line, so the kind comes from the row of a verify.
PALETTE_API=http://127.0.0.1:9 STRATA_RETRIES=0 "$strata" verify "$docs/record-aws-stack.json" --summary --json --compact -q \
  >"$docs/verify-summary-down.json" || [ "$?" -eq 2 ]

# This cluster has the value team-payments in a variable, so the document holds a clear value.
start cluster-values
uid=$("$strata" list | sed -n 's/.*cluster:\([0-9a-f]\{24\}\) *payments.*/\1/p')
"$strata" snapshot aws-stack@1.1.1 -o values-record.json -q
run compare values-record.json "cluster:$uid/aws-stack" >"$docs/compare-values.json"

# The raw output of strata holds the value, so the grep of the output of once proves something.
grep -q team-payments "$docs/compare-values.json" || { echo "the compare holds no clear value" >&2; exit 2; }
records_dir values values-record.json
once values "$uid" values
expect values '.content.failedRuns == 0'
! grep -q team-payments "$work/once-values.json" "$work/once-values.err" || { echo "a clear value left the observer" >&2; exit 2; }
if [ -n "${OBSERVER_CONTRACT_SHOW:-}" ]; then
  echo "grep -c team-payments:"
  (cd "$work" && grep -c team-payments docs/compare-values.json once-values.json once-values.err || true)
fi
stop


# The stand-in is stopped. Two runs with one state directory reach lost, with the kind transport. One
# record keeps the retries of strata short.
records_dir stopped record-platform-addons.json
once stopped "$uid" stopped
expect stopped '.page == "first-load" and .content.failedRuns == 1'
once stopped "$uid" stopped
expect stopped '.page == "lost" and .content.failedRuns == 2 and .content.lastError.kind == "transport"'

STRATA_BIN=$strata STRATA_DOCS=$docs go -C "$observer" test ./internal/docs/ -count=1 -run 'Schema|DocumentsOfTheBinary|ClearValue|KindOfAFailedCheck'
if [ "${1:-}" = --write ]; then
  mkdir -p "$observer/internal/docs/testdata"
  cp "$docs/verify-summary-drift.json" "$docs/behind.json" "$docs"/move-*.json "$observer/internal/docs/testdata/"
fi

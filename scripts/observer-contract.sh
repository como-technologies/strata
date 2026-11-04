#!/usr/bin/env bash
# Run the commands of the observer against the stand-in for Palette, with the strata binary of this
# commit, and give the documents to the allow-list of the observer.
#
#   scripts/observer-contract.sh [--write]
#
# --write also copies the documents to observer/internal/docs/testdata/, for the tests with no binary.
# It also builds strata-observer and runs `once` against the seeds, with the real binary of strata.
# Needs just, go, jq, curl, and target/release/strata.
set -euo pipefail
root=${STRATA_ROOT:-$(git rev-parse --show-toplevel)}
observer=${STRATA_OBSERVER:-$root/observer}
strata=${STRATA_BIN:-$root/target/release/strata}
work=$(mktemp -d)
docs=$work/docs
mkdir -p "$docs"
pg=
serve_pid=
cleanup() {
  [ -z "$serve_pid" ] || kill "$serve_pid" 2>/dev/null || true
  [ -z "$pg" ] || kill -- "-$pg" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
records_revision=contract
# shellcheck source=scripts/observer-lib.sh
source "$root/scripts/observer-lib.sh"

# The exit code of a strata command that gives an answer is 0 or 1.
run() { "$strata" "$@" --json --compact -q || [ "$?" -le 1 ]; }

observer_bin=$work/strata-observer
go -C "$observer" build -o "$observer_bin" .

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

# serve_up CLUSTER-UID RECORDS-NAME PORT starts `strata-observer serve` against the stand-in that runs now.
# It returns when the first content cycle ended well, and fails when that takes more than two minutes.
serve_up() {
  mkdir -p "$work/serve-work"
  "$observer_bin" serve --strata "$strata" --cluster-uid "$1" --records "$work/records-$2" --work "$work/serve-work" \
    --state "$work/serve-state" --listen "127.0.0.1:$3" >"$work/serve.out" 2>"$work/serve.err" &
  serve_pid=$!
  for _ in $(seq 120); do
    [ "$(curl -s "http://127.0.0.1:$3/v1/cluster" | jq -r '.page // empty')" = ok ] && return 0
    sleep 1
  done
  cat "$work/serve.err" >&2
  echo "the server did not reach the page state ok" >&2
  exit 2
}
# serve_down stops the server with SIGTERM, and fails when its exit code is not 0.
serve_down() {
  kill "$serve_pid"
  wait "$serve_pid" || { echo "the server exited with $?" >&2; exit 2; }
  serve_pid=
}
# status NAME EXPECTED CURL-ARGUMENT... fails when the HTTP status that curl gets is not the expected one.
status() {
  local got
  got=$(curl -s -o /dev/null -w '%{http_code}' "${@:3}")
  [ "$got" = "$2" ] || { echo "$1: status $got, expected $2" >&2; exit 2; }
  if [ -n "${OBSERVER_CONTRACT_SHOW:-}" ]; then echo "$1: status $got"; fi
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
once edited "$(cluster_uid payments)" edited
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
uid=$(cluster_uid payments)
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
  once "release-$name" "$(cluster_uid "$name")" release
done
expect release-payments '.page == "ok" and .counts.findings == 0 and .content.failedRuns == 0 and .cluster.name == "payments"'
expect release-ledger '.content.failedRuns == 0 and (.findings | length) == 1 and .findings[0].kind == "cluster-differs" and .findings[0].packs == ["csi/csi-aws-ebs"]'
stop

# Palette does not answer. A snapshot writes a plain line, so the kind comes from the row of a verify.
PALETTE_API=http://127.0.0.1:9 STRATA_RETRIES=0 "$strata" verify "$docs/record-aws-stack.json" --summary --json --compact -q \
  >"$docs/verify-summary-down.json" || [ "$?" -eq 2 ]

# This cluster has the value team-payments in a variable, so the document holds a clear value.
start cluster-values
uid=$(cluster_uid payments)
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

# The server shows the tiers. A summary route holds no clear value, and the server refuses a POST and a
# Host that is not loopback. The port is high, so that it does not meet a service of the host.
port=$((20000 + RANDOM % 20000))
base=http://127.0.0.1:$port
serve_up "$uid" values "$port"
for route in cluster findings profiles newer notchecked events; do
  curl -s "$base/v1/$route" >"$work/serve-$route.json"
  jq -e . "$work/serve-$route.json" >/dev/null || { echo "/v1/$route is no JSON" >&2; exit 2; }
  ! grep -q team-payments "$work/serve-$route.json" || { echo "a clear value is in /v1/$route" >&2; exit 2; }
done
status "POST" 405 -X POST "$base/v1/cluster"
status "Host: evil.example" 421 -H 'Host: evil.example' "$base/v1/cluster"
status "unknown profile" 404 "$base/v1/profiles/nope"
status "health" 200 "$base/healthz"
"$observer_bin" health --listen "127.0.0.1:$port" || { echo "health failed with the server up" >&2; exit 2; }
serve_down
code=0
"$observer_bin" health --listen "127.0.0.1:$port" 2>/dev/null || code=$?
[ "$code" = 1 ] || { echo "health exited $code with no server, expected 1" >&2; exit 2; }
code=0
"$observer_bin" serve --strata "$strata" --cluster-uid "$uid" --records "$work/records-values" --work "$work/serve-work" \
  --state "$work/serve-state" --listen=0.0.0.0:8080 2>/dev/null || code=$?
[ "$code" = 2 ] || { echo "serve --listen=0.0.0.0:8080 exited $code, expected 2" >&2; exit 2; }
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

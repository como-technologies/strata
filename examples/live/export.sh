#!/usr/bin/env bash
# The profile export test (#156): Palette exports a profile version as one file, and
# `snapshot --from-candidate` reads that file as a candidate. It builds no cluster, creates no
# profile, and needs no OpenTofu: each request is a GET. So it runs whenever the key works.
#
#   examples/live/export.sh [STAGE]
#
#   1  for each version: the record of the version, the export with format=yaml, the record of
#      the export, and the two roots. A version with a manifest pack differs only by the pack uid
#      that Palette mints.
#   2  the export with format=json gives the same record as the export with format=yaml
#   3  what the export route answers: a request that accepts JSON gets no file; and three
#      exports at once, whose refusals the run records
#
# PROFILES holds the versions, by <name>@<version>, in the project of the key. The default is the
# three versions that `just live-up` publishes. Palette can refuse an export with HTTP 429 and
# the message "only 1 request in 10 seconds is allowed", so the script waits 11 seconds before
# each export.
#
# Results go under runs/<UTC time>/export/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=export
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

stage=${1:-all}
profiles=${PROFILES:-strata-live-profile@1.0.0 strata-live-registries@1.0.0 strata-live-addon@1.0.0}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/export}
mkdir -p "$run" && cd "$run" || exit 2

summary() {
  {
    echo "# profile export test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`: \`$profiles\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

# export_file ID WHAT UID FORMAT: one export, after the wait that the limit of Palette needs
export_file() {
  sleep 11
  check "$1" 0 "$2" -- "$live/api.sh" export "$3" "$4"
}

# the record of each version gives its uid, and the root that the export must give
for ref in $profiles; do
  name=${ref%@*}
  "$strata" snapshot "$ref" -q -o "$name.version.json" || die "no record of $ref"
done

if [ "$stage" = all ] || [ "$stage" = 1 ]; then
  echo "== the export of each version, read as a candidate (#156)" | tee -a steps.log
  for ref in $profiles; do
    name=${ref%@*}
    uid=$(jq -r .source.uid "$name.version.json")
    export_file "export-$name" "#156: GET /v1/clusterprofiles/{uid}/export?format=yaml gives the file of $ref" "$uid" yaml
    cp "export-$name.out" "$name.yaml"
    check "export-record-$name" 0 "#156: snapshot --from-candidate reads the export of $ref" -- \
      "$strata" snapshot --from-candidate "$name.yaml" -o "$name.export.json"
    check "export-same-$name" 0 "#156: same-release passes the record of the export and the record of $ref" -- \
      "$strata" same-release "$name.export.json" "$name.version.json"
    if jq -e 'any(.packs[]; .type == "manifest")' "$name.version.json" > /dev/null; then
      check "export-minted-$name" 0 "#156: the roots differ only by the uid that Palette mints for the manifest pack" -- \
        bash -c '"$1" same-release "$2" "$3" --json | jq -e ".pairs[0].mintedUidOnly | length > 0"' _ \
        "$strata" "$name.export.json" "$name.version.json"
    else
      check "export-root-$name" 0 "#156: the record of the export has the root of $ref" -- \
        test "$(jq -r .root "$name.export.json")" = "$(jq -r .root "$name.version.json")"
    fi
  done
fi

if [ "$stage" = all ] || [ "$stage" = 2 ]; then
  echo "== the export as JSON (#156)" | tee -a steps.log
  ref=${profiles%% *}
  name=${ref%@*}
  uid=$(jq -r .source.uid "$name.version.json")
  export_file "export-json-$name" "#156: format=json gives the same file as JSON" "$uid" json
  cp "export-json-$name.out" "$name.export-file.json"
  check "export-json-record-$name" 0 "#156: snapshot --from-candidate reads the JSON export" -- \
    "$strata" snapshot --from-candidate "$name.export-file.json" -o "$name.export-json.json"
  check "export-json-root-$name" 0 "#156: the JSON export gives the root of $ref" -- \
    test "$(jq -r .root "$name.export-json.json")" = "$(jq -r .root "$name.version.json")"
fi

if [ "$stage" = all ] || [ "$stage" = 3 ]; then
  echo "== what the export route answers (#156)" | tee -a steps.log
  ref=${profiles%% *}
  name=${ref%@*}
  uid=$(jq -r .source.uid "$name.version.json")
  sleep 11
  check export-accepts-json 0 '#156: a request that accepts JSON gets an answer' -- \
    "$live/api.sh" get "/v1/clusterprofiles/$uid/export?format=yaml"
  check export-accepts-json-no-file 0 '#156: the answer is {"Reader":{}}, and not the file' -- \
    grep -qx '{"Reader":{}}' export-accepts-json.out
  sleep 11
  # three exports at once: some runs see HTTP 429 TooManyRequests, and some do not, so the check
  # records the exit code and the answers, and does not judge them
  check export-limit '?' '#156: three exports at once; exit 2 when Palette refused one or more' -- \
    bash -c 'refused=0; for i in 1 2 3; do "$1" export "$2" yaml > /dev/null || refused=2; done; exit $refused' _ \
    "$live/api.sh" "$uid"
fi

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

#!/usr/bin/env bash
# The fleet test (#80): the clusters that a query of labels selects, from the search of Palette or
# a saved tag filter, and a record of each, written to a directory.
#
#   examples/live/fleet.sh
#
#   1  cluster A carries the tags of the example as labels: the listing and a record of the
#      cluster carry them, and list clusters --tag finds the cluster at tenant scope, with its
#      project
#   2  the label env=prod on cluster A, through the API, and a saved tag filter that selects it:
#      list clusters --tag and --filter find cluster A, a query that no cluster meets finds
#      none, and the saved filters refuse a request that names the project of cluster A
#   3  snapshot filter:<name> --dir --prune: one record for the cluster, and a record of a
#      cluster that left the set is removed
#   4  the label removed: the next run removes the record of cluster A, which a commit shows
#   5  the filter deleted, and the labels of cluster A as they were
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# jq, and the key as verify.sh needs it. Results go under runs/<UTC time>/fleet/, or OUT. Exit 0
# only when no check fails.
set -uo pipefail
script=fleet
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/fleet}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
C=$(out cluster_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
FILTER=strata-live-prod

summary() {
  write_summary "fleet test" ", cluster \`$C\`, filter \`$FILTER\`."
}
trap summary EXIT

# row FILE: the row of cluster A in a list --json
row() { jq -c --arg c "$C" '[.clusters[] | select(.uid == $c)] | first' "$1"; }

echo "== 1. the tags of the example are labels" | tee -a steps.log
require_baseline
check listing 0 '#80: list clusters, the plain listing' -- "$strata" list clusters --json
check listing-labels 0 '#80: the listed cluster carries its labels: the tags of the example' -- \
  bash -c "jq -e '.labels.purpose == \"strata-live-example\" and .labels[\"managed-by\"] == \"opentofu\"' <<<'$(row listing.out)'"
check record 0 'record cluster A' -- "$strata" snapshot "cluster:$C" -o a.json
check record-labels 0 '#80: the record carries the labels' -- \
  jq -e '.source.cluster.labels.purpose == "strata-live-example"' a.json
labels=$(jq -c '.source.cluster.labels' a.json)
check tag 0 '#80: list clusters --tag finds the cluster by a tag of the example' -- \
  "$strata" list clusters --tag purpose=strata-live-example --json
check tag-search 0 '#80: from the search of Palette, at tenant scope, with the project of the cluster' -- \
  bash -c "jq -e '.resolved == \"search\" and .projectUid == \"\"' tag.out && jq -e --arg p $PALETTE_PROJECT_UID '.projectUid == \$p and (.projectName | length > 0)' <<<'$(row tag.out)'"

echo "== 2. a label and a saved filter" | tee -a steps.log
check label 0 'cluster A gets the label env=prod, through the API' -- \
  "$live/api.sh" patch "/v1/spectroclusters/$C/metadata" "{\"metadata\": {\"labels\": $(jq -c '. + {"env": "prod"}' <<<"$labels")}}"
check prod 0 '#80: list clusters --tag env=prod finds cluster A' -- "$strata" list clusters --tag env=prod --json
check prod-row 0 '#80: and only cluster A, with the label' -- \
  jq -e --arg c "$C" '[.clusters[].uid] == [$c] and .clusters[0].labels.env == "prod"' prod.out
check none 0 '#80: a query that no cluster meets finds none' -- \
  bash -c "\"$strata\" list clusters --tag env=prod --tag team=nobody --json | jq -e '.clusters == []'"
check filter-create 0 "a saved tag filter $FILTER that selects env=prod, through the API" -- \
  "$live/api.sh" filter-create "$FILTER" env prod
F=$(cat filter-create.out)
check filters 0 '#80: list filters names the filter, and what it selects' -- \
  bash -c "\"$strata\" list filters | grep -Eq '^  filter:$FILTER +$F +env=prod$'"
check by-filter 0 '#80: list clusters --filter finds cluster A' -- \
  bash -c "\"$strata\" list clusters --filter $FILTER --json | jq -e --arg c $C '[.clusters[].uid] == [\$c] and .queryFilter.name == \"$FILTER\"'"
# the saved filters belong to the tenant: a request that names the project of cluster A is refused,
# also with the query filterType=tag. api.sh sends PALETTE_PROJECT_UID, and get-status prints the
# status and the body of the refusal
check filters-project 0 '#539: the listing of the saved filters, asked with the project of cluster A' -- \
  "$live/api.sh" get-status '/v1/filters/metadata?filterType=tag'
check filters-project-says 0 '#539: Palette answers HTTP 403 with the code ProjectUidIsNotEmpty' -- \
  bash -c "head -n 1 filters-project.out | grep -qx 'HTTP 403' && sed 1d filters-project.out | jq -e '.code == \"ProjectUidIsNotEmpty\"'"
check filter-project 0 "#539: the saved filter $FILTER, asked with the project of cluster A" -- \
  "$live/api.sh" get-status "/v1/filters/tag/$F"
check filter-project-says 0 '#539: Palette answers HTTP 403 with the code ProjectUidIsNotEmpty' -- \
  bash -c "head -n 1 filter-project.out | grep -qx 'HTTP 403' && sed 1d filter-project.out | jq -e '.code == \"ProjectUidIsNotEmpty\"'"

echo "== 3. a record of each cluster of the set" | tee -a steps.log
mkdir -p fleet
cp a.json fleet/left.json
echo '{"not": "a record"}' > fleet/notes.json
check set 0 "#80: snapshot filter:$FILTER --dir fleet --prune" -- \
  "$strata" snapshot "filter:$FILTER" --dir fleet --prune
check set-files 0 '#80: one record, of cluster A, and the record of a cluster that left the set is removed' -- \
  bash -c "[ \"\$(ls fleet | tr '\\n' ' ')\" = \"$(jq -r .source.cluster.name a.json).json notes.json \" ] && grep -q '^removed fleet/left.json$' set.out"
check set-record 0 '#80: the record is of cluster A, with the label' -- \
  bash -c "jq -e --arg c $C '.source.cluster.uid == \$c and .source.cluster.labels.env == \"prod\"' fleet/*.json 2>/dev/null | grep -q true"
check set-tag 0 '#80: the same with tag:env=prod' -- \
  bash -c "\"$strata\" snapshot tag:env=prod --dir fleet-by-tag && ls fleet-by-tag | grep -q json"
check set-verify 0 '#80: verify of the record that the set wrote' -- \
  "$strata" verify "fleet/$(jq -r .source.cluster.name a.json).json" --summary

echo "== 4. the label goes" | tee -a steps.log
check unlabel 0 'cluster A loses the label env=prod, through the API' -- \
  "$live/api.sh" patch "/v1/spectroclusters/$C/metadata" "{\"metadata\": {\"labels\": $labels}}"
check set-again 0 "#80: the next run of snapshot filter:$FILTER --dir fleet --prune" -- \
  "$strata" snapshot "filter:$FILTER" --dir fleet --prune
check set-removed 0 '#80: the record of cluster A is removed: the cluster left the set' -- \
  bash -c "grep -q '^removed fleet/' set-again.out && [ \"\$(ls fleet)\" = notes.json ]"

echo "== 5. the baseline" | tee -a steps.log
check filter-delete 0 "delete the filter $FILTER" -- "$live/api.sh" filter-delete "$F"
check baseline 0 'the example is at its baseline: the labels are the tags of the example again' -- "$live/baseline.sh"

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

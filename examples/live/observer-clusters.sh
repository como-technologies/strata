#!/usr/bin/env bash
# The observer of a set of clusters (#702). The key is the key of a user whose one role is Project
# Viewer in the project of cluster A. The code of Palette gives the answer that each check of the role
# expects.
# The book states each answer as a fact of the code until a run of this script shows it.
#
#   OBSERVER_ROLE_KEY=KEY examples/live/observer-clusters.sh [FROM_STAGE]
#
#   1  the label env=prod on cluster A, and a saved tag filter that selects it, through the API with
#      the key of the example
#   2  the reads of Palette with the key of the role: the saved filters at tenant scope, and the
#      cluster search at tenant scope and in the project of cluster A. The key of the example reads
#      each one
#   3  list clusters with the key of the role: --tag with --project finds cluster A, and --tag at
#      tenant scope and --filter fail
#   4  the observer with --clusters tag:env=prod and the key of the role as api-key: one content
#      cycle reads cluster A in its project
#   5  the baseline: the filter goes, and the labels of cluster A are as they were
#
# Make the user of the key as observer-role.sh says, with the role Project Viewer. The key stays in the
# environment of one command at a time. It is also in the file of the keys of stage 4, and the run
# removes that file.
# With no key, the script says "could not check" and exits 2.
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# the observer binary (just observer-build, or OBSERVER_BIN), and jq. Stages 1 and 5 change the labels
# of cluster A and a saved filter of the tenant. Results go under runs/<UTC time>/observer-clusters/, or
# OUT; FROM_STAGE resumes a run in OUT. Exit 0 only when no check fails.
set -uo pipefail
script=observer-clusters
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4|5) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"
observer_bin=${OBSERVER_BIN:-$(cd "$live/../.." && pwd)/target/observer/strata-observer}
[ -n "${OBSERVER_ROLE_KEY:-}" ] \
  || die "could not check: set OBSERVER_ROLE_KEY to the key of a user with the role Project Viewer alone"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/observer-clusters}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
C=$(out cluster_uid) P=$(out project_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$P
FILTER=strata-live-set
SEARCH='/v1/dashboard/spectroclusters/search?limit=50'

summary() {
  rm -rf keys
  write_summary "observer of a set of clusters" ", cluster $(md span "$C"), filter $(md span "$FILTER")."
}
trap summary EXIT

# with_role COMMAND...: run COMMAND with the key of the role
with_role() { PALETTE_API_KEY=$OBSERVER_ROLE_KEY "$@"; }

# search_body: the body of the cluster search for the label env=prod, as strata sends it
search_body() {
  jq -n '{filter: {conjunction: "and", filterGroups: [{conjunction: "and", filters: [{property: "tags", type: "string",
    condition: {string: {operator: "eq", negation: false, ignoreCase: false, match: {conjunction: "or", values: ["env:prod"]}}}}]}]},
    sort: [{field: "clusterName", order: "asc"}]}' > search-body.json
}

# holds_cluster FILE: FILE holds HTTP 200 and a search whose items name cluster A, each in the project of cluster A
holds_cluster() {
  answer_is 200 "$1" '([.items[].metadata.uid] | index($c)) != null and all(.items[]; .specSummary.projectMeta.uid == $p)' \
    --arg c "$C" --arg p "$P"
}

if stage 1 "a label and a saved filter"; then
  require_baseline
  check record 0 'record cluster A, for its labels' -- "$strata" snapshot "cluster:$C" -o a.json
  labels=$(jq -c '.source.cluster.labels' a.json)
  check label 0 'cluster A gets the label env=prod, through the API' -- \
    "$live/api.sh" patch "/v1/spectroclusters/$C/metadata" "{\"metadata\": {\"labels\": $(jq -c '. + {"env": "prod"}' <<<"$labels")}}"
  check filter-create 0 "a saved tag filter $FILTER that selects env=prod, through the API" -- \
    "$live/api.sh" filter-create "$FILTER" env prod
fi
F=$(cat filter-create.out 2>/dev/null)

if stage 2 "the reads of Palette with the key of the role"; then
  check filters-control 0 '#702: the key of the example reads the saved filters at tenant scope' -- \
    "$live/api.sh" tenant-get-status '/v1/filters/metadata?filterType=tag'
  check filters-control-says 0 '#702: Palette answers HTTP 200' -- answered 200 filters-control.out
  check filters-role 0 '#702: the key of the role asks for the saved filters at tenant scope' -- \
    with_role "$live/api.sh" tenant-get-status '/v1/filters/metadata?filterType=tag'
  check filters-role-says 0 '#702: Palette answers HTTP 403 with AccessDenied: the role holds no filter.list in the tenant' -- \
    refused 403 AccessDenied filters-role.out
  check filter-role 0 "#702: the key of the role asks for the saved filter $FILTER at tenant scope" -- \
    with_role "$live/api.sh" tenant-get-status "/v1/filters/tag/$F"
  check filter-role-says 0 '#702: Palette answers HTTP 500 with OperationForbidden: the role holds no filter.get' -- \
    refused 500 OperationForbidden filter-role.out
  check search-body 0 'the body of the search for the label env=prod' -- search_body
  check search-control 0 '#702: the key of the example searches at tenant scope' -- \
    "$live/api.sh" tenant-post-status "$SEARCH" search-body.json
  check search-control-says 0 '#702: the search names cluster A in its project' -- holds_cluster search-control.out
  check search-role-tenant 0 '#702: the key of the role searches at tenant scope' -- \
    with_role "$live/api.sh" tenant-post-status "$SEARCH" search-body.json
  check search-role-tenant-says 0 '#702: Palette answers HTTP 403 with AccessDenied: the role holds no project.list in the tenant' -- \
    refused 403 AccessDenied search-role-tenant.out
  check search-role-project 0 '#702: the key of the role searches the project of cluster A' -- \
    with_role "$live/api.sh" post-status "$SEARCH" search-body.json
  check search-role-project-says 0 '#702: the search of the project names cluster A, in its project' -- \
    holds_cluster search-role-project.out
fi

if stage 3 "list clusters with the key of the role"; then
  check list-role-project 0 '#702: list clusters --tag env=prod --project with the key of the role' -- \
    with_role "$strata" list clusters --tag env=prod --project "$P" --json
  check list-role-project-row 0 '#702: the list names cluster A in its project' -- \
    jq -e --arg c "$C" --arg p "$P" '.projectUid == $p and ([.clusters[] | select(.uid == $c and .projectUid == $p)] | length) == 1' list-role-project.out
  check list-role-tenant 2 '#702: list clusters --tag env=prod at tenant scope with the key of the role fails' -- \
    with_role "$strata" list clusters --tag env=prod --json
  check list-role-filter 2 "#702: list clusters --filter $FILTER with the key of the role fails: the filters need a tenant role" -- \
    with_role "$strata" list clusters --filter "$FILTER" --project "$P" --json
fi

# observer_records: a record of each profile of cluster A, by its uid, gzipped as the Secrets of the records hold it
observer_records() {
  mkdir -p records work state
  "$strata" list clusters --detail --cluster "$C" --json \
    | jq -r --arg c "$C" '.clusters[] | select(.uid == $c) | .profiles[].uid' > profiles.txt || return 2
  [ -s profiles.txt ] || return 1
  while read -r uid; do
    "$strata" snapshot "$uid" -o "records/$uid.json" || return 2
    gzip -f "records/$uid.json" || return 2
  done < profiles.txt
  echo '{"baseline":"approved","revision":"live"}' > records/meta.json
}

# observer_once: one content cycle and one versions cycle of the observer for tag:env=prod. The key of
# the role is the key of the project of cluster A.
observer_once() {
  ( umask 077 && mkdir -p keys && printf %s "$OBSERVER_ROLE_KEY" > keys/api-key ) || return 2
  "$observer_bin" once --strata "$strata" --clusters tag:env=prod --keys keys --cluster-uid "$C" \
    --records records --work work --state state > once.json 2> once.err
  local code=$?
  rm -rf keys
  [ "$code" -le 1 ]
}

if stage 4 "the observer of the set with the key of the role"; then
  [ -x "$observer_bin" ] || die "no observer binary at $observer_bin; run just observer-build"
  check observer-records 0 'a record of each profile of cluster A' -- observer_records
  check observer-once 0 '#702: one cycle of the observer with --clusters tag:env=prod and the key of the role' -- observer_once
  check observer-row 0 '#1003: the row of cluster A names its project and has checkedAt. Its status is the status of its content, also with a cause of Not checked' -- \
    jq -e --arg c "$C" --arg p "$P" '[.clusters[] | select(.uid == $c and .project.uid == $p and .checkedAt != null and .status != "not-checked")] | length == 1' once.json
  check observer-selector 0 '#702: the selector tag:env=prod selects cluster A' -- \
    jq -e '.selectors == [{"selector": "tag:env=prod", "clusters": 1}]' once.json
  check observer-no-key 0 '#702: no file of the run holds the key of the role' -- \
    bash -c "! grep -rqF -- \"\$OBSERVER_ROLE_KEY\" ."
fi

if stage 5 "the baseline"; then
  labels=$(jq -c '.source.cluster.labels' a.json)
  check unlabel 0 'cluster A gets its labels of stage 1 again, through the API' -- \
    "$live/api.sh" patch "/v1/spectroclusters/$C/metadata" "{\"metadata\": {\"labels\": $labels}}"
  [ -z "$F" ] || check filter-delete 0 "delete the filter $FILTER" -- "$live/api.sh" filter-delete "$F"
  check baseline 0 'the example is at its baseline: the labels of cluster A are the tags of the example again' -- "$live/baseline.sh"
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

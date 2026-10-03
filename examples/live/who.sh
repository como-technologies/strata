#!/usr/bin/env bash
# The audit log test (#69): what `verify --who` reads from the audit log of Palette, for an edit
# of a published version through the API, and for an update that a cluster accepts.
#
#   examples/live/who.sh [FROM_STAGE]
#
#   1  record the add-on version, edit it in place through the API, and read who made the edit;
#      then undo the edit
#   2  with a cluster: record cluster A, edit the add-on version it runs, accept the update on the
#      cluster through the API, and read who wrote to the cluster; then undo both. Without a
#      cluster, the stage records nothing and says so
#
# Stage 1 needs the profiles (just live-profiles) and no cluster: no cost. Stage 2 needs
# just live-up, and uses the cluster that is there. The script makes one edit at a time and
# undoes it, so it leaves the example as it found it. Needs the strata release binary, jq, and
# the key as verify.sh needs it. Results go under runs/<UTC time>/who/, or OUT; FROM_STAGE resumes
# a run in OUT. Exit 0 only when no check fails.
set -uo pipefail
script=who
from=${1:-1}
case "$from" in 1|2) ;; *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;; esac
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

addon=strata-live-addon
edit_from='edited: "no"'
edit_to='edited: "yes"'

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/who}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  {
    echo "# audit log test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, add-on"
    echo "\`$addon@1.0.0\` (\`${AD:-}\`), cluster \`${C:-none}\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

save() { echo "$1=$2" >> ids.env; }
[ -f ids.env ] && . ./ids.env
stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

# undo_edit: put the edit back, when the add-on version differs from its record
undo_edit() {
  "$strata" compare addon.json "$AD" > /dev/null 2>&1 \
    || "$live/api.sh" edit "$AD" "$edit_to" "$edit_from" >> steps.log 2>&1
}

if stage 1 "an edit of a published version through the API"; then
  AD=$("$strata" list --json 2> /dev/null | jq -r --arg n "$addon" '.profiles[] | select(.name == $n and .version == "1.0.0") | .uid' | head -1)
  [ -n "$AD" ] || die "no $addon@1.0.0 in the project; run just live-profiles"
  save AD "$AD"
  check record 0 "record $addon@1.0.0" -- "$strata" snapshot "$AD" -o addon.json
  check unchanged-who 0 '#69: an unchanged record reads no audit log, and prints no who line' -- \
    "$strata" verify addon.json --who
  check unchanged-no-line 1 '#69: the output has no who line' -- grep -q '^who:' unchanged-who.out
  check api-edit 0 "#69: edit $addon@1.0.0 in place through the API" -- \
    "$live/api.sh" edit "$AD" "$edit_from" "$edit_to"
  check edit-who 1 '#69: the record reports DRIFT, and --who reads the audit log of the profile' -- \
    "$strata" verify addon.json --who
  check edit-who-says 0 '#69: the output names the writes since the record' -- \
    grep -q '^who: .* in the audit log since the record' edit-who.out
  check edit-who-person 0 '#69: the last write is an update by a person' -- \
    grep -Eq '^  [0-9TZ:.-]+  .+ \(user\)  update(: .+)?$' <(grep -E '^  [0-9]{4}-' edit-who.out | tail -1)
  check edit-who-json 0 '#69: --json gives verify.who, last an update by a user with a uid, logged after the record' -- \
    bash -c "\"$strata\" verify addon.json --who --json | jq -e --arg at \"\$(jq -r .source.fetchedAt addon.json)\" \
      '.verify.who[-1] | .actorType == \"user\" and .action == \"update\" and (.actorUid | length) > 0 and .time >= \$at'"
  check edit-who-resource 0 '#140: each entry names the object of the write: the profile, with its kind and its uid' -- \
    bash -c "\"$strata\" verify addon.json --who --json | jq -e --arg u $AD \
      '.verify.who | length > 0 and all(.resource.kind == \"clusterprofile\" and .resource.uid == \$u)'"
  check edit-last-modified 0 '#140: the document gives the time of the edit: verify.lastModified is after the record, and the verdict ends with it' -- \
    bash -c "\"$strata\" verify addon.json --json | jq -e --arg at \"\$(jq -r .source.fetchedAt addon.json)\" '.verify.lastModified >= \$at' \
      && grep -Eq '^DRIFT: .*; Palette last changed it [0-9TZ:.-]+$' edit-who.out"
  check audits-none 0 '#140: Palette answers the log of an object with no entry with an empty list, and not 404' -- \
    bash -c "\"$live/api.sh\" get '/v1/audits?resourceKind=clustergroup&resourceUid=$AD&limit=100' | jq -e '(.items // []) | length == 0'"
  check audits-limit 0 '#188: Palette serves at most 100 entries on a page: a limit of 101 answers 500 with InvalidLimit' -- \
    bash -c "\"$live/api.sh\" get '/v1/audits?resourceUid=$AD&limit=101' 2>&1 | grep -q 'HTTP 500: .*InvalidLimit'"
  check api-undo 0 '#69: undo the edit through the API' -- \
    "$live/api.sh" edit "$AD" "$edit_to" "$edit_from"
  check restored 0 '#69: the record verifies again after the undo' -- "$strata" verify addon.json
fi

if stage 2 "an update that a cluster accepts"; then
  load_outputs 2> /dev/null || outputs='{}'
  C=$(out cluster_uid)
  if [ -z "$C" ]; then
    echo "no cluster: just live-up makes one; stage 2 records nothing" | tee -a steps.log
  else
    save C "$C"
    check cluster-record 0 'record cluster A' -- "$strata" snapshot "cluster:$C" -o cluster.json
    check cluster-edit 0 "#69: edit $addon@1.0.0, which cluster A runs, through the API" -- \
      "$live/api.sh" edit "$AD" "$edit_from" "$edit_to"
    check cluster-apply 0 '#69: accept the update on cluster A through the API' -- \
      "$live/api.sh" apply "$C"
    # the cluster takes the content some time after the request
    for _ in $(seq 1 30); do
      "$strata" verify cluster.json > /dev/null 2>&1
      [ $? = 1 ] && break
      sleep 30
    done
    check cluster-who 1 '#69: the cluster record reports DRIFT, and --who reads the audit log of the cluster' -- \
      "$strata" verify cluster.json --who
    check cluster-who-says 0 '#69: the output names the writes to the cluster since the record' -- \
      grep -q '^who: .* in the audit log since the record' cluster-who.out
    check cluster-who-person 0 '#69: a person updated the cluster' -- \
      grep -Eq '^  [0-9TZ:.-]+  .+ \(user\)  update' cluster-who.out
    check cluster-who-json 0 '#69: --json gives verify.who for the cluster' -- \
      bash -c "\"$strata\" verify cluster.json --who --json | jq -e '.verify.who | length > 0'"
    undo_edit
    "$live/api.sh" apply "$C" >> steps.log 2>&1
    for _ in $(seq 1 30); do
      "$strata" verify cluster.json > /dev/null 2>&1 && break
      sleep 30
    done
    check cluster-restored 0 '#69: cluster A runs the recorded content again after the undo' -- \
      "$strata" verify cluster.json
  fi
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

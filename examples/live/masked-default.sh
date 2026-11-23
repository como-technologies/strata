#!/usr/bin/env bash
# The masked default test (#997): a move of cluster A to a version with a required sensitive
# variable with a default value. The variables route of the version gives ******** in place of
# the default value. Strata reads the mask as a default value, so it names no required variable
# for the move (#912). The test shows what Palette takes at the update, and what the cluster
# then holds.
#
#   examples/live/masked-default.sh [FROM_STAGE]
#
#   1  a new version of the add-on profile, through the API: the packs of the add-on version, and
#      the variable strata_sensitive, which is required and sensitive and has a default value.
#      Its manifest reads the variable. The variables route of the version gives the mask in
#      place of the default value
#   2  compare --validate of cluster A and the version names no required variable. The update
#      puts the version in the place of the add-on version, and gives the variable no value.
#      Palette takes the update, and cluster A runs the version
#   3  what cluster A holds: the cluster variables route gives the mask for the value of the
#      variable, and the ConfigMap of the manifest holds the default value. The script reads the
#      ConfigMap inside the cluster until it holds the default value, or PROPAGATION_MINUTES pass
#      (default 20)
#   4  undo: cluster A back on the add-on version, the version deleted, and the baseline
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# jq, kubectl, and the key as verify.sh needs it. The kubeconfig lives in a temporary file and is
# never saved. A check that fails stops no stage, so stage 4 also runs after a failed check.
# Results go under runs/<UTC time>/masked-default/, or OUT; FROM_STAGE resumes a run in OUT, and
# `masked-default.sh 4` removes what an earlier run left. Exit 0 only when no check fails.
set -uo pipefail
script=masked-default
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"
command -v kubectl > /dev/null || die "kubectl is needed to read the value inside the cluster"

# The variable of the version, its default value, and the version. No other script makes this
# version of the add-on profile.
variable=strata_sensitive
default="the default that the profile holds"
version=1.0.97
fields=$(jq -cn --arg d "$default" '{displayName: "strata sensitive", required: true, isSensitive: true, defaultValue: $d}')
reads_variable="edited: \"{{.spectro.var.$variable}}\""
minutes=${PROPAGATION_MINUTES:-20}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/masked-default}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  write_summary "masked default test" ", cluster $(md span "${C:-}")," \
    "add-on $(md span "${AD:-}"), version $(md span "${N:-}")."
}
trap summary EXIT

load_outputs
C=$(out cluster_uid) AD=$(out addon_profile_uid) P=$(out project_uid)
addon=$(out addon_profile_name)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$P
ref=$addon@$version

# made: the uid of the version that stage 1 makes, or empty when the project has no such version
made() {
  "$strata" list profiles --json | jq -r --arg n "$addon" --arg v "$version" \
    '[.profiles[] | select(.name == $n and .version == $v) | .uid] | first // empty'
}
# on_cluster UID: the document of cluster A names the profile version UID
on_cluster() {
  "$live/api.sh" get "/v1/spectroclusters/$C" | jq -e --arg u "$1" 'any(.spec.clusterProfileTemplates[]; .uid == $u)' > /dev/null
}
# runs UID: wait up to five minutes until the document of cluster A names the profile version UID
runs() {
  for _ in $(seq 1 20); do
    on_cluster "$1" && return 0
    sleep 15
  done
  return 1
}
# inside: the value of the ConfigMap that the manifest makes, read inside the cluster
inside() { kube "$C" get configmap strata-live -n default -o jsonpath='{.data.edited}'; }
# inside_is WANT: read the ConfigMap until it holds WANT, at most `minutes` minutes. The value can
# be the value of a sensitive variable, so no line prints it. Each read writes the case that it
# saw to propagation.log: WANT, the mask, or another value
inside_is() {
  local seen said
  for _ in $(seq 1 $((minutes * 2))); do
    seen=$(inside)
    case "$seen" in
      "$1") said="the value that the check wants" ;;
      "********") said="the mask" ;;
      *) said="another value, or no answer" ;;
    esac
    echo "$(date -u +%H:%M:%S) the ConfigMap holds $said" >> propagation.log
    [ "$seen" = "$1" ] && return 0
    sleep 30
  done
  return 1
}

if stage 1 "a version with a required sensitive variable with a default value"; then
  require_baseline
  check masked-version 0 "#997: $ref through the API, with the variable $variable: required, sensitive, and with a default value" -- \
    "$live/api.sh" variable-copy "$AD" "$addon" "$variable" "$addon_from" "$reads_variable" "$version" "$fields"
  [ -s masked-version.out ] || die "Palette made no version $ref; see $PWD/masked-version.err"
  check masked-route 0 "#997: the variables route of $ref" -- \
    "$live/api.sh" get "/v1/clusterprofiles/$(cat masked-version.out)/variables"
  check masked-route-mask 0 '#997: the route gives the mask in place of the default value, and the variable is required and sensitive' -- \
    jq -e --arg v "$variable" '[.variables[] | select(.name == $v) | {required, isSensitive, defaultValue}]
      == [{required: true, isSensitive: true, defaultValue: "********"}]' masked-route.out
fi

N=$(made)

if stage 2 "the move of cluster A to $ref"; then
  [ -n "$N" ] || die "the project has no version $ref; run stage 1"
  check masked-validate '?' "#997: compare --validate asks the route about the move of cluster A to $ref" -- \
    "$strata" compare "cluster:$C" "$ref" --validate --json
  check masked-asked 0 '#997: the comparison holds an answer of the route' -- \
    jq -e '.paletteValidation | has("accepts")' masked-validate.out
  check masked-says 0 '#912, #997: strata names no required variable, because it reads the mask as a default value' -- \
    jq -e '.updateChecks.requiredVariables == null' masked-validate.out
  check masked-update 0 "#997: the update of cluster A from $AD to $N, which gives the variable no value" -- \
    "$live/api.sh" replace-status "$C" "$AD" "$N"
  check masked-taken 0 '#997: Palette takes the update with HTTP 204, and does not refuse it with InvalidVariable' -- \
    answered 204 masked-update.out
  check masked-runs 0 "#997: cluster A runs $ref" -- runs "$N"
fi

if stage 3 "what cluster A holds"; then
  check masked-cluster 0 '#997: the cluster variables route of cluster A' -- \
    "$live/api.sh" get "/v1/spectroclusters/$C/variables"
  check masked-cluster-value 0 '#997: the route gives the mask for the value of the variable, so the cluster gives it a value' -- \
    jq -e --arg v "$variable" '[.. | objects | select(.name? == $v) | .value] == ["********"]' masked-cluster.out
  check masked-inside 0 "#997: the ConfigMap inside cluster A holds the default value of the profile, and not the mask, within $minutes minutes (propagation.log)" -- \
    inside_is "$default"
fi

if stage 4 "undo"; then
  # cluster A goes back only when it runs the version: a refused update moved nothing
  if [ -n "$N" ] && on_cluster "$N"; then
    check masked-back 0 "the update of cluster A from $N back to $AD" -- "$live/api.sh" replace-status "$C" "$N" "$AD"
    check masked-back-taken 0 'Palette takes the update with HTTP 204' -- answered 204 masked-back.out
    check masked-back-runs 0 'cluster A runs the add-on version again' -- runs "$AD"
    check masked-back-inside 0 "the ConfigMap inside cluster A holds the value of the add-on version again, within $minutes minutes (propagation.log)" -- \
      inside_is no
  fi
  [ -z "$N" ] || check masked-delete 0 "$ref deleted" -- "$live/api.sh" delete "$N"
  check masked-baseline 0 'the example is at its baseline' -- "$live/baseline.sh"
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

#!/usr/bin/env bash
# The least role of the key of the observer (#645). The script sends each request of the two cycles of
# the observer twice: with the key of the example, and with a second key. It records the status and the
# error code of each answer. Then it runs the commands of the cycles with both keys.
#
#   OBSERVER_ROLE=NAME OBSERVER_ROLE_KEY=KEY examples/live/observer-role.sh [FROM_STAGE]
#
#   1  the requests: the content cycle reads cluster A, its profile, and the audit log. The versions
#      cycle reads the registries and the packs. The click "Ask Palette" sends one more request: POST
#      validate/packs for the add-on version (#647). A request with the key of the example must answer
#      2xx. With the second key, the check records the answer and does not judge it
#   2  the commands of the cycles: verify, snapshot of the cluster, and behind, with both keys
#   3  the palette tier of preflight on cluster A, with both keys (#619). The validate route of the
#      create needs cluster.create, also for a body that creates nothing
#
# The role of the second key is the question, so a person makes the user once. Making a user in the
# tenant is for a maintainer. The steps:
#   1. As an admin of the tenant, make a user with no tenant role.
#   2. Give the user the role under test in the project of the example, such as Project Viewer or
#      Cluster Viewer.
#   3. Make an API key as that user.
#   4. Run: OBSERVER_ROLE='Project Viewer' OBSERVER_ROLE_KEY=<the key> just live-observer-role
#   5. Give the user the next role, and run the script again.
# OBSERVER_ROLE names the role, for the summary. The key stays in the environment of one command at a
# time, and no file of the run holds it. With no key, the script says "could not check" and exits 2.
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# and jq. The script changes nothing. Its POSTs go to validate routes, which only check.
# Results go under runs/<UTC time>/observer-role/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0
# only when no check fails.
set -uo pipefail
script=observer-role
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"
[ -n "${OBSERVER_ROLE_KEY:-}" ] \
  || die "could not check: set OBSERVER_ROLE_KEY to the key of a user with the role under test, and OBSERVER_ROLE to its name"
[ -n "${OBSERVER_ROLE:-}" ] || die "could not check: set OBSERVER_ROLE to the name of the role of the key"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/observer-role}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  write_summary "observer role test" ", cluster \`${C:-}\`, role \`$OBSERVER_ROLE\`."
}
trap summary EXIT

load_outputs
C=$(out cluster_uid) AD=$(out addon_profile_uid) P=$(out project_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$P
registry=$(registry_uid "$public_repo")
[ -n "$registry" ] || die "the key of the example cannot see the registry $public_repo"

# route_status KEY PATH [BODY_FILE]: print the HTTP status and the error code of the answer to GET PATH.
# With BODY_FILE, the request is a POST of that body. An empty KEY is the key of the example. It
# succeeds when the status is 2xx. The key reaches api.sh in the environment of one call.
route_status() {
  local answer status call=(get-status "$2")
  [ -z "${3:-}" ] || call=(post-status "$2" "$3")
  answer=$( { [ -z "$1" ] || export PALETTE_API_KEY=$1; "$live/api.sh" "${call[@]}"; } ) || return 2
  status=$(head -n 1 <<<"$answer")
  printf '%s %s\n' "$status" "$(sed 1d <<<"$answer" | jq -r '.code // empty' 2> /dev/null)"
  [ "${status:5:1}" = 2 ]
}

# The requests of the cycles, as the book lists them in Palette API requests. NAME PATH.
routes() {
  cat << EOF
cluster /v1/spectroclusters/$C
cluster-manifests /v1/spectroclusters/$C/profiles/packs/manifests
cluster-variables /v1/spectroclusters/$C/variables
cluster-rbacs /v1/spectroclusters/$C/config/rbacs
cluster-namespaces /v1/spectroclusters/$C/config/namespaces
notifications /v1/notifications/spectrocluster/$C
profile /v1/clusterprofiles/$AD
profile-manifests /v1/clusterprofiles/$AD/packs/manifests
profile-variables /v1/clusterprofiles/$AD/variables
audits /v1/audits?resourceKind=clusterprofile&resourceUid=$AD&startTime=2020-01-01T00:00:00Z&limit=100
registries /v1/registries/metadata
registry-pack-summary /v1/registries/pack/summary
registry-helm-summary /v1/registries/helm/summary
registry-oci-summary /v1/registries/oci/summary
packs /v1/packs?filters=spec.name=$addon_pack&limit=100&offset=0
pack-tags /v1/packs/$addon_pack/registries/$registry
EOF
}

# validate_body: the body of POST validate/packs for the add-on version, made from its record. It has
# the cloud type, the type, and each pack with the tag, the uids, the type, the layer, the values,
# and the manifests. The click of the observer sends a body of this shape.
validate_body() {
  "$strata" snapshot "$AD" -o addon.json || return 1
  jq -c '{cloudType: .source.cloudType, type: .source.type, packs: [.packs[] | {name, tag, uid: .packUid,
    registryUid, type, layer, values, manifests: [(.manifests // [])[] | {name, content}]}]}' addon.json > validate-body.json
}

if stage 1 "the requests of the cycles"; then
  while read -r name path; do
    check "control-$name" 0 "#645: the key of the example reads $path" -- route_status "" "$path"
    check "role-$name" ? "#645: the answer to GET $path with the key of the role $OBSERVER_ROLE" -- route_status "$OBSERVER_ROLE_KEY" "$path"
  done < <(routes)
  validate_path=/v1/clusterprofiles/$AD/validate/packs
  check validate-body 0 '#647: the body of the validate request, made from the record of the add-on version' -- validate_body
  check control-validate-packs 0 "#647: the key of the example sends POST $validate_path, and Palette answers 2xx" -- \
    route_status "" "$validate_path" validate-body.json
  check role-validate-packs ? "#647: the answer to POST $validate_path with the key of the role $OBSERVER_ROLE" -- \
    route_status "$OBSERVER_ROLE_KEY" "$validate_path" validate-body.json
fi

# with_role COMMAND...: run COMMAND with the key of the role
with_role() { PALETTE_API_KEY=$OBSERVER_ROLE_KEY "$@"; }

if stage 2 "the commands of the cycles"; then
  check command-record 0 'record the add-on version with the key of the example' -- "$strata" snapshot "$AD" -o addon.json
  check command-control-verify 0 '#645: verify --who with the key of the example: the record verifies' -- "$strata" verify addon.json --who
  check command-role-verify ? "#645: the same verify with the key of the role $OBSERVER_ROLE" -- with_role "$strata" verify addon.json --who
  check command-control-cluster 0 '#645: a record of cluster A with the key of the example' -- "$strata" snapshot "cluster:$C" -o cluster.json
  check command-role-cluster ? "#645: the same record with the key of the role $OBSERVER_ROLE" -- with_role "$strata" snapshot "cluster:$C" -o cluster-role.json
  check command-control-behind ? '#645: behind with the key of the example (0 or 1)' -- "$strata" behind addon.json --no-impact --json --compact
  check command-role-behind ? "#645: the same behind with the key of the role $OBSERVER_ROLE" -- with_role "$strata" behind addon.json --no-impact --json --compact
fi

# preflight_validate KEY FILE: run the rules of the validate route of the create on cluster A, with KEY,
# and write the report to FILE. An empty KEY is the key of the example. Print the verdict and the
# first message of each check. It succeeds when no check gave an error.
preflight_validate() {
  ( [ -z "$1" ] || export PALETTE_API_KEY=$1
    "$strata" preflight "cluster:$C" --tier palette --rule palette-validate --rule worker-cpu-floor \
      --rule pack-dependency --json --quiet > "$2" )
  jq -r '.checks[] | "\(.id) \(.verdict): \((.messages // [])[0] // "")"' "$2" || return 2
  jq -e '[.checks[] | select(.verdict == "error")] | length == 0' "$2" > /dev/null
}

if stage 3 "the palette tier of preflight"; then
  check preflight-control-validate 0 '#619: with the key of the example, the rules of the validate route of the create give no error' -- \
    preflight_validate "" preflight-control.json
  check preflight-role-validate ? "#619: the same rules with the key of the role $OBSERVER_ROLE: the route needs cluster.create" -- \
    preflight_validate "$OBSERVER_ROLE_KEY" preflight-role.json
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

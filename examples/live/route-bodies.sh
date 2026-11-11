#!/usr/bin/env bash
# The case bodies of validate-route.sh on AWS (#408). Four cases of the cluster validate route do
# not depend on the cloud. Each case needs a body of its own, because its fault is in a profile.
# This script writes those bodies, and runs validate-route.sh on them (#413).
#
#   examples/live/route-bodies.sh BODIES
#
# BODIES is the directory for `validate-route.sh BODIES`. The script makes it when it is not
# there. It writes the clean body of a create on AWS, from the release profile and the cloud
# account of terraform.tfvars. It also writes one body for each case whose profile Palette takes. A
# case body is the clean body with one more profile:
#
#   aws-clean.json           the body that the route must take
#   pack-state-disabled      an add-on profile with the pack gloo, which carries the mark disabled
#   pack-state-deleted       an add-on profile with a pack version in the state deleted
#   profile-cloud-mismatch   an add-on profile of the cloud type gcp
#   pack-cloud-support       an add-on profile with a pack version whose cloud types lack aws
#
# The script makes each profile through the API, with the name strata-route-<case>, and publishes
# it. A case body names its profile, and Palette answers 404 `ResourceNotFound` for a profile that
# is gone. So the script runs `validate-route.sh BODIES` itself, while the profiles are there.
# Only then does it delete each profile and the body of each case. It does the same when a step
# fails, and when a signal stops it. A later `validate-route.sh BODIES` skips the four cases,
# because no body of a case stays in BODIES. aws-clean.json stays: it names the release profile.
# The results of validate-route.sh go under validate-route/ in the results of this script.
#
# Palette can refuse a profile, and the registry can lack a pack in the state of a case. The
# script then skips that case. steps.log says why, and no body of the case stays in BODIES. The
# create of a profile refuses a disabled pack and a deleted pack. So the two cases of pack-state
# stay skipped, unless a profile holds a pack that Palette disables or deletes later. The script
# creates no content in a registry. For pack-cloud-support, it reads the pack listing of Public
# Repo for the first pack in the layer addon whose cloud types lack aws.
# PACK_CLOUD_SUPPORT=<pack>@<version> names that pack instead.
#
# One more case makes no profile (#816). cidr-overlap.json is the clean body with the first VPC
# of the region that has a CIDR. It also holds the values of the Kubernetes pack of the release
# profile, with that CIDR as the pod CIDR. The script sends it to the route with api.sh
# post-status, and records the status and the code. The code of Palette refuses it with HTTP 500
# and K8SCidrOverlap. cidr-overlap.json stays, as aws-clean.json does. The script skips the case
# when Palette lists no VPC with a CIDR, or the Kubernetes pack has no podCIDR.
#
# Needs the release profile of the live example on AWS (just live-profiles), the cloud account of
# terraform.tfvars, and the key as verify.sh needs it. It builds no cluster, and costs nothing
# that stays. Results go under runs/<UTC time>/route-bodies/, or OUT. Exit 0 only when no check
# fails, and that includes the checks of validate-route.sh.
set -uo pipefail
script=route-bodies
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"
case "${1:-}" in
  -h | --help) usage; exit 0 ;;
  "") usage >&2; exit 2 ;;
esac
mkdir -p "$1" && bodies=$(cd "$1" && pwd) || die "no directory $1"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/route-bodies}
mkdir -p "$run" && cd "$run" || exit 2

# The profiles that this run made, one "CASE UID" on a line. cleanup deletes them and the body of
# each case, and empties the list. A body that stays would name a profile that is gone.
made=""
cleanup() {
  local todo=$made id uid
  made=""
  while read -r id uid; do
    [ -n "$id" ] || continue
    rm -f "$bodies/$id.json"
    check "$id-deleted" 0 "#408: the profile of $id is deleted" -- "$live/api.sh" delete "$uid"
  done <<<"$todo"
}

summary() {
  write_summary "case bodies of the cluster validate route" ": bodies in \`$bodies\`."
}
finish() { cleanup; summary; }
trap finish EXIT
trap 'exit 130' INT TERM

load_outputs
export PALETTE_PROJECT_UID=$(out project_uid)
R=$(out release_profile_uid)
[ -n "$R" ] || die "no release profile; run just live-profiles first"
"$live/api.sh" get "/v1/clusterprofiles/$R" > release.json || die "cannot read the release profile $R"
profile_cloud=$(jq -r '.spec.published.cloudType // empty' release.json)
[ "$profile_cloud" = aws ] || die "the release profile $R is of cloud type ${profile_cloud:-unknown}, not aws: apply with -var cloud=aws"
public_uid=$(registry_uid "$public_repo")
[ -n "$public_uid" ] || die "the key cannot see the registry $public_repo"

echo "== the clean body of a create on AWS (#408)" | tee -a steps.log
create_body aws "$R" strata-route-aws "$bodies/aws-clean.json"
check aws-clean-body 0 "#408: aws-clean.json names the account $account_name and the release profile $R" -- \
  jq -e --arg a "$A" --arg r "$R" '.spec.cloudAccountUid == $a and .spec.profiles[0].uid == $r' "$bodies/aws-clean.json"

# refused STATUS CODE FILE: FILE, the output of api.sh post-status, holds HTTP STATUS and an
# answer with the code CODE
refused() {
  [ "$(head -n 1 "$3")" = "HTTP $1" ] && tail -n +2 "$3" | jq -e --arg c "$2" '.code == $c' > /dev/null
}

# cidr_body: write BODIES/cidr-overlap.json, send it to the route, and record the answer. Skip the
# case when Palette lists no VPC with a CIDR, or the Kubernetes pack has no podCIDR.
cidr_body() {
  local vpc cidr
  echo "== cidr-overlap: the pod CIDR of the Kubernetes pack is the CIDR of the VPC (#816)" | tee -a steps.log
  rm -f "$bodies/cidr-overlap.json"
  if ! "$live/api.sh" get "/v1/clouds/aws/regions/$region/vpcs?cloudAccountUid=$A" > vpcs.json 2> vpcs.err; then
    echo "skipped cidr-overlap: Palette lists no VPC of $region: $(head -n 1 vpcs.err)" | tee -a steps.log
    return
  fi
  vpc=$(jq -r '[.vpcs[]? | select((.cidrBlock // "") != "")][0].vpcId // empty' vpcs.json)
  [ -n "$vpc" ] || { echo "skipped cidr-overlap: the region $region lists no VPC with a CIDR" | tee -a steps.log; return; }
  cidr=$(jq -r --arg v "$vpc" '[.vpcs[] | select(.vpcId == $v)][0].cidrBlock' vpcs.json)
  if ! jq --arg v "$vpc" --arg c "$cidr" --slurpfile p release.json '
      [$p[0].spec.published.packs[]? | select(.layer == "k8s" and ((.values // "") | test("podCIDR:")))][0] as $k
      | if $k == null then error("the Kubernetes pack of the release profile has no podCIDR") else . end
      | .spec.cloudConfig.vpcId = $v
      | .spec.profiles[0].packValues = [{name: $k.name, tag: $k.tag,
          values: ($k.values | sub("podCIDR:[^\n]*"; "podCIDR: \"\($c)\""))}]' \
      "$bodies/aws-clean.json" > "$bodies/cidr-overlap.json" 2> cidr-overlap.err; then
    rm -f "$bodies/cidr-overlap.json"
    echo "skipped cidr-overlap: $(head -n 1 cidr-overlap.err)" | tee -a steps.log
    return
  fi
  check cidr-overlap-body 0 "#816: cidr-overlap.json names the VPC $vpc, and its Kubernetes pack has the pod CIDR $cidr" -- \
    jq -e --arg v "$vpc" --arg c "$cidr" \
      '.spec.cloudConfig.vpcId == $v and (.spec.profiles[0].packValues[0].values | contains("podCIDR: \"\($c)\""))' \
      "$bodies/cidr-overlap.json"
  check cidr-overlap-status 0 "#816: the route answers the body of cidr-overlap; the output holds the status and the answer" -- \
    "$live/api.sh" post-status /v1/spectroclusters/aws/validate "$bodies/cidr-overlap.json"
  check cidr-overlap-code 0 "#816: the route refuses the body with HTTP 500 and K8SCidrOverlap" -- \
    refused 500 K8SCidrOverlap cidr-overlap-status.out
}
cidr_body

# case_body CASE NAME PACK VERSION [CLOUD]: make and publish the add-on profile NAME with PACK at
# VERSION from the registry of the lab, of the cloud type CLOUD or all. Then write BODIES/CASE.json:
# the clean body and that profile. When Palette refuses the profile, skip the case.
case_body() {
  local id=$1 name=$2 pack=$3 version=$4 cloud=${5:-all} uid
  echo "== $id: the profile $name with $pack@$version" | tee -a steps.log
  # a body of an earlier run names a profile that is gone. A case that is skipped has no body.
  rm -f "$bodies/$id.json"
  "$live/api.sh" delete-named "$name" > /dev/null || die "cannot delete the profile $name of an earlier run"
  if ! uid=$("$live/api.sh" pin-profile "$name" "$public_uid" spectro "$pack" "$version" "" addon "" "$cloud" 2> "$id.err"); then
    # the create can pass and the publish fail, and the profile then stays
    "$live/api.sh" delete-named "$name" > /dev/null 2>&1
    echo "skipped $id: Palette refuses the profile with $pack@$version: $(head -n 1 "$id.err")" | tee -a steps.log
    return
  fi
  made+="$id $uid"$'\n'
  jq --arg u "$uid" '.spec.profiles += [{uid: $u, packValues: [], variables: []}]' "$bodies/aws-clean.json" > "$bodies/$id.json" ||
    { rm -f "$bodies/$id.json"; die "cannot write $bodies/$id.json"; }
  check "$id-body" 0 "#408: $id.json holds the release profile and $name" -- \
    jq -e --arg r "$R" --arg u "$uid" '[.spec.profiles[].uid] == [$r, $u]' "$bodies/$id.json"
}

# pack_without CLOUD: print NAME@VERSION of the first pack in Public Repo, in the layer addon, whose
# cloud types hold neither CLOUD nor all. A pack with no cloud type supports each cloud. Exit 1
# when the listing has none.
pack_without() {
  local pack
  pack=$("$live/api.sh" packs-where --arg r "$public_uid" --arg c "$1" \
    '.spec.registryUid == $r and .spec.layer == "addon"
       and ((.spec.cloudTypes // []) | length > 0 and index("all") == null and index($c) == null)') || return
  jq -r '"\(.spec.name)@\(.spec.version)"' <<<"$pack"
}

echo "== the four cases of the profiles (#408)" | tee -a steps.log
case_body pack-state-disabled strata-route-pack-disabled "$marked" "$marked_version"
case_body pack-state-deleted strata-route-pack-deleted "$deleted" "$deleted_version"
case_body profile-cloud-mismatch strata-route-cloud-mismatch "$addon_pack" "$addon_pack_version" gcp
cloud_pack=${PACK_CLOUD_SUPPORT:-$(pack_without aws 2> pack-cloud-support.err)}
if [ -n "$cloud_pack" ]; then
  case_body pack-cloud-support strata-route-cloud-support "${cloud_pack%@*}" "${cloud_pack#*@}"
else
  rm -f "$bodies/pack-cloud-support.json"
  echo "skipped pack-cloud-support: $public_repo lists no pack in the layer addon without aws; give PACK_CLOUD_SUPPORT=<pack>@<version>. $(head -n 1 pack-cloud-support.err)" |
    tee -a steps.log
fi

# A body names a profile, and Palette answers 404 for a profile that is gone. So the profiles stay
# until validate-route.sh has sent each body (#413).
echo "== validate-route.sh on the bodies, while the profiles are there (#413)" | tee -a steps.log
check validate-route 0 "#413: validate-route.sh runs on the bodies while the profiles of the cases are there" -- \
  env OUT="$PWD/validate-route" "$live/validate-route.sh" "$bodies"

cleanup
echo "route-bodies: $failures check(s) failed; results of validate-route.sh in $run/validate-route; results in $run"
[ "$failures" = 0 ]

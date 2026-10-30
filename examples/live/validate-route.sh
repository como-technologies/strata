#!/usr/bin/env bash
# The cluster validate route test (#243): does POST /v1/spectroclusters/{cloudType}/validate refuse
# the fault that a rule of the catalog copies? A rule leaves the catalog only when the route
# answers its check. Each case below sends one fault to the route with `strata preflight --rule
# palette-validate`. The route changes nothing, and the script writes nothing to the tenant.
#
#   examples/live/validate-route.sh BODIES
#
# BODIES is a directory with the body of a create for each engine: aws-clean.json, edge-clean.json,
# eks-clean.json, gcp-clean.json, maas-clean.json, azure-clean.json, and vsphere-clean.json. The
# route must take each one. A case makes its fault with one jq edit of the body of its engine. A
# case whose body is not there is skipped, and steps.log names it.
#
# Five cases need a body of their own, BODIES/<case>.json, because the fault is in a profile:
#   edge-two-node-pxk      a two-node cluster whose Kubernetes pack is not K3s
#   pack-state-disabled    a profile with a disabled pack
#   pack-state-deleted     a profile with a deleted pack
#   profile-cloud-mismatch a profile of another cloud type
#   pack-cloud-support     a pack version that does not list the cloud type
#
# Each case has two checks. `<case>` is that the route refuses the body. `<case>-says` is that the
# refusal names the fault. A case that fails shows that the route does not answer the check, so
# its rule stays in the catalog.
#
# Needs the key as verify.sh needs it, and the cloud accounts that the bodies name. It builds no
# cluster, and costs nothing. Results go under runs/<UTC time>/validate-route/, or OUT. Exit 0
# only when no check fails.
set -uo pipefail
script=validate-route
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"
usage() { sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; }
case "${1:-}" in
  -h | --help) usage; exit 0 ;;
  "") usage >&2; exit 2 ;;
esac
bodies=$(cd "$1" 2> /dev/null && pwd) || die "no directory $1"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/validate-route}
mkdir -p "$run" && cd "$run" || exit 2

summary() {
  {
    echo "# cluster validate route test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`: bodies from \`$bodies\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

# route_says VERDICT ENGINE BODY: ask the route about BODY. The report goes to the standard output.
# Exit 0 when palette-validate has the verdict VERDICT. strata exits 1 for a failed rule, and the
# verdict is the answer.
route_says() {
  local report
  report=$("$strata" preflight "$3" --cloud "$2" --rule palette-validate --json --quiet --no-color)
  printf '%s\n' "$report"
  jq -e --arg v "$1" '[.checks[] | select(.id == "palette-validate" and .verdict == $v)] | length == 1' <<<"$report" > /dev/null
}

# names TEXT FILE: the messages of palette-validate in the report FILE match TEXT, with no case
names() {
  jq -e --arg s "$1" '[.checks[] | select(.id == "palette-validate") | (.messages // [])[]] | any(.[]; test($s; "i"))' "$2" > /dev/null
}

# clean ENGINE: the route takes the body of ENGINE. The check runs once for each engine.
clean() {
  [ -e "$1-clean.out" ] || [ ! -f "$bodies/$1-clean.json" ] && return 0
  check "$1-clean" 0 "#243: the route takes the clean body of $1" -- route_says passed "$1" "$bodies/$1-clean.json"
}

# fault CASE ENGINE SAYS EDIT: the fault of CASE is the jq program EDIT over the clean body of
# ENGINE, or the file BODIES/CASE.json when EDIT is `-`. The route must refuse it, and the refusal
# must match SAYS.
fault() {
  local id=$1 engine=$2 says=$3 edit=$4 body
  if [ "$edit" = - ]; then
    body=$bodies/$id.json
    [ -f "$body" ] || { echo "skipped $id: no $body" | tee -a steps.log; return; }
  else
    [ -f "$bodies/$engine-clean.json" ] || { echo "skipped $id: no $bodies/$engine-clean.json" | tee -a steps.log; return; }
    body=$id.json
    command jq "$edit" "$bodies/$engine-clean.json" > "$body" || die "the edit of $id failed on $bodies/$engine-clean.json"
  fi
  clean "$engine"
  check "$id" 0 "#243: the route refuses the fault of $id" -- route_says failed "$engine" "$body"
  check "$id-says" 0 "#243: the refusal of $id matches $says" -- names "$says" "$id.out"
}

echo "== the cases of the rules that copy a check of the route (#243)" | tee -a steps.log
# vsphere-instance-floor: the CPU, the memory, the disk, and the missing instance type
type_of='.spec.machinepoolconfig[0].cloudConfig.instanceType'
fault vsphere-instance-floor-cpu vsphere 'cpu' "$type_of.numCPUs = 0"
fault vsphere-instance-floor-memory vsphere 'memory|MiB' "$type_of.memoryMiB = 512"
fault vsphere-instance-floor-disk vsphere 'disk|GiB' "$type_of.diskGiB = 10"
fault vsphere-instance-floor-no-type vsphere '.' "del($type_of)"
# eks-ami-type: the AL2 type, and a type that is not known
fault eks-ami-type-al2 eks 'UnsupportedAmiType' '.spec.machinepoolconfig[0].cloudConfig.amiType = "AL2_x86_64"'
fault eks-ami-type-unknown eks 'ami' '.spec.machinepoolconfig[0].cloudConfig.amiType = "BOTTLEROCKET_x86_64"'
# edge-two-node: the Kubernetes pack that is not K3s. The body is the file of the case.
fault edge-two-node-pxk edge 'TwoNodeClusterPackMissing' -
# ssh-key-format: vSphere and Edge
fault ssh-key-format-vsphere vsphere 'SshKeyValidationFailure' '.spec.cloudConfig.sshKeys = ["ssh-rsa"]'
fault ssh-key-format-edge edge 'SshKeyValidationFailure' '.spec.cloudConfig.sshKeys = ["ssh-rsa"]'
# azure-ssh-key: the empty key
fault azure-ssh-key-empty azure 'sshKey' '.spec.cloudConfig.sshKey = ""'
# pack-state: a profile that holds a pack in the state disabled, and one in the state deleted
fault pack-state-disabled aws 'disabled' -
fault pack-state-deleted aws 'deleted' -
# profile-cloud-mismatch and pack-cloud-support: the fault is in a profile
fault profile-cloud-mismatch aws '.' -
fault pack-cloud-support gcp '.' -
# vsphere-static-ip-pool: a static network with no pool, and a DHCP cluster with a static network
network='(.spec.cloudConfig.placement.network, (.spec.machinepoolconfig[].cloudConfig.placements[]?.network))'
fault vsphere-static-ip-pool-empty vsphere 'ParentPoolUid' \
  ".spec.cloudConfig.staticIp = true | $network |= (. // {} | .parentPoolUid = \"\")"
fault vsphere-static-ip-pool-dhcp vsphere 'static' \
  ".spec.cloudConfig.staticIp = false | $network |= (. // {} | .staticIp = true)"
# vsphere-controlplane-endpoint and edge-vip-multi-node: the empty host
fault vsphere-controlplane-endpoint-no-host vsphere 'Host endpoint' '.spec.cloudConfig.controlPlaneEndpoint = {type: "VIP", host: ""}'
fault edge-vip-multi-node-no-host edge 'host|endpoint' '.spec.cloudConfig.controlPlaneEndpoint = {type: "VIP", host: ""}'
# azure-subnets-in-vnet: a vnet with no CIDR
fault azure-subnets-in-vnet-no-cidr azure 'VNet CIDR Block' \
  '.spec.cloudConfig |= (.vnetName = (.vnetName // "strata-vnet") | .vnetCidrBlock = "")'
# ntp-servers: an entry that is not an address or a name
for engine in vsphere edge maas; do
  fault "ntp-servers-$engine" "$engine" 'ntp' '.spec.cloudConfig.ntpServers = ["not a host name!"]'
done

echo "validate-route: $failures check(s) failed; results in $run"
[ "$failures" = 0 ]

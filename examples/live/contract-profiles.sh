#!/usr/bin/env bash
# The standing cluster profiles of the contract run (#531). Four facts of the contract run need a
# cluster profile in the project, and the project holds none after each `just live-down`. This
# script keeps a small set of profiles in the project, apart from the live example.
#
#   just contract-profiles up|down|status
#
# up applies the module of examples/live/terraform with the name prefix strata-contract, the cloud
# aws, no cluster, and own_registries off. So it makes cluster profiles only: no AWS resource, no
# access key, no registry, no cluster. The cluster group, the cluster template, and the registry
# kinds stay off, as their defaults say. Like live-profiles, it needs the terraform.tfvars of the
# example for the names in the tenant.
#
# The state is the file terraform/contract.tfstate. The recipes live-up and live-down read
# terraform.tfstate, so live-down never removes this set, and live-up never sees it. down removes
# the set. status lists what the state holds: exit 0 when it holds a resource, 1 when it holds
# none. up and down show their plan and ask before they change anything.
#
# examples/live/tf.sh runs OpenTofu. It reads the Palette key from the environment or from
# ~/.palette.env, and never prints it.
set -uo pipefail

live=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
script=contract-profiles
. "$live/common.sh"

state=$live/terraform/contract.tfstate
vars=(-var name_prefix=strata-contract -var cloud=aws -var cluster=false -var own_registries=false)

case "${1:-}" in
  up)
    "$live/tf.sh" init -input=false || exit 2
    "$live/tf.sh" apply -state="$state" "${vars[@]}"
    ;;
  down)
    "$live/tf.sh" destroy -state="$state" "${vars[@]}"
    ;;
  status)
    [ -f "$state" ] || { echo "$script: the standing set holds no resource: $state does not exist"; exit 1; }
    held=$("$live/tf.sh" state list -state="$state") || exit 2
    [ -n "$held" ] || { echo "$script: the standing set holds no resource"; exit 1; }
    printf '%s\n' "$held"
    ;;
  -h | --help)
    usage
    ;;
  *)
    usage >&2
    die "the argument is up, down, or status"
    ;;
esac

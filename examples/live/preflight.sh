#!/usr/bin/env bash
# The checks of the lab itself, before a live recipe builds anything (#91): the tools, the key, the
# credentials of the cloud CLIs, the APIs of the GCP project, the core policies of an EKS run, and
# the leftovers of a run before. They are lab checks, not strata checks: `strata preflight` checks
# the cluster, and this script checks the machine and the accounts the lab runs from. The rows of
# the catalog with `engines: [lab]` name each one.
#
#   examples/live/preflight.sh [--cloud aws|eks|gcp|gke]
#
# Reads terraform.tfvars for the cloud (default aws), the region, the AWS profile, the GCP project,
# and the cluster names. Results go under runs/<UTC time>/preflight/, or OUT. Exit 0 only when no
# check fails; `just live-up` and `just live-profiles` stop at exit 1. lib.sh stops the script
# first when the strata binary is older than the sources (strata-binary-stale).
set -uo pipefail
script=preflight
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata; run just build"

tfvars=$live/terraform/terraform.tfvars
var() { sed -n "s/^$1 *= *\"\(.*\)\"/\1/p" "$tfvars" | head -n 1; }
cloud=$(var cloud)
cloud=${cloud:-aws}
while [ $# -gt 0 ]; do
  case "$1" in
    --cloud) cloud=$2; shift 2 ;;
    -h|--help) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option $1" ;;
  esac
done
case "$cloud" in aws|eks|gcp|gke) ;; *) die "no lab for the cloud '$cloud'" ;; esac
region=$(var region); region=${region:-us-east-2}
aws_profile=$(var aws_profile)
gcp_project=$(var gcp_project)
cluster_a=$(var cluster_name); cluster_a=${cluster_a:-fred}
cluster_b=$(var second_cluster_name); cluster_b=${cluster_b:-barney}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/preflight}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  {
    echo "# the checks of the lab: $cloud"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, cloud \`$cloud\`, region \`$region\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

# ---- the tools --------------------------------------------------------------------------------------
case "$cloud" in
  aws|eks) tools="tofu jq just aws crane oras kubectl helm" ;;
  gcp|gke) tools="tofu jq just gcloud crane oras kubectl helm" ;;
esac
missing_tools() { local t m=""; for t in $tools; do command -v "$t" > /dev/null || m="$m $t"; done; echo "${m# }"; [ -z "$m" ]; }
check tools-present 0 "tools-present: each tool of the lab is on the PATH ($tools)" -- missing_tools

# ---- Palette ------------------------------------------------------------------------------------------
check palette-key 0 "the key reads the projects of the tenant" -- "$strata" list projects --json
check palette-preflight 0 "strata preflight runs, and lists the rules of $cloud" -- "$strata" preflight --rules --cloud "$cloud"

# ---- the cloud CLI -----------------------------------------------------------------------------------
case "$cloud" in
  aws|eks)
    [ -n "$aws_profile" ] && export AWS_PROFILE=$aws_profile
    check cloud-cli-auth 0 "cloud-cli-auth: aws sts get-caller-identity answers with the profile ${AWS_PROFILE:-default}" -- \
      aws sts get-caller-identity --output json
    ;;
  gcp|gke)
    check cloud-cli-auth 0 "cloud-cli-auth: gcloud has an access token" -- gcloud auth print-access-token
    ;;
esac

# ---- the APIs of the GCP project ----------------------------------------------------------------------
if [ "$cloud" = gcp ] || [ "$cloud" = gke ]; then
  [ -n "$gcp_project" ] || die "set gcp_project in terraform.tfvars"
  gcloud services list --enabled --project "$gcp_project" --format json > services.json 2> services.err \
    || die "gcloud services list failed; see $PWD/services.err"
  check gcp-apis-off 0 "gcp-apis-off: compute, cloudresourcemanager, and iam are enabled in $gcp_project" -- \
    jq -e 'map(.config.name) | (index("compute.googleapis.com") != null) and (index("cloudresourcemanager.googleapis.com") != null) and (index("iam.googleapis.com") != null)' services.json
  if [ "$cloud" = gke ]; then
    check gke-api-off 0 "gke-api-off: container.googleapis.com is enabled in $gcp_project (#90)" -- \
      jq -e 'map(.config.name) | index("container.googleapis.com") != null' services.json
  fi
fi

# ---- the core policies of an EKS run -------------------------------------------------------------------
if [ "$cloud" = eks ]; then
  account=$(aws sts get-caller-identity --query Account --output text 2> /dev/null)
  core_policies() {
    local p m=""
    for p in PaletteControllerPolicy PaletteControlPlanePolicy PaletteNodesPolicy PaletteDeploymentPolicy PaletteCAPATaggingPolicy PaletteCloudFormationPolicy; do
      aws iam get-policy --policy-arn "arn:aws:iam::$account:policy/$p" --output json > /dev/null 2>&1 || m="$m $p"
    done
    echo "${m# }"; [ -z "$m" ]
  }
  check eks-core-policies 0 "eks-core-policies: the six core policies of Palette are in the AWS account (#89)" -- core_policies
fi

# ---- the login of the OCI registry ----------------------------------------------------------------------
# The registry of the example is an ECR repository that live-up makes; before the first apply there
# is none, and the check does not apply. A build needs no login; verify.sh and registry.sh do, so
# the check records the state (SEEN) and stops nothing. The exit code is in steps.log: 0 is a
# login that reads the repository, 1 is none, or one older than 12 hours.
if outputs=$("$live/tf.sh" output -json 2> /dev/null) && [ -n "$(jq -r '.owned_chart_repository.value // empty' <<<"$outputs")" ]; then
  repo=$(jq -r '.owned_chart_repository.value' <<<"$outputs")
  check ecr-token '?' "ecr-token, crane-empty-password: crane reads the ECR repository of the example; 0 is a login of the last 12 hours, which verify.sh needs and a build does not" -- \
    crane ls "$repo"
fi

# ---- the leftovers of a run before ----------------------------------------------------------------------
# The inventories read the cloud, not Palette: 0 on every line, or the run before is not finished.
case "$cloud" in
  aws|eks)
    check leftovers-a 0 "leftovers-before-build: nothing of cluster $cluster_a is in AWS $region" -- "$live/aws-inventory.sh" "$cluster_a" "$region"
    check leftovers-b 0 "leftovers-before-build: nothing of cluster $cluster_b is in AWS $region" -- "$live/aws-inventory.sh" "$cluster_b" "$region"
    ;;
  gcp)
    check leftovers-a 0 "leftovers-before-build: nothing of cluster $cluster_a is in $gcp_project" -- "$live/gcp-inventory.sh" "$gcp_project" "$cluster_a"
    check leftovers-b 0 "leftovers-before-build: nothing of cluster $cluster_b is in $gcp_project" -- "$live/gcp-inventory.sh" "$gcp_project" "$cluster_b"
    ;;
  gke)
    check leftovers-a 0 "leftovers-before-build, inventory-managed: nothing of cluster $cluster_a is in $gcp_project, the GKE cluster counted" -- "$live/gcp-inventory.sh" "$gcp_project" "$cluster_a" --gke
    check leftovers-b 0 "leftovers-before-build, inventory-managed: nothing of cluster $cluster_b is in $gcp_project" -- "$live/gcp-inventory.sh" "$gcp_project" "$cluster_b" --gke
    ;;
esac

echo "preflight: $failures check(s) failed; results in $run"
[ "$failures" = 0 ]

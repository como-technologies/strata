#!/usr/bin/env bash
# The test plan of #91 for one engine, in two parts: each rule that the lab can make fails when it
# must, and a clean entity passes. Each part runs `strata preflight` with the static and the live
# tiers, and the cloud tier with the tools of this machine. No cluster is built; the profiles of
# the live example are enough (just live-profiles, with the cloud of the engine). Each result goes
# to the ledger of the runs, preflight/evidence/runs.yaml, which the confidence of each rule is
# counted from.
#
#   examples/live/preflight-plan.sh ENGINE [--build]
#
#   1  the clean entity of the engine, from examples/live/preflight/ENGINE.json and the tenant: the
#      cloud account, the release profile, the region, and the zone of terraform.tfvars
#   2  each fault on purpose: one edit of the clean entity, and strata preflight --rule ID must
#      FAIL on it, in seconds. The ledger gets `caught`, or `missed`
#   3  the clean entity: every tier, and each rule of severity fail that ran must PASS. The ledger
#      gets `passed`, or `false-fail`
#   4  with --build only: just live-up, then strata preflight --watch on cluster A until it is
#      Running, or a stuck state. The ledger gets `passed` for each watch rule that passed. The
#      cluster stays: run just live-down after
#
# Needs live-profiles with cloud = ENGINE in terraform.tfvars (or -var cloud=ENGINE), the strata
# and strata-preflight binaries, jq, the key as verify.sh needs it, and the tool of the cloud (aws,
# gcloud). PALETTE_RELEASE names the Palette release for the ledger, when known; a tenant key
# cannot read it. Results go under runs/<UTC time>/preflight-plan/, or OUT. Exit 0 only when no
# check fails.
set -uo pipefail
script=preflight-plan
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata; run just build"
engine=${1:-}
usage() { sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; }
case "$engine" in
  aws|eks|gcp|gke) ;;
  -h|--help) usage; exit 0 ;;
  "") usage >&2; exit 2 ;;
  *) die "no plan for the engine '$engine'" ;;
esac
shift
build=""
[ "${1:-}" = "--build" ] && build=1

tfvars=$live/terraform/terraform.tfvars
var() { sed -n "s/^$1 *= *\"\(.*\)\"/\1/p" "$tfvars" | head -n 1; }
ledger=$live/../../preflight/evidence/runs.yaml
release=${PALETTE_RELEASE:-}

run_id=$(date -u +%Y%m%dT%H%M%SZ)
run=${OUT:-$live/runs/$run_id/preflight-plan}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
export PALETTE_PROJECT_UID=$(out project_uid)
R=$(out release_profile_uid)
[ -n "$R" ] || die "no release profile; run just live-profiles first"
"$strata" snapshot "$R" -o release.json > /dev/null 2>&1 || die "cannot record the release profile $R"
profile_cloud=$(jq -r .source.cloudType release.json)
[ "$profile_cloud" = "$engine" ] || die "the release profile is of cloud type $profile_cloud, not $engine: apply with -var cloud=$engine"

summary() {
  {
    echo "# the test plan of #91: $engine"
    echo
    echo "Run \`$run_id\` with \`$("$strata" preflight --version)\`, release profile \`$R\`, Palette release \`${release:-not given}\`."
    echo
    summary_table
    echo
    echo "## the ledger entry"
    echo
    echo '```yaml'
    cat ledger-entry.yaml 2> /dev/null
    echo '```'
  } > summary.md
}
trap summary EXIT

# ---- 1. the clean entity -------------------------------------------------------------------------------
echo "== 1. the clean entity of $engine" | tee -a steps.log
case "$engine" in
  aws|eks)
    account_name=$(var cloud_account_name)
    [ "$engine" = eks ] && account_name="$(var name_prefix)-eks" && account_name=${account_name#-eks}
    [ "$engine" = eks ] && account_name="$(var name_prefix || echo strata-live)-eks"
    kind=aws; region=$(var region); region=${region:-us-east-2}; az=$(var az); az=${az:-${region}a}
    ssh_key=$(var ssh_key_name); project=""
    ;;
  gcp|gke)
    account_name=$(var gcp_cloud_account_name); kind=gcp
    region=$(var gcp_region); region=${region:-us-east1}; az=$(var gcp_zone); az=${az:-${region}-b}
    ssh_key=""; project=$(var gcp_project)
    ;;
esac
# the name_prefix of the example defaults to strata-live
[ "$engine" = eks ] && account_name="$( { var name_prefix; } | grep . || echo strata-live)-eks"
"$live/api.sh" get "/v1/cloudaccounts/summary" > accounts.json 2> accounts.err || die "cannot list the cloud accounts; see $PWD/accounts.err"
A=$(jq -r --arg n "$account_name" --arg k "$kind" '.items[] | select(.metadata.name == $n and .kind == $k) | .metadata.uid' accounts.json | head -n 1)
[ -n "$A" ] || die "no cloud account named $account_name of kind $kind in the project; the summary lists: $(jq -r '.items[] | "\(.metadata.name) (\(.kind))"' accounts.json | paste -sd ', ' -)"
name="strata-preflight-$engine"
sed -e "s/__ACCOUNT_UID__/$A/g" -e "s/__PROFILE_UID__/$R/g" -e "s/__REGION__/$region/g" -e "s/__AZ__/$az/g" \
    -e "s/__SSH_KEY__/$ssh_key/g" -e "s/__PROJECT__/$project/g" -e "s/__NAME__/$name/g" \
    "$live/preflight/$engine.json" > clean.json
check clean-entity 0 "the clean entity of $engine parses, and names the account $account_name and the profile $R" -- \
  jq -e --arg a "$A" --arg r "$R" '.spec.cloudAccountUid == $a and .spec.profiles[0].uid == $r' clean.json

release_arg=()
[ -n "$release" ] && release_arg=(--release "$release")
pf() { "$strata" preflight "$@" --cloud "$engine" "${release_arg[@]}" --json --quiet --no-color; }

# ---- 2. each fault on purpose -------------------------------------------------------------------------
# One line for each fault: the rule id, then the jq edit of the clean entity that makes it. The
# rule must FAIL on the edited entity. A rule that PASSES missed the fault.
echo "== 2. each fault on purpose" | tee -a steps.log
results=()
record() { results+=("$1|$2|$3"); }   # rule|result|note
fault() {
  local id=$1 edit=$2 label
  jq "$edit" clean.json > "fault-$id.json" || { record "$id" not-made "the edit failed"; return; }
  pf "fault-$id.json" --rule "$id" --out "out-fault-$id" > "fault-$id.report.json" 2> "fault-$id.err"
  local verdict
  verdict=$(jq -r --arg id "$id" '.checks[] | select(.id == $id) | .verdict' "fault-$id.report.json" 2> /dev/null)
  case "$verdict" in
    failed) label=caught ;;
    passed) label=missed ;;
    *) label=not-made ;;
  esac
  record "$id" "$label" "the fault was made: $edit"
  check "fault-$id" 0 "$id fails on its fault, in $(jq -r --arg id "$id" '.checks[] | select(.id == $id) | .elapsedMs' "fault-$id.report.json" 2> /dev/null) ms: $(jq -r --arg id "$id" '[.checks[] | select(.id == $id) | (.messages // [])[]] | join("; ") | .[:160]' "fault-$id.report.json" 2> /dev/null)" -- \
    test "$label" = caught
}
# resize: set the size, the minimum, and the maximum of a pool together; Palette refuses a size
# under the minimum with ShouldBeEqOrGreaterThanParamValues, which is not the fault of the row
resize='def resize($n): .poolConfig.size = $n | .poolConfig.minSize = $n | .poolConfig.maxSize = $n;'
first_pool="$resize .spec.machinepoolconfig[0] |= resize"
worker_pools="$resize (.spec.machinepoolconfig[] | select((.poolConfig.isControlPlane // false) | not)) |= resize"
case "$engine" in
  gke)
    fault gke-pool-multiple-3 "$first_pool(4)"
    fault palette-validate "$first_pool(4)"
    fault gke-no-release-channel '.spec.cloudConfig.overrideClusterAPIConfig = ""'
    fault gke-no-gpu '.spec.machinepoolconfig[0].cloudConfig.instanceType = "a2-highgpu-1g"'
    fault gke-autopilot '.spec.cloudConfig.managedClusterConfig.enableAutoPilot = true'
    fault gcp-name-length '.metadata.name = "a-cluster-name-that-is-longer-than-forty-characters"'
    fault account-missing '.spec.cloudAccountUid = "000000000000000000000000"'
    ;;
  eks)
    fault eks-one-zone ".spec.machinepoolconfig[0].cloudConfig.azs = [\"$az\"]"
    fault eks-worker-needed "$first_pool(0)"
    fault update-strategy-eks '.spec.machinepoolconfig[0].poolConfig.updateStrategy = {type: "RollingUpdateScaleOut"}'
    fault api-endpoint-reach '.spec.cloudConfig.endpointAccess = {public: false, private: true, publicCIDRs: [], privateCIDRs: []}'
    fault account-missing '.spec.cloudAccountUid = "000000000000000000000000"'
    fault worker-cpu-floor "$first_pool(1)"
    ;;
  aws)
    fault worker-cpu-floor "$worker_pools(1)"
    fault palette-validate "$worker_pools(1)"
    fault api-endpoint-reach '.spec.cloudConfig.controlPlaneLoadBalancer = "internal"'
    fault account-missing '.spec.cloudAccountUid = "000000000000000000000000"'
    ;;
  gcp)
    fault worker-cpu-floor "$worker_pools(1)"
    fault palette-validate "$worker_pools(1)"
    fault gcp-name-length '.metadata.name = "a-cluster-name-that-is-longer-than-fifty-two-characters-x"'
    fault account-missing '.spec.cloudAccountUid = "000000000000000000000000"'
    ;;
esac

# ---- 3. the clean entity -------------------------------------------------------------------------------
echo "== 3. the clean entity, every tier" | tee -a steps.log
pf clean.json --out out-clean > clean.report.json 2> clean.err
clean_exit=$?
check clean-exit 0 "the clean entity of $engine: exit 0 (a failed probe does not count)" -- test "$clean_exit" = 0
check clean-report 0 "the report of the clean entity, one line for each rule" -- \
  jq -r '.checks[] | select(.verdict != "skipped") | "\(.verdict)\t\(.level)\t\(.severity)\t\(.id)\t\((.messages // []) | join("; ") | .[:120])"' clean.report.json
check clean-no-error 0 "no check of the clean entity gave ERROR" -- jq -e '[.checks[] | select(.verdict == "error")] | length == 0' clean.report.json
# each rule of severity fail that ran: passed, or false-fail
while IFS=$'\t' read -r id verdict; do
  case "$verdict" in
    passed) record "$id" passed "the clean entity" ;;
    failed) record "$id" false-fail "the clean entity: $(jq -r --arg id "$id" '[.checks[] | select(.id == $id) | (.messages // [])[]] | join("; ") | .[:160]' clean.report.json)" ;;
  esac
done < <(jq -r '.checks[] | select(.severity == "fail" and (.verdict == "passed" or .verdict == "failed")) | "\(.id)\t\(.verdict)"' clean.report.json)
check clean-fail-rules 0 "each rule of severity fail that ran on the clean entity passed" -- \
  jq -e '[.checks[] | select(.severity == "fail" and .verdict == "failed")] | length == 0' clean.report.json
check clean-scorecard 0 "the section scorecard of the clean entity" -- \
  jq -r '.sections[] | "\(.title)\t rows \(.rows) ran \(.ran) passed \(.passed) failed \(.failed) not-checked \(.notChecked) readiness \(.readiness // "-")"' clean.report.json

# ---- 4. the watch, with --build ---------------------------------------------------------------------------
if [ -n "$build" ]; then
  echo "== 4. the build, and the watch" | tee -a steps.log
  echo "tofu apply -var cloud=$engine" | tee -a steps.log
  "$live/tf.sh" apply -input=false -auto-approve -no-color -var "cloud=$engine" > apply.txt 2>&1 || die "tofu apply failed; see $PWD/apply.txt"
  load_outputs
  C=$(out cluster_uid)
  [ -n "$C" ] || die "no cluster A after the apply"
  pf "cluster:$C" --watch --every 30 --limit 5400 > watch.report.json 2> watch.err
  watch_exit=$?
  check watch-exit 0 "the watch of cluster $C ends with the cluster Running and each watch rule passed" -- test "$watch_exit" = 0
  check watch-report 0 "the report of the watch" -- jq -r '.subject.source, (.checks[] | "\(.verdict)\t\(.id)\t\((.messages // []) | join("; ") | .[:160])")' watch.report.json
  while IFS=$'\t' read -r id verdict; do
    case "$verdict" in
      passed) record "$id" passed "the clean build, watched to Running" ;;
      failed) record "$id" false-fail "the clean build: $(jq -r --arg id "$id" '[.checks[] | select(.id == $id) | (.messages // [])[]] | join("; ") | .[:160]' watch.report.json)" ;;
    esac
  done < <(jq -r '.checks[] | select(.severity == "fail" and (.verdict == "passed" or .verdict == "failed")) | "\(.id)\t\(.verdict)"' watch.report.json)
  echo "the cluster $C stays: run just live-down" | tee -a steps.log
fi

# ---- the ledger ------------------------------------------------------------------------------------------
{
  echo "  - run: $run_id"
  echo "    engine: $engine"
  echo "    release: \"$release\""
  echo "    results:"
  for r in "${results[@]}"; do
    IFS='|' read -r id result note <<<"$r"
    printf '      - {rule: %s, result: %s, note: %s}\n' "$id" "$result" "\"$(sed 's/"/\\"/g' <<<"$note")\""
  done
} > ledger-entry.yaml
if [ -f "$ledger" ]; then
  # the empty ledger says `runs: []`; the first entry turns it into a list
  sed -i 's/^runs: \[\]$/runs:/' "$ledger"
  cat ledger-entry.yaml >> "$ledger"
  check ledger 0 "the ledger parses with the entry of this run, and --promotion reads it" -- "$strata" preflight --promotion
else
  echo "no ledger at $ledger; the entry is in $PWD/ledger-entry.yaml" | tee -a steps.log
fi

echo "preflight-plan: $failures check(s) failed; results in $run"
[ "$failures" = 0 ]

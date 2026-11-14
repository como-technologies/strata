#!/usr/bin/env bash
# The test plan of #91 for one engine, in two parts. In the first part, each rule that the lab can
# make fails when it must. In the second part, a clean entity passes. Each part runs
# `strata preflight` with the static and the live tiers, and the cloud tier with the tools of this
# machine. The plan builds no cluster; the profiles of the live example are enough (just
# live-profiles, with the cloud of the engine). Each result goes to the ledger of the runs,
# crates/strata-preflight/evidence/runs.yaml, from which strata-preflight counts the confidence of each rule.
#
#   examples/live/preflight-plan.sh ENGINE [--build [--down]]
#
#   1  the clean entity of the engine, from examples/live/preflight/ENGINE.json and the tenant: the
#      cloud account, the release profile, the region, and the zone of terraform.tfvars
#   2  each fault on purpose: one edit of the clean entity, and strata preflight --rule ID must
#      FAIL on it, in seconds. The ledger gets `caught`, or `missed`.
#
#      Three rules read a list of the cloud account from Palette (#143): instance-offering,
#      ssh-key-pair on eks, and gcp-properties on gcp. The fault of instance-offering is a type
#      that the instance type route lists with a zone in nonSupportedZones. The value of
#      nonSupportedZones is null for a type that each zone offers. The script reads the type from
#      the route itself. On aws, the property route refuses a key pair that the region lacks, and
#      iaas-pod-identity sends the key pair to it (#430)
#   3  the clean entity: every tier, and each rule of severity fail that ran must PASS. The ledger
#      gets `passed`, or `false-fail`
#   4  with --build only: the apply of cluster A through the provider, and strata preflight --watch
#      on it, from the moment Palette lists it. The watch runs until the cluster is Running or a
#      stuck state. The ledger gets `passed` for each watch rule that passed
#   5  with --build only, once the cluster is Running (the provider can return before that): strata
#      preflight on cluster:UID. This is the cluster as Palette holds it, which is the entity the
#      provider sent. Then strata preflight on cluster:UID with a change of the instance type of
#      the worker pool. The rules on a change must fail on it. The ledger gets both
#   6  with --build --down only: the destroy, the delete of the versions outside the state, and
#      the inventory of the cloud. The inventory must be 0 on every line. Without --down the
#      cluster stays: run just live-down after
#
# Needs live-profiles with cloud = ENGINE in terraform.tfvars, or -var cloud=ENGINE. A switch of the
# cloud needs `just live-down` first, or Palette refuses the apply with ClusterProfileAlreadyExists.
# It also needs the strata and strata-preflight binaries, jq, the key as verify.sh needs it, and the
# tool of the cloud (aws, gcloud). PALETTE_RELEASE names the Palette release for the ledger, when
# known; a tenant key cannot read it. Results go under runs/<UTC time>/preflight-plan/, or OUT. Exit
# 0 only when no check fails.
set -uo pipefail
script=preflight-plan
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata; run just build"
engine=${1:-}
case "$engine" in
  aws|eks|gcp|gke) ;;
  -h|--help) usage; exit 0 ;;
  "") usage >&2; exit 2 ;;
  *) die "no plan for the engine '$engine'" ;;
esac
shift
build=""
down=""
while [ $# -gt 0 ]; do
  case "$1" in
    --build) build=1; shift ;;
    --down) down=1; shift ;;
    *) die "unknown option $1" ;;
  esac
done
[ -z "$down" ] || [ -n "$build" ] || die "--down goes with --build"

ledger=$live/../../crates/strata-preflight/evidence/runs.yaml
release=${PALETTE_RELEASE:-}

run_id=$(date -u +%Y%m%dT%H%M%SZ)
run=${OUT:-$live/runs/$run_id/preflight-plan}
mkdir -p "$run" && cd "$run" || exit 2
echo "preflight-plan: run $run_id; the report of each step goes to $run"
export STRATA_QUIET=1

load_outputs
export PALETTE_PROJECT_UID=$(out project_uid)
R=$(out release_profile_uid)
[ -n "$R" ] || die "no release profile; run just live-profiles first"
"$strata" snapshot "$R" -o release.json > /dev/null 2>&1 || die "cannot record the release profile $R"
profile_cloud=$(jq -r .source.cloudType release.json)
[ "$profile_cloud" = "$engine" ] || die "the release profile is of cloud type $profile_cloud, not $engine: apply with -var cloud=$engine"

summary() {
  write_summary "the test plan of #91: $engine" ", preflight \`$("$strata" preflight --version)\`," \
    "release profile \`$R\`, Palette release \`${release:-not given}\`."
  # a note of the entry can quote a message, so no line of the entry can end the block
  local fence
  fence=$(cat ledger-entry.yaml 2> /dev/null | jq -Rrs "$markdown_jq fence")
  {
    echo
    echo "## the ledger entry"
    echo
    echo "${fence}yaml"
    cat ledger-entry.yaml 2> /dev/null
    # the last line gets its line break, or the fence joins it
    [ -z "$(tail -c 1 ledger-entry.yaml 2> /dev/null)" ] || echo
    echo "$fence"
  } >> summary.md
}
trap summary EXIT

# ---- 1. the clean entity -------------------------------------------------------------------------------
echo "== 1. the clean entity of $engine" | tee -a steps.log
name="strata-preflight-$engine"
create_body "$engine" "$R" "$name" clean.json
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
record() { # rule|result|note. A result the ledger does not know stays out of the entry, and steps.log names it
  case "$2" in
    caught|missed|passed|false-fail) results+=("$1|$2|$3") ;;
    *) echo "not in the ledger: $1 $2: $3" | tee -a steps.log ;;
  esac
}
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
# resize: set the size, the minimum, and the maximum of a pool together. Palette refuses a size
# under the minimum with ShouldBeEqOrGreaterThanParamValues, which is not the fault of the row
resize='def resize($n): .poolConfig.size = $n | .poolConfig.minSize = $n | .poolConfig.maxSize = $n;'
# offering_edit: print the jq edit of the clean entity. The edit puts the first pool in a zone that
# does not offer the type of the pool, according to the instance type route. The script reads the
# route for the region and the account of the entity, with the account_kind that create_body set.
#
# The query is the one that the rules send. It lifts the default filter of Palette: see
# instancetypes-default-filter. The script keeps the answer in instancetypes.json.
#
# The edit takes the first type that has a zone in nonSupportedZones. Exit 1, with the reason, when
# the route lists none.
offering_edit() {
  local pair
  "$live/api.sh" get "/v1/clouds/$account_kind/regions/$region/instancetypes?cloudAccountUid=$A&cpuGtEq=0&memoryGtEq=0" > instancetypes.json || { echo "api.sh get failed: $(tail -n 1 instancetypes.json)" >&2; return 1; }
  pair=$(jq -r '[.instanceTypes[]? | select((.nonSupportedZones // []) | length > 0)] | .[0] // empty | "\(.type)\t\(.nonSupportedZones[0])"' instancetypes.json) || { echo "instancetypes.json is not JSON" >&2; return 1; }
  [ -n "$pair" ] || { echo "the route lists no type with a zone in nonSupportedZones" >&2; return 1; }
  local type zone
  IFS=$'\t' read -r type zone <<<"$pair"
  printf '.spec.machinepoolconfig[0].cloudConfig.instanceType = "%s" | .spec.machinepoolconfig[0].cloudConfig.azs = ["%s"]\n' "$type" "$zone"
}
first_pool="$resize .spec.machinepoolconfig[0] |= resize"
worker_pools="$resize (.spec.machinepoolconfig[] | select((.poolConfig.isControlPlane // false) | not)) |= resize"
case "$engine" in
  gke)
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
    # not worker-cpu-floor: the EKS validate route reports no CPU floor for a managed node group
    # (runs 20260929T042910Z and 20260929T044751Z). So the rule does not name eks
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

# The three rules that read a list of the cloud account (#143). The first check shows the shape of
# the answer of the route; the faults then show that each rule reads it.
echo "== 2b. the rules that read a list of the cloud account" | tee -a steps.log
if edit=$(offering_edit 2> offering-edit.err); then
  check instance-types-shape 0 "the instance type route lists the types of $region as instanceTypes[] with a type, and nonSupportedZones as an array or null" -- \
    jq -e '(.instanceTypes | length) > 0 and all(.instanceTypes[]; has("type") and ((.nonSupportedZones | type) | . == "array" or . == "null"))' instancetypes.json
  fault instance-offering "$edit"
else
  record instance-offering not-made "$(cat offering-edit.err)"
  echo "instance-offering: $(cat offering-edit.err)" | tee -a steps.log
fi
# The quota rules count the vCPUs of a pool from the field cpu of the same route (#393). A type
# under 2 vCPUs or 4 GiB is in the list, because the query lifts the default filter of Palette.
# That type has a cpu too
check instancetypes-cpu 0 "the instance type route lists each pool type of the entity with cpu as a number above 0, the vCPUs that aws-quotas, gcp-quotas, and azure-vcpu-quota count, and it lists a type under 2 vCPUs or 4 GiB with cpu above 0" -- \
  jq -e --slurpfile e clean.json 'def small: select((.memory | type == "number" and . < 4) or (.cpu | type == "number" and . < 2)); [$e[0].spec.machinepoolconfig[].cloudConfig.instanceType] as $want | .instanceTypes as $t | all($want[]; . as $w | ([$t[] | select(.type == $w) | .cpu] | first | (type == "number" and . > 0))) and ([$t[] | small] | length) > 0 and all($t[] | small; (.cpu | type == "number" and . > 0))' instancetypes.json
# With no query, the route lists only the types with 2 vCPUs or more and 4 GiB or more. The same
# request without the query shows the filter: the answer lists no small type. The answer with the
# query lists one and is longer
"$live/api.sh" get "/v1/clouds/$account_kind/regions/$region/instancetypes?cloudAccountUid=$A" > instancetypes-default.json 2> instancetypes-default.err
check instancetypes-default-filter 0 "the instance type route with no query for the filter lists no type under 2 vCPUs or 4 GiB, and the query cpuGtEq=0&memoryGtEq=0 lists more types, with a small type among them" -- \
  jq -e --slurpfile all instancetypes.json 'def small: select((.memory | type == "number" and . < 4) or (.cpu | type == "number" and . < 2)); (.instanceTypes | length) > 0 and ([.instanceTypes[] | small] | length) == 0 and ([$all[0].instanceTypes[] | small] | length) > 0 and ($all[0].instanceTypes | length) > (.instanceTypes | length)' instancetypes-default.json
case "$engine" in
  aws|eks)
    "$live/api.sh" get "/v1/clouds/aws/regions/$region/keypairs?cloudAccountUid=$A" > keypairs.json 2> keypairs.err
    check key-pairs-shape 0 "the key pair route lists keyNames[] for $region, and the key $ssh_key of the entity is in it" -- \
      jq -e --arg k "$ssh_key" '(.keyNames | index($k)) != null' keypairs.json
    # Three list routes answer the rules static-subnets and sg-name-collision (#393). Each check
    # reads the keys that the route gives, and fails on a key beside them. A new key can be a field
    # that a cloud rule reads, such as the tags of a subnet. Then that rule can move to Palette
    "$live/api.sh" get "/v1/clouds/aws/regions/$region/vpcs?cloudAccountUid=$A" > vpcs.json 2> vpcs.err
    check vpcs-route-keys 0 "the VPC route lists vpcs[] with vpcId, and each subnet with subnetId and az; a VPC has no key beside cidrBlock, name, subnets, and vpcId, and a subnet has none beside az, isPrivate, mapPublicIpOnLaunch, name, and subnetId" -- \
      jq -e '(.vpcs | length) > 0 and all(.vpcs[]; has("vpcId") and ((keys - ["cidrBlock", "name", "subnets", "vpcId"]) | length) == 0) and ([.vpcs[] | (.subnets // [])[]] as $s | ($s | length) > 0 and all($s[]; has("subnetId") and has("az") and ((keys - ["az", "isPrivate", "mapPublicIpOnLaunch", "name", "subnetId"]) | length) == 0))' vpcs.json
    "$live/api.sh" get "/v1/clouds/aws/regions/$region/availabilityzones?cloudAccountUid=$A" > zones.json 2> zones.err
    check zones-route-keys 0 "the zone route lists zones[] with name, state, and zoneId, and a zone has no key beside those three, so it lists no zone type" -- \
      jq -e '(.zones | length) > 0 and all(.zones[]; has("name") and has("state") and has("zoneId") and ((keys - ["name", "state", "zoneId"]) | length) == 0)' zones.json
    vpc=$(jq -r '.vpcs[0].vpcId // empty' vpcs.json 2> /dev/null)
    "$live/api.sh" get "/v1/clouds/aws/securitygroups?cloudAccountUid=$A&region=$region&vpcId=$vpc" > sgs.json 2> sgs.err
    check sgs-route-keys 0 "the security group route lists groups[] with groupId and groupName for the first VPC of $region, and a group has no key beside groupId, groupName, and ownerId" -- \
      jq -e '(.groups | length) > 0 and all(.groups[]; has("groupId") and has("groupName") and ((keys - ["groupId", "groupName", "ownerId"]) | length) == 0)' sgs.json
    # ssh-key-pair names eks alone. The property route of aws refuses a key pair that the region
    # lacks (HTTP 500, AwsKeyPairNotFound). iaas-pod-identity sends it the key pair (#430)
    case "$engine" in
      aws)
        fault iaas-pod-identity '.spec.cloudConfig.sshKeyName = "strata-no-such-key"'
        check properties-key-pair 0 "the property route of aws refuses a key pair that the region lacks with HTTP 500 and AwsKeyPairNotFound, and the report of iaas-pod-identity fails the rule with that code, not as an error" -- \
          jq -e '[.checks[] | select(.id == "iaas-pod-identity" and .verdict == "failed") | (.messages // [])[]] | any(.[]; test("AwsKeyPairNotFound"))' fault-iaas-pod-identity.report.json
        ;;
      eks) fault ssh-key-pair '.spec.cloudConfig.sshKeyName = "strata-no-such-key"' ;;
    esac
    ;;
  gcp)
    fault gcp-properties '.spec.cloudConfig.project = "strata-no-such-project-000000"'
    ;;
esac

# ---- 3. the clean entity -------------------------------------------------------------------------------
# with the custom probes of the lab beside the catalog: a custom probe on a real tenant
echo "== 3. the clean entity, every tier, with the custom probes of the lab" | tee -a steps.log
pf clean.json --probes "$live/preflight/lab-probes.yaml" --out out-clean > clean.report.json 2> clean.err
clean_exit=$?
check custom-probes 0 "the custom probes of the lab ran beside the catalog: lab-region and lab-name passed" -- \
  jq -e '[.checks[] | select(.section == "lab")] | length == 2 and all(.[]; .verdict == "passed")' clean.report.json
check clean-exit 0 "the clean entity of $engine: exit 0 (a failed probe does not count)" -- test "$clean_exit" = 0
check clean-report 0 "the report of the clean entity, one line for each rule" -- \
  jq -r '.checks[] | select(.verdict != "skipped" and .verdict != "not-run") | "\(.verdict)\t\(.level)\t\(.severity)\t\(.id)\t\((.messages // []) | join("; ") | .[:120])"' clean.report.json
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

# ---- 4. the build, watched, with --build ------------------------------------------------------------------
# The apply runs in the background, and the watch starts as soon as Palette lists the cluster. This
# is because the provider returns at its own end, and the watch is about the build itself.
if [ -n "$build" ]; then
  echo "== 4. the build of cluster A on $engine, watched" | tee -a steps.log
  cluster_name=$(var cluster_name); cluster_name=${cluster_name:-fred}
  echo "tofu apply -var cloud=$engine (in the background)" | tee -a steps.log
  "$live/tf.sh" apply -input=false -auto-approve -no-color -var "cloud=$engine" > apply.txt 2>&1 &
  apply_pid=$!
  C=""
  for _ in $(seq 1 120); do
    C=$("$strata" list clusters --json 2> /dev/null | jq -r --arg n "$cluster_name" '.clusters[] | select(.name == $n) | .uid' | head -n 1)
    [ -n "$C" ] && break
    kill -0 "$apply_pid" 2> /dev/null || break
    sleep 10
  done
  if [ -z "$C" ]; then
    wait "$apply_pid"
    die "Palette lists no cluster named $cluster_name after 20 minutes; see $PWD/apply.txt"
  fi
  echo "Palette lists cluster $C; the watch starts while the apply runs" | tee -a steps.log
  # the watch streams to the terminal, one line for each rule at each reading, and to watch.err.
  # The tee is a pipe, so the tool asks for the color when the terminal is there
  plan_color=never; [ -t 2 ] && plan_color=always
  "$strata" preflight "cluster:$C" --cloud "$engine" "${release_arg[@]}" --json --verbose --color "$plan_color" \
    --watch --every 30 --limit 5400 --out out-watch > watch.report.json 2> >(tee watch.err >&2)
  watch_exit=$?
  wait "$apply_pid"
  apply_exit=$?
  command grep -E '^(Plan|Apply complete|Error)' apply.txt | tee -a steps.log
  check apply 0 "the apply of cluster A on $engine through the provider ends with exit 0" -- test "$apply_exit" = 0
  check watch-exit 0 "the watch of cluster $C ends with exit 0: no counted watch rule failed (a failed probe or a warning is in the report)" -- test "$watch_exit" = 0
  check watch-report 0 "the report of the watch: the readings, and each watch rule" -- \
    jq -r '.subject.source, (.checks[] | "\(.verdict)\t\(.id)\t\((.messages // []) | join("; ") | .[:160])")' watch.report.json
  while IFS=$'\t' read -r id verdict; do
    case "$verdict" in
      passed) record "$id" passed "the clean build, watched to Running" ;;
      failed) record "$id" false-fail "the clean build: $(jq -r --arg id "$id" '[.checks[] | select(.id == $id) | (.messages // [])[]] | join("; ") | .[:160]' watch.report.json)" ;;
    esac
  done < <(jq -r '.checks[] | select(.severity == "fail" and (.verdict == "passed" or .verdict == "failed")) | "\(.id)\t\(.verdict)"' watch.report.json)

  # The provider can end its apply with exit 0 while the cluster still provisions. See the row
  # provider-timeout-success, and the eks run 20260929T142534Z after a pivot webhook timeout. The
  # checks of the cluster, and above all the delete, wait for Running. A delete of a cluster that
  # provisions stays in Deleting (delete-while-provisioning), which that run showed.
  echo "wait for cluster $C to be Running" | tee -a steps.log
  wait_running "$C"

  # ---- 5. the cluster as Palette holds it, and a change of it ----------------------------------------------
  echo "== 5. cluster $C as Palette holds it, and a change of its worker pool" | tee -a steps.log
  pf "cluster:$C" --out out-cluster > cluster.report.json 2> cluster.err
  check cluster-exit 0 "strata preflight cluster:$C, the entity the provider sent: exit 0" -- test "$?" = 0
  check cluster-report 0 "the report of the cluster as Palette holds it" -- \
    jq -r '.checks[] | select(.verdict != "skipped" and .verdict != "not-run") | "\(.verdict)\t\(.level)\t\(.severity)\t\(.id)\t\((.messages // []) | join("; ") | .[:120])"' cluster.report.json
  check cluster-no-error 0 "no check of the cluster gave ERROR" -- jq -e '[.checks[] | select(.verdict == "error")] | length == 0' cluster.report.json
  while IFS=$'\t' read -r id verdict; do
    case "$verdict" in
      passed) record "$id" passed "the cluster as Palette holds it, after a clean build" ;;
      failed) record "$id" false-fail "the cluster as Palette holds it: $(jq -r --arg id "$id" '[.checks[] | select(.id == $id) | (.messages // [])[]] | join("; ") | .[:160]' cluster.report.json)" ;;
    esac
  done < <(jq -r '.checks[] | select(.severity == "fail" and (.verdict == "passed" or .verdict == "failed")) | "\(.id)\t\(.verdict)"' cluster.report.json)
  # the change: another instance type for the worker pool. It replaces the pool on each engine,
  # orphans a node group on EKS (#89), and deletes the node pool on GKE (#90)
  case "$engine" in
    gke) new_type=e2-standard-8; change_rules="gke-type-change pool-replace-fields" ;;
    eks) new_type=m5.xlarge; change_rules="eks-type-change-orphan pool-replace-fields" ;;
    gcp) new_type=e2-standard-8; change_rules="pool-replace-fields" ;;
    aws) new_type=m5.xlarge; change_rules="pool-replace-fields" ;;
  esac
  jq -n --arg t "$new_type" '{spec: {machinepoolconfig: [{poolConfig: {name: "worker-pool"}, cloudConfig: {instanceType: $t}}]}}' > change.json
  pf "cluster:$C" change.json --out out-change > change.report.json 2> change.err
  for id in $change_rules; do
    verdict=$(jq -r --arg id "$id" '.checks[] | select(.id == $id) | .verdict' change.report.json 2> /dev/null)
    case "$verdict" in failed) label=caught ;; passed) label=missed ;; *) label=not-made ;; esac
    record "$id" "$label" "the change of the worker pool of cluster $C to $new_type"
    check "change-$id" 0 "$id fails on the change of the instance type to $new_type: $(jq -r --arg id "$id" '[.checks[] | select(.id == $id) | (.messages // [])[]] | join("; ") | .[:160]' change.report.json 2> /dev/null)" -- \
      test "$label" = caught
  done

  # ---- 6. the teardown, with --down ----------------------------------------------------------------------------
  if [ -n "$down" ]; then
    echo "== 6. the teardown" | tee -a steps.log
    names=$(out profile_names | jq -r '.[]' 2> /dev/null)
    "$live/tf.sh" destroy -input=false -auto-approve -no-color > destroy.txt 2>&1 \
      || "$live/tf.sh" destroy -input=false -auto-approve -no-color >> destroy.txt 2>&1
    destroy_exit=$?
    command grep -E '^(Destroy complete|Error)' destroy.txt | tee -a steps.log
    check destroy 0 "tofu destroy of the example ends with exit 0" -- test "$destroy_exit" = 0
    # shellcheck disable=SC2086
    "$live/api.sh" delete-named $names > delete-named.txt 2>&1 || echo "delete-named: $(tail -n 1 delete-named.txt)" | tee -a steps.log
    case "$engine" in
      aws|eks) check inventory 0 "the inventory of AWS for $cluster_name is 0 on every line" -- "$live/aws-inventory.sh" "$cluster_name" "$region" ;;
      gcp) check inventory 0 "the inventory of GCP for $cluster_name is 0 on every line" -- "$live/gcp-inventory.sh" "$project" "$cluster_name" ;;
      gke) check inventory 0 "the inventory of GCP for $cluster_name is 0 on every line, the GKE cluster counted" -- "$live/gcp-inventory.sh" "$project" "$cluster_name" --gke ;;
    esac
  else
    echo "the cluster $C stays: run just live-down" | tee -a steps.log
  fi
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

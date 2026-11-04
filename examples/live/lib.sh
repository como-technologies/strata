# Shared by the scripts in examples/live: the strata binary, the OpenTofu outputs, the usage and the
# stages, the checks and their summary, the edits of the lab, the OpenTofu applies, and the baseline.
# Set `script` to the script's name, `flag` to the immutable-clusterprofiles setting the script runs
# with, and `from` to the first stage of the run, then source it.
#
# The baseline is the state `just live-up` builds: cluster A Running the release and add-on
# profiles' current content, no cluster B, every profile name holding only the versions
# OpenTofu created, and an OpenTofu plan with the default variables that changes nothing.
# baseline.sh checks it. Every script requires it before it starts and leaves it as it found it,
# so the scripts run in any order.

live=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
strata=${STRATA_BIN:-$(cd "$live/../.." && pwd)/target/release/strata}
script=${script:-live}
flag=${flag:-false}

# The scripts read the tenant of the lab, and the lab can hold a value that strata does not know.
# No run of a script sends a report of such a value, whatever the environment, ~/.palette.env, or a
# saved answer say. A script that tests the send, such as report-send.sh, sets it again.
export STRATA_REPORT=0

# `die` and `usage` are in common.sh. A script that runs no strata sources that file alone, because
# the check for a stale binary below stops it.
. "$live/common.sh"

# stage N TITLE: print the heading of stage N, and succeed when this run reaches it. The script sets
# `from` to the first stage of the run.
stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

# A jq older than 1.7 exits with 0 for `jq -e` when its input is empty, as it is when the command
# before it failed. A check on that output passes. For such a jq, this function exits with 4 when
# `jq -e` prints nothing, as jq 1.7 does. `export -f` gives it to `bash -c` and to the scripts
# that this one starts.
if : | command jq -e . > /dev/null 2>&1; then
  jq() {
    local arg exit_status= printed rc
    for arg in "$@"; do
      case "$arg" in --exit-status) exit_status=1 ;; --*) ;; -*e*) exit_status=1 ;; esac
    done
    [ -n "$exit_status" ] || { command jq "$@"; return; }
    printed=$(mktemp) || return 2
    command jq "$@" | tee "$printed"
    rc=${PIPESTATUS[0]}
    [ "$rc" -ne 0 ] || [ -s "$printed" ] || rc=4
    rm -f "$printed"
    return "$rc"
  }
  export -f jq
fi

# The default binary must be built from these sources: a run with an older one tests old code.
# `just build`, and each live recipe, builds it. A binary set with STRATA_BIN is the caller's.
if [ -z "${STRATA_BIN:-}" ] && [ -e "$strata" ]; then
  stale=$(find "$live/../../src" "$live/../../Cargo.toml" "$live/../../Cargo.lock" -type f -newer "$strata" -print -quit 2>/dev/null)
  [ -z "$stale" ] || die "$strata is older than $(cd "$(dirname "$stale")" && pwd)/$(basename "$stale"); run just build"
fi

# ---- OpenTofu outputs ----------------------------------------------------------------------------

# load_outputs: read the OpenTofu outputs again; every apply here does it for you
load_outputs() { outputs=$("$live/tf.sh" output -json 2>/dev/null) || die "no tofu outputs; run just live-up first"; }
# out NAME: one output, as text; empty when it is not set
out() {
  local json=${outputs:-}
  [ -n "$json" ] || json='{}'
  jq -r --arg k "$1" '.[$k].value // empty | if type == "string" then . else tojson end' <<<"$json"
}

# ---- the machines of a cluster -----------------------------------------------------------------------

# instances [UID]: the machines of a cluster, cluster A ($C) by default, that are not deleted, by
# name, sorted, on one line.
# Palette lists them for each cloud under /v1/cloudconfigs/<cloud type>/<config uid>, so this works
# on each cloud. A repave replaces each machine, and so each name. The list is empty until the
# agent of the cluster reports, so read it on a cluster that has been Running.
instances() {
  local doc kind cfg pool
  doc=$("$live/api.sh" get "/v1/spectroclusters/${1:-$C}") || return 2
  kind=$(jq -r .spec.cloudType <<<"$doc")
  cfg=$(jq -r .spec.cloudConfigRef.uid <<<"$doc")
  "$live/api.sh" get "/v1/cloudconfigs/$kind/$cfg" | jq -r '.spec.machinePoolConfig[]?.name' | while read -r pool; do
    "$live/api.sh" get "/v1/cloudconfigs/$kind/$cfg/machinePools/$pool/machines" \
      | jq -r '.items[]? | select(.status.instanceState != "Deleted") | .metadata.name'
  done | sort | paste -sd ' ' -
}

# ---- the edits of the lab ----------------------------------------------------------------------------

# addon_from: the line of the manifest of the add-on profile that terraform/main.tf writes, and
# addon_to: the same line after the edit. An edit of addon_from into addon_to changes one manifest
# of the add-on profile, and no pack value: api.sh edit UID "$addon_from" "$addon_to".
addon_from='edited: "no"'
addon_to='edited: "yes"'

# kubeadm_from: the one line of the Kubernetes pack under postKubeadmCommands, with its indentation
# and its newline; the OS pack has the key too. kubeadm_to: the same line and one command more.
# An edit of kubeadm_from into kubeadm_to changes a key that Palette says repaves:
# api.sh edit UID "$kubeadm_from" "$kubeadm_to". kubeadm_repave: the line that strata compare
# --repave prints for that edit, which names the pack, the reason, and the key.
kubeadm_from=$'    - \'chmod 600 /var/lib/kubelet/config.yaml\'\n'
kubeadm_to=${kubeadm_from}$'    - \'echo strata-repave-test\'\n'
kubeadm_repave='palette    repave: PackValuesUpdated kubernetes (kubeadmconfig.postKubeadmCommands)'

# ---- the body of a create ----------------------------------------------------------------------------

# var NAME: the value of NAME in terraform/terraform.tfvars, or in the file TFVARS; empty when the
# file does not set it
var() { sed -n "s/^$1 *= *\"\(.*\)\"/\1/p" "${TFVARS:-$live/terraform/terraform.tfvars}" | head -n 1; }

# create_body ENGINE PROFILE NAME FILE: write the body of the create of a cluster named NAME to FILE,
# from preflight/ENGINE.json and the tenant: the profile with the uid PROFILE, the cloud account of
# terraform.tfvars, and its region and zone. ENGINE is aws, eks, gcp, or gke. It stops the script
# when the tenant has no such cloud account. It leaves the summary of the accounts in accounts.json.
# It sets the account uid in $A, and its facts in $account_name, $account_kind, $region, $az,
# $ssh_key, and $project. $account_kind is the kind of the cloud account: aws or gcp.
create_body() {
  local engine=$1 profile=$2 name=$3 file=$4
  case "$engine" in
    aws|eks)
      account_name=$(var cloud_account_name)
      account_kind=aws; region=$(var region); region=${region:-us-east-2}; az=$(var az); az=${az:-${region}a}
      ssh_key=$(var ssh_key_name); project=""
      ;;
    gcp|gke)
      account_name=$(var gcp_cloud_account_name); account_kind=gcp
      region=$(var gcp_region); region=${region:-us-east1}; az=$(var gcp_zone); az=${az:-${region}-b}
      ssh_key=""; project=$(var gcp_project)
      ;;
  esac
  # the name_prefix of the example defaults to strata-live
  [ "$engine" = eks ] && account_name="$( { var name_prefix; } | grep . || echo strata-live)-eks"
  "$live/api.sh" get "/v1/cloudaccounts/summary" > accounts.json 2> accounts.err || die "cannot list the cloud accounts; see $PWD/accounts.err"
  A=$(jq -r --arg n "$account_name" --arg k "$account_kind" '.items[] | select(.metadata.name == $n and .kind == $k) | .metadata.uid' accounts.json | head -n 1)
  [ -n "$A" ] || die "no cloud account named $account_name of kind $account_kind in the project; the summary lists: $(jq -r '.items[] | "\(.metadata.name) (\(.kind))"' accounts.json | paste -sd ', ' -)"
  sed -e "s/__ACCOUNT_UID__/$A/g" -e "s/__PROFILE_UID__/$profile/g" -e "s/__REGION__/$region/g" -e "s/__AZ__/$az/g" \
      -e "s/__SSH_KEY__/$ssh_key/g" -e "s/__PROJECT__/$project/g" -e "s/__NAME__/$name/g" \
      "$live/preflight/$engine.json" > "$file"
}

# ---- the packs of the lab registry --------------------------------------------------------------------

# The registry of the lab and the pack versions in it that the scripts use. pack-states.sh shows what
# Palette does with each state. addon_pack is a pack that Palette takes in an add-on profile. marked
# has the mark disabled of Spectro Cloud and no system_state (#125). deleted has system_state deleted.
# public_repo is the default of the Terraform variable registry_name in terraform/variables.tf: a
# lab that names its registry in another way changes both.
public_repo="Public Repo"
addon_pack=cert-manager
addon_pack_version=1.21.1
marked=gloo
marked_version=1.2.18
deleted=generic-vm-libvirt
deleted_version=1.0.0

# registry_uid NAME: the uid of the registry NAME in the project; empty when the key cannot see it
registry_uid() {
  "$live/api.sh" get /v1/registries/metadata | jq -r --arg n "$1" '.items[] | select(.name == $n) | .uid'
}

# ---- checks ------------------------------------------------------------------------------------------

failures=0
# check ID EXPECTED WHAT -- COMMAND...: run COMMAND with its output in ID.out and ID.err, and
# compare its exit code with EXPECTED; `?` records the code without judging it. The result goes to
# steps.log and WHAT to checks.tsv; a resumed run appends to both, and summary_table reads them.
check() {
  local id=$1 want=$2 what=$3
  shift 4
  printf '%s\t%s\n' "$id" "$what" >> checks.tsv
  "$@" > "$id.out" 2> "$id.err"
  local got=$? verdict=PASS
  if [ "$want" = "?" ]; then
    verdict=SEEN
  elif [ "$got" != "$want" ]; then
    verdict=FAIL
    failures=$((failures + 1))
  fi
  printf '%-4s  %-32s expected %s, got %s\n' "$verdict" "$id" "$want" "$got" | tee -a steps.log
}

# summary_table: the checks in steps.log as a markdown table, with what each shows
summary_table() {
  echo "| result | check | expected exit | got | what it shows |"
  echo "|---|---|---|---|---|"
  [ -f steps.log ] || return 0
  command grep -E '^(PASS|FAIL|SEEN) ' steps.log | while read -r verdict id _ want _ got; do
    what=$(awk -F '\t' -v id="$id" '$1 == id { w = $2 } END { print w }' checks.tsv 2>/dev/null)
    echo "| $verdict | \`$id\` | ${want%,} | $got | $what |"
  done
}

# write_summary TITLE [NOTE...]: write summary.md, the report of the run. The file holds the title,
# the line "Run `<run id>` with `<strata version>`" followed by the words of NOTE, and
# summary_table. The first word of NOTE starts with the punctuation that follows the version. The
# run id is $run_id, or the name of the directory above $run. The line omits the version when no
# strata binary exists. A script calls this function in the `summary` that its EXIT trap runs, so
# NOTE holds the values that the run found.
write_summary() {
  local title=$1 with=""
  shift
  [ ! -x "$strata" ] || with=" with \`$("$strata" --version)\`"
  {
    echo "# $title"
    echo
    echo "Run \`${run_id:-$(basename "$(dirname "$run")")}\`$with$*"
    echo
    summary_table
  } > summary.md
}

# ---- OpenTofu ----------------------------------------------------------------------------------------
#
# The provider's immutable-clusterprofiles flag (variable immutable_profiles) governs only profile
# resources, and sets skip_destroy on every profile. Every apply runs with the script's flag, except
# the one that restores the baseline (apply_baseline).

applies=$(ls apply-*.txt 2>/dev/null | wc -l)
# tf_apply FLAG ARG...: tofu apply with immutable_profiles=FLAG; the output goes to
# apply-<n>.txt. A failed apply saves diagnostics and stops the script.
tf_apply() {
  local with=$1
  shift
  applies=$((applies + 1))
  echo "tofu apply -var immutable_profiles=$with $*" | tee -a steps.log
  "$live/tf.sh" apply -input=false -auto-approve -no-color -var "immutable_profiles=$with" "$@" \
    > "apply-$applies.txt" 2>&1
  local code=$?
  # Palette now and then refuses one read of a profile with OperationForbidden and no cause, and
  # the next read passes. One apply again, when that was the failure.
  if [ "$code" != 0 ] && command grep -q 'OperationForbidden' "apply-$applies.txt"; then
    echo "tofu apply again: Palette refused a read with OperationForbidden" | tee -a steps.log
    "$live/tf.sh" apply -input=false -auto-approve -no-color -var "immutable_profiles=$with" "$@" \
      >> "apply-$applies.txt" 2>&1
    code=$?
  fi
  command grep -E '^(Plan|Apply complete|Error)' "apply-$applies.txt" | tee -a steps.log
  if [ "$code" != 0 ]; then
    "$live/diagnose.sh" "$PWD/diagnostics" >&2
    die "tofu apply failed; see $PWD/apply-$applies.txt and $PWD/diagnostics"
  fi
  load_outputs
}
# apply ARG...: an apply with the script's flag
apply() { tf_apply "$flag" "$@"; }
# try_apply ARG...: an apply whose refusal is the result. The exit code is tofu's, the output goes
# to stdout, and the script goes on; a check judges the code. An OperationForbidden read runs it
# again, as tf_apply does.
try_apply() {
  applies=$((applies + 1))
  echo "tofu apply -var immutable_profiles=$flag $*" >> steps.log
  "$live/tf.sh" apply -input=false -auto-approve -no-color -var "immutable_profiles=$flag" "$@" \
    > "apply-$applies.txt" 2>&1
  local code=$?
  if [ "$code" != 0 ] && command grep -q 'OperationForbidden' "apply-$applies.txt"; then
    echo "tofu apply again: Palette refused a read with OperationForbidden" >> steps.log
    "$live/tf.sh" apply -input=false -auto-approve -no-color -var "immutable_profiles=$flag" "$@" \
      >> "apply-$applies.txt" 2>&1
    code=$?
  fi
  command grep -E '^(Plan|Apply complete|Error)' "apply-$applies.txt" >> steps.log
  cat "apply-$applies.txt"
  load_outputs
  return "$code"
}
# hold_versions: at the baseline, store skip_destroy for the script's flag on every profile in the
# state, changing nothing else. Every script that applies runs it right after require_baseline. A
# flag-on version bump replaces the profile resource, and the provider deletes the replaced object with
# the skip_destroy its state holds, not the new value (provider 0.30.1,
# resourceClusterProfileDelete): without this, the bump asks Palette to delete the old version,
# which Palette refuses while a cluster runs it (DeletionResourceInUseError, drift-report.sh on,
# run 20260917T132642Z).
hold_versions() { tf_apply "$flag"; }
# apply_baseline: the default variables with the flag off, which is the baseline's configuration;
# skip_destroy goes back to false on every profile. A profile resource this removes stays in
# Palette when its state held skip_destroy = true.
apply_baseline() { tf_apply false; }
# tf_plan FLAG ARG...: tofu plan with -detailed-exitcode: 0 no changes, 2 changes, 1 refused
tf_plan() { local with=$1; shift; "$live/tf.sh" plan -input=false -no-color -detailed-exitcode -var "immutable_profiles=$with" "$@"; }

# ---- clusters ------------------------------------------------------------------------------------------

# wait_running UID: wait up to an hour for the cluster to be Running; Palette does not carry out
# the deletion of a cluster that is still provisioning. Saves diagnostics and stops the script when
# it does not get there.
wait_running() {
  local state=""
  for _ in $(seq 1 60); do
    state=$("$live/tenant.sh" cluster "$1" 2>&1)
    case "$state" in *": Running") return 0 ;; esac
    sleep 60
  done
  "$live/diagnose.sh" "$PWD/diagnostics" >&2
  die "cluster $1 is not Running after an hour: $state; see $PWD/diagnostics"
}

# kube CLUSTER_UID ARGS...: run kubectl with ARGS inside the cluster, and print its output and its
# errors. The kubeconfig lives in a temporary file that this function deletes. It is a credential:
# no script saves it or prints it to a log.
kube() {
  local k rc cluster=$1
  shift
  k=$(mktemp)
  "$live/api.sh" get-kubeconfig "$cluster" > "$k" 2> /dev/null
  KUBECONFIG=$k timeout 60 kubectl "$@" 2>&1
  rc=$?
  rm -f "$k"
  return "$rc"
}

# ---- baseline ------------------------------------------------------------------------------------------

# require_baseline: stop unless the live example is at its baseline
require_baseline() {
  "$live/baseline.sh" > baseline-before.txt 2>&1 \
    || die "the live example is not at its baseline; see $PWD/baseline-before.txt (just live-down && just live-up rebuilds it)"
}

# restore_addon UID RECORD: put the add-on profile back at its baseline, whatever the flag did to
# it: uid UID holds version 1.0.0 with the content of RECORD (a record of it at the baseline),
# OpenTofu manages UID, no other version of its name exists, and cluster A runs it. The steps are
# the same with the flag off and on; each changes nothing where there is nothing to undo.
restore_addon() {
  local uid=$1 record=$2 name current v
  load_outputs
  name=$(out addon_profile_name)
  # 1. the content: undo an in-place edit that is still there
  if ! "$strata" compare "$record" "$uid" >/dev/null 2>&1; then
    "$live/api.sh" edit "$uid" "$addon_to" "$addon_from" | tee -a steps.log || die "cannot undo the edit of $uid"
  fi
  # 2. OpenTofu manages the uid the cluster runs
  current=$(out addon_profile_uid)
  if [ "$current" != "$uid" ]; then
    "$live/tf.sh" state rm spectrocloud_cluster_profile.addon >> steps.log 2>&1 || die "cannot remove $current from the state"
    "$live/tf.sh" import -input=false -no-color spectrocloud_cluster_profile.addon "$uid:project" >> steps.log 2>&1 \
      || die "cannot import $uid"
  fi
  # 3. no other version of the name
  for v in $("$strata" list profiles --json | jq -r --arg n "$name" --arg u "$uid" \
      '.profiles[] | select(.name == $n and .uid != $u) | .uid'); do
    "$live/api.sh" delete "$v" | tee -a steps.log
  done
  # 4. the default variables: version 1.0.0, the clusters on the resource's uid, and skip_destroy off
  apply_baseline
  # 5. cluster A runs it
  "$live/api.sh" apply "$(out cluster_uid)" | tee -a steps.log || die "cannot apply the add-on profile to cluster A"
  for _ in $(seq 1 30); do
    "$strata" compare "$uid" "cluster:$(out cluster_uid)/$name" >/dev/null 2>&1 && return 0
    sleep 30
  done
  die "cluster A does not run the restored add-on profile after 15 minutes"
}

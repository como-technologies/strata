# Shared by the scripts in examples/live: the strata binary, the OpenTofu outputs, the usage and the
# stages, the checks and their summary, the markdown of a text, the edits of the lab, the OpenTofu
# applies, and the baseline.
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

# ---- git ---------------------------------------------------------------------------------------------

# isolate_git NAME: give each git command of the script the author NAME and no setting of the
# machine. Git reads no global or system file, and no variable whose name starts with GIT_CONFIG.
# Git reads GIT_CONFIG_COUNT and GIT_CONFIG_PARAMETERS after the config of a repository, so their
# settings win. tests/common/git.rs gives the same settings to the tests and the chapters of the book.
isolate_git() {
  local name
  for name in $(compgen -e); do
    case "$name" in GIT_CONFIG*) unset "$name" ;; esac
  done
  export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
  export GIT_AUTHOR_NAME="$1" GIT_AUTHOR_EMAIL="$1@example.com"
  export GIT_COMMITTER_NAME="$1" GIT_COMMITTER_EMAIL="$1@example.com"
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

# answered STATUS FILE...: the first line of each FILE, the output of api.sh get-status or
# post-status, is HTTP STATUS
answered() {
  local status=$1 f
  shift
  for f in "$@"; do [ "$(head -n 1 "$f")" = "HTTP $status" ] || return 1; done
}
# body_of FILE: the body of the answer that FILE holds after its status line
body_of() { sed 1d "$1"; }
# answer_is STATUS FILE FILTER [OPTION...]: FILE holds HTTP STATUS, and the jq FILTER is true of
# its body. Each OPTION goes to jq, such as --slurpfile d FILE.
answer_is() {
  local status=$1 file=$2 filter=$3
  shift 3
  answered "$status" "$file" && body_of "$file" | jq -e "$@" "$filter" > /dev/null
}
# refused STATUS CODE FILE: FILE holds HTTP STATUS and an answer with the code CODE
refused() { answer_is "$1" "$3" '.code == $c' --arg c "$2"; }

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

# ---- markdown ----------------------------------------------------------------------------------------

# The jq functions that write a text into markdown, as strata writes it. `html` is
# `text::html_text`: each `&`, `<`, and `>` is an entity, so a tag of HTML shows as text. `cell` is
# `text::markdown_cell`: `marks` puts a `\` before each mark of markdown, and `table` escapes each
# `|` and writes a line break as a space. So a link, an emphasis, or a code span of the text shows
# as text. A run of `_` with a letter or a digit on each side starts no emphasis, so `marks` keeps
# it as it is. `span` is `text::code_span`: its fence is longer than each run of backticks in the
# text. `code` is `text::code_cell`, a `span` in a cell of a table. `fence` is the fence of
# `text::code_block`, so that no line of the text ends the block.
markdown_jq='def html: split("&") | join("&amp;") | split("<") | join("&lt;") | split(">") | join("&gt;");
  def marks: gsub("(?<m>[\\\\`*\\[\\]~]|(?<![[:alnum:]_])_+|_+(?![[:alnum:]_]))"; .m | explode | map(92, .) | implode);
  def table: split("|") | join("\\|") | split("\n") | join(" ");
  def cell: marks | html | table;
  def backticks($n): [range($n)] | map("`") | join("");
  def ticks: [scan("`+") | length] | max // 0;
  def span: backticks(ticks + 1) as $f | (if startswith("`") or endswith("`") then " " else "" end) as $p
    | $f + $p + . + $p + $f;
  def code: span | table;
  def fence: backticks([ticks + 1, 3] | max);'

# md FUNCTION TEXT: TEXT through the function FUNCTION of markdown_jq, such as `cell` or `span`
md() { jq -rn --arg t "$2" "$markdown_jq \$t | $1"; }

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
# the skip_destroy its state holds, not the new value (provider 0.30.1): without this, the bump asks
# Palette to delete the old version, which Palette refuses while a cluster runs it
# (DeletionResourceInUseError, drift-report.sh on, run 20260917T132642Z).
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
# errors, with a limit of 60 seconds. KUBE_TIMEOUT gives another limit in seconds. KUBE_STDERR=1
# keeps the errors on standard error, so that a caller can read JSON. The kubeconfig lives in a
# temporary file that this function deletes. It is a credential: no script saves it or prints it to a
# log. KUBE_FILE names a kubeconfig that the caller keeps: the function reads it, and does not
# fetch or delete a kubeconfig.
kube() {
  local k rc cluster=$1
  shift
  if [ -n "${KUBE_FILE:-}" ]; then
    k=$KUBE_FILE
  else
    k=$(mktemp)
    "$live/api.sh" get-kubeconfig "$cluster" > "$k" 2> /dev/null
  fi
  if [ -n "${KUBE_STDERR:-}" ]; then
    KUBECONFIG=$k timeout "${KUBE_TIMEOUT:-60}" kubectl "$@"
  else
    KUBECONFIG=$k timeout "${KUBE_TIMEOUT:-60}" kubectl "$@" 2>&1
  fi
  rc=$?
  [ -n "${KUBE_FILE:-}" ] || rm -f "$k"
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

# ---- the observer ------------------------------------------------------------------------------------

# from_env_file NAME: the value of NAME in ~/.palette.env; empty when the file or the name is absent
from_env_file() {
  [ -f "$HOME/.palette.env" ] || return 0
  (set -a; . "$HOME/.palette.env" >/dev/null 2>&1; printf %s "${!1:-}")
}

# palette_settings: set $palette_key and $API from the environment or ~/.palette.env, as api.sh reads
# them. It stops the script when there is no key.
palette_settings() {
  palette_key=${PALETTE_API_KEY:-$(from_env_file PALETTE_API_KEY)}
  [ -n "$palette_key" ] || die "set PALETTE_API_KEY or put it in ~/.palette.env"
  API=${PALETTE_API:-$(from_env_file PALETTE_API)}
  API=${API:-https://api.spectrocloud.com}
}

# The two objects that the observer needs and that deploy/observer does not hold. Each function runs
# `kc`, the kubectl of the cluster, and reads $ns, $C, $API, and $PALETTE_PROJECT_UID. The script
# defines them.
# create_apply ARG...: make the object that `kubectl create ARG...` describes in the namespace $ns, or
# repair it when it exists. The create only writes the object as YAML, and apply sends it, so a second
# run works. Standard input goes to the create, so a key or a token reaches no argument and no file.
create_apply() { KUBE_STDERR=1 kc -n "$ns" create "$@" --dry-run=client -o yaml | kc apply -f -; }
# observer_config: the ConfigMap with the cluster, the address of Palette, and the project
observer_config() {
  create_apply configmap strata-observer --from-literal=cluster-uid="$C" \
    --from-literal=palette-api="$API" --from-literal=project-uid="$PALETTE_PROJECT_UID"
}
# observer_key KEY: the Secret of the key. The key goes through standard input, and it is in no log.
observer_key() {
  printf %s "$1" | create_apply secret generic strata-observer-key --from-file=api-key=/dev/stdin
}
# other_pod_refused: a pod of the namespace default cannot connect to the pod of the observer on its
# port. The test pod prints "refused" when wget fails, so a pod that did not start is no refusal.
other_pod_refused() {
  local ip answer
  ip=$(kc -n "$ns" get pod -l app.kubernetes.io/name=strata-observer -o jsonpath='{.items[0].status.podIP}') \
    && [ -n "$ip" ] || return 1
  answer=$(kc -n default run observer-probe --rm -i --restart=Never --image=busybox:1.37 --command -- \
    sh -c "wget -T 5 -qO- http://$ip:8080/healthz && echo reached || echo refused" < /dev/null)
  grep -qx refused <<<"$answer"
}

# The kubectl of cluster A, and the waits and the page that the scripts of the observer share. A script
# sets `ns`, `port`, `root`, and `C` before it calls them.
# kc ARGS...: kubectl in cluster A. kj does the same with the errors on standard error, for JSON.
# observer-kind.sh defines kc again for its kind cluster.
kc() { kube "$C" "$@"; }
kj() { KUBE_STDERR=1 kube "$C" "$@"; }
# can_i ARGS...: yes or no, as kubectl auth can-i gives it
can_i() { kc auth can-i "$@" | tail -n 1; }
# not_found ARGS...: kubectl in cluster A says that ARGS name nothing
not_found() {
  local answer
  answer=$(kc "$@")
  case "$answer" in
    *"not found"*) return 0 ;;
  esac
  return 1
}
# poll COUNT COMMAND...: run COMMAND until it succeeds, at most COUNT times, 5 seconds apart
poll() {
  local count=$1 _
  shift
  for _ in $(seq "$count"); do
    "$@" && return 0
    sleep 5
  done
  return 1
}
# uri TEXT: TEXT as one segment of a path
uri() { jq -rn --arg r "$1" '$r | @uri'; }

# The page of the observer, through kubectl port-forward. The kubeconfig is a credential: it lives in a
# temporary file until the end of the run, and the script deletes it.
kubeconfig= pf_pid= pf_user=
# cluster_kubeconfig: the kubeconfig of the cluster that the run reads, on standard output. observer-kind.sh
# defines it again for its kind cluster.
cluster_kubeconfig() { "$live/api.sh" get-kubeconfig "$C"; }
# pf_stop: end the port-forward
pf_stop() {
  [ -z "$pf_pid" ] || { kill "$pf_pid" 2> /dev/null; wait "$pf_pid" 2> /dev/null; }
  pf_pid= pf_user=
}
# pf_start [USER]: open kubectl port-forward deploy/strata-observer, as USER when it is given. The log is
# port-forward.log. The wait ends at the line "Forwarding from" in the log, or at the end of the process.
# Its limit is 30 seconds, not 30 looks: the stand-in sleep of the tests returns at once, so 30 looks take
# less than a second there. When the function fails, it says why on standard error.
pf_start() {
  local end why
  pf_stop
  if [ ! -s "$kubeconfig" ]; then
    kubeconfig=$(mktemp) || return 1
    if ! cluster_kubeconfig > "$kubeconfig" 2> /dev/null || [ ! -s "$kubeconfig" ]; then
      rm -f "$kubeconfig"
      kubeconfig=
      echo "the port-forward got no kubeconfig of the cluster" >&2
      return 1
    fi
  fi
  kubectl --kubeconfig "$kubeconfig" ${1:+--as="$1"} -n "$ns" port-forward deploy/strata-observer "$port:8080" > port-forward.log 2>&1 < /dev/null &
  pf_pid=$! pf_user=${1:-}
  end=$((SECONDS + 30)) why="printed no line \"Forwarding from\" in 30 seconds"
  while [ "$SECONDS" -lt "$end" ]; do
    grep -q 'Forwarding from' port-forward.log && return 0
    # the process can end after it prints its line, so the log gets a last look
    if ! kill -0 "$pf_pid" 2> /dev/null; then
      grep -q 'Forwarding from' port-forward.log && return 0
      why="ended with no line \"Forwarding from\""
      break
    fi
    sleep 1
  done
  { echo "the port-forward $why. Its log:"; cat port-forward.log; } >&2
  return 1
}
# page PATH: the answer of the observer through the port-forward
page() { curl -fsS --max-time 20 "http://127.0.0.1:$port$1"; }
# images_read: /v1/images says that the observer read the pods and found images. images.json keeps the answer.
images_read() {
  page /v1/images > images.json 2> /dev/null \
    && jq -e '.pods == "read" and .readAt != null and .error == null and .counts.images > 0' images.json > /dev/null
}
# pods_read: open the port-forward, and wait for the first read of the pods. $tries is the number of looks.
# The output says what the observer reports about the read.
pods_read() {
  pf_start || return 1
  poll "$tries" images_read || { jq '{pods, readAt, error, counts}' images.json; return 1; }
  jq '{pods, readAt, error, counts}' images.json
}

# The image of the observer. It is on a public package of GitHub, so a node pulls it with no pull
# Secret. A run never pushes it to another registry, so the pod runs the image that CI built.
# observer_image [TAG]: set $image from OBSERVER_IMAGE (default: the image with the tag TAG, or with the
# tag of the version in Cargo.toml), and $image_name and $image_pin for a kustomization. It stops the
# script for an image with neither a tag nor a digest.
observer_image() {
  local version
  version=$(grep -m1 '^version' "$live/../../Cargo.toml" | cut -d'"' -f2)
  image=${OBSERVER_IMAGE:-ghcr.io/como-technologies/strata-observer:${1:-v$version}}
  case "$image" in
    *@sha256:*) image_name=${image%%@*}; image_pin="digest: ${image#*@}" ;;
    *:*) image_name=${image%:*}; image_pin="newTag: ${image##*:}" ;;
    *) die "OBSERVER_IMAGE needs a tag or a digest" ;;
  esac
  case "${image##*:}" in */*) die "OBSERVER_IMAGE needs a tag or a digest" ;; esac
}
# observer_namespace [FLAG...]: apply the namespace of the observer from its file, with each FLAG, such
# as --server-side. The namespace has the label of the restricted standard. The ConfigMap and the
# Secret of the key go into it.
observer_namespace() { kc apply "$@" -f "$root/deploy/observer/namespace.yaml"; }
# no_pull_secret: the pod of the observer runs the image of the run, and it names no pull Secret. So a
# node pulled the image from the public package with no login. pull.json keeps the pod, and the output
# shows the image, the image id, and the pull Secrets.
no_pull_secret() {
  kj -n "$ns" get pod -l app.kubernetes.io/name=strata-observer -o json > pull.json || return 1
  jq '.items[0] | {image: .spec.containers[0].image, imageID: .status.containerStatuses[0].imageID,
    imagePullSecrets: (.spec.imagePullSecrets // [])}' pull.json
  jq -e --arg i "$image" '.items[0] | (.spec.imagePullSecrets // [] | length == 0)
    and .spec.containers[0].image == $i and (.status.containerStatuses[0].imageID // "") != ""' pull.json > /dev/null
}
# observer_overlay ARG...: base/ is a copy of deploy/observer. kustomization.yaml sets the image, and adds
# each ARG to the arguments of the pod, such as --content-every=1m, so that a run takes minutes.
# observer_image sets the image first.
observer_overlay() {
  local arg
  mkdir -p base && cp "$root"/deploy/observer/*.yaml base/ || return 1
  {
    cat << YAML
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
resources:
  - base
images:
  - name: ghcr.io/como-technologies/strata-observer
    newName: $image_name
    $image_pin
YAML
    [ $# = 0 ] || printf 'patches:\n  - target:\n      kind: Deployment\n      name: strata-observer\n    patch: |-\n'
    for arg in "$@"; do
      printf '      - op: add\n        path: /spec/template/spec/containers/0/args/-\n        value: %s\n' "$arg"
    done
  } > kustomization.yaml
}
# observer_overlay_pods: add the component deploy/observer/pods to the overlay that observer_overlay
# wrote. pods/ is a copy of the component, and kustomization.yaml lists it as a team does.
observer_overlay_pods() {
  mkdir -p pods && cp "$root"/deploy/observer/pods/*.yaml pods/ && printf 'components:\n  - pods\n' >> kustomization.yaml
}
# observer_render: render the overlay in the current folder to observer.yaml. The render needs no cluster.
observer_render() { kubectl kustomize . > observer.yaml && [ -s observer.yaml ]; }
# observer_ready: the Deployment of the observer is Available in 3 minutes
observer_ready() { KUBE_TIMEOUT=200 kc -n "$ns" rollout status deploy/strata-observer --timeout=180s; }

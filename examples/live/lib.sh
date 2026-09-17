# Shared by the scripts in examples/live: the strata binary, Terraform's outputs, the checks and
# their summary, the Terraform applies, and the baseline. Source it, then set `script` to the
# script's name and `flag` to the immutable-clusterprofiles setting the script runs with.
#
# The baseline is the state `just live-up` builds: cluster A Running the release and add-on
# profiles' current content, no cluster B, every profile name holding only the versions
# Terraform created, and a Terraform plan with the default variables that changes nothing.
# baseline.sh checks it. Every script requires it before it starts and leaves it as it found it,
# so the scripts run in any order.

live=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
strata=${STRATA_BIN:-$(cd "$live/../.." && pwd)/target/release/strata}
script=${script:-live}
flag=${flag:-false}

die() { echo "$script: $*" >&2; exit 2; }

# The default binary must be built from these sources: a run with an older one tests old code.
# `just build`, and each live recipe, builds it. A binary set with STRATA_BIN is the caller's.
if [ -z "${STRATA_BIN:-}" ] && [ -e "$strata" ]; then
  stale=$(find "$live/../../src" "$live/../../Cargo.toml" "$live/../../Cargo.lock" -type f -newer "$strata" -print -quit 2>/dev/null)
  [ -z "$stale" ] || die "$strata is older than $(cd "$(dirname "$stale")" && pwd)/$(basename "$stale"); run just build"
fi

# ---- Terraform outputs ---------------------------------------------------------------------------

# load_outputs: read Terraform's outputs again; every apply here does it for you
load_outputs() { outputs=$("$live/tf.sh" output -json 2>/dev/null) || die "no terraform outputs; run just live-up first"; }
# out NAME: one output, as text; empty when it is not set
out() {
  local json=${outputs:-}
  [ -n "$json" ] || json='{}'
  jq -r --arg k "$1" '.[$k].value // empty | if type == "string" then . else tojson end' <<<"$json"
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

# ---- Terraform ---------------------------------------------------------------------------------------
#
# The provider's immutable-clusterprofiles flag (variable immutable_profiles) governs only profile
# resources, and keep_profile_versions holds skip_destroy where the flag set it.
#
# Workaround, until the provider release after 0.29.7 that stops it: 0.29.7 refuses every flag-on
# plan that includes an existing profile whose version stays the same, even one that changes
# nothing (check plan-no-change). So an apply that changes a profile targets that profile
# (apply_profile), and an apply that changes only clusters runs with the flag off (apply_clusters).
# With the flag off, both rules change nothing. When plan-no-change passes with the flag on, remove
# the -target and the flag override here, and keep_profile_versions.

applies=$(ls apply-*.txt 2>/dev/null | wc -l)
# tf_apply FLAG KEEP ARG...: terraform apply with immutable_profiles=FLAG and
# keep_profile_versions=KEEP; the output goes to apply-<n>.txt. A failed apply saves diagnostics
# and stops the script.
tf_apply() {
  local with=$1 keep=$2
  shift 2
  applies=$((applies + 1))
  echo "terraform apply -var immutable_profiles=$with -var keep_profile_versions=$keep $*" | tee -a steps.log
  "$live/tf.sh" apply -input=false -auto-approve -no-color \
    -var "immutable_profiles=$with" -var "keep_profile_versions=$keep" "$@" > "apply-$applies.txt" 2>&1
  local code=$?
  command grep -E '^(Plan|Apply complete|Error)' "apply-$applies.txt" | tee -a steps.log
  if [ "$code" != 0 ]; then
    "$live/diagnose.sh" "$PWD/diagnostics" >&2
    die "terraform apply failed; see $PWD/apply-$applies.txt and $PWD/diagnostics"
  fi
  load_outputs
}
# apply_profile RESOURCE ARG...: an apply that changes one profile resource, with the script's flag.
# With the flag on it stops unless the resource's state already holds skip_destroy = true (see
# hold_versions); a resource not yet in the state is created with it.
apply_profile() {
  local r=$1 held
  shift
  if [ "$flag" = true ]; then
    held=$("$live/tf.sh" state show -no-color "$r" 2>/dev/null | awk '$1 == "skip_destroy" { print $3 }')
    [ "$held" != false ] \
      || die "$r holds skip_destroy = false in the Terraform state; run hold_versions first, because Terraform deletes a replaced version with the skip_destroy its state holds"
  fi
  tf_apply "$flag" "$flag" "-target=$r" "$@"
}
# hold_versions: at the baseline, store skip_destroy for the script's flag on every profile in the
# state, changing nothing else. A flag-on version bump replaces the profile resource, and Terraform
# deletes the replaced object with the skip_destroy its state holds (provider 0.29.7,
# resourceClusterProfileDelete), not the new value: without this, the bump that sets it asks
# Palette to delete the old version, which Palette refuses while a cluster runs it
# (DeletionResourceInUseError, drift-report.sh on, run 20260917T132642Z).
hold_versions() { tf_apply false "$flag"; }
# apply_clusters ARG...: an apply that changes only clusters, with skip_destroy held where the
# script's flag set it
apply_clusters() { tf_apply false "$flag" "$@"; }
# tf_plan FLAG ARG...: terraform plan with -detailed-exitcode: 0 no changes, 2 changes, 1 refused
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

# ---- baseline ------------------------------------------------------------------------------------------

# require_baseline: stop unless the live example is at its baseline
require_baseline() {
  "$live/baseline.sh" > baseline-before.txt 2>&1 \
    || die "the live example is not at its baseline; see $PWD/baseline-before.txt (just live-down && just live-up rebuilds it)"
}

# restore_addon UID RECORD: put the add-on profile back at its baseline, whatever the flag did to
# it: uid UID holds version 1.0.0 with the content of RECORD (a record of it at the baseline),
# Terraform manages UID, no other version of its name exists, and cluster A runs it. The steps are
# the same with the flag off and on; each changes nothing where there is nothing to undo.
restore_addon() {
  local uid=$1 record=$2 name current v
  load_outputs
  name=$(out addon_profile_name)
  # 1. the content: undo an in-place edit that is still there
  if ! "$strata" compare "$record" "$uid" >/dev/null 2>&1; then
    "$live/api.sh" edit "$uid" 'edited: "yes"' 'edited: "no"' | tee -a steps.log || die "cannot undo the edit of $uid"
  fi
  # 2. Terraform manages the uid the cluster runs
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
  # 4. the default variables: version 1.0.0, and the clusters on the resource's uid
  tf_apply false false
  # 5. cluster A runs it
  "$live/api.sh" apply "$(out cluster_uid)" | tee -a steps.log || die "cannot apply the add-on profile to cluster A"
  for _ in $(seq 1 30); do
    "$strata" compare "$uid" "cluster:$(out cluster_uid)/$name" >/dev/null 2>&1 && return 0
    sleep 30
  done
  die "cluster A does not run the restored add-on profile after 15 minutes"
}

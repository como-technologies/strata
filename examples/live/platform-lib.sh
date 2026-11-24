# The state of the platform cluster wilma (#700) and of the workload cluster pebbles (#701). platform.sh
# and platform-boundary.sh share this file: each one sources lib.sh, and then this file. lib.sh stops
# the send of a report, so this file sources it when no script did.
#
# terraform-platform/ holds both clusters in a state apart from the live example. pebbles is in a
# second project of the tenant. STRATA_LIVE_SECOND_PROJECT names that project, or the state of the
# last apply does. With neither, the state holds wilma alone.

[ -n "${live:-}" ] || . "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

platform=$live/terraform-platform
pins_file=$platform/terraform.tfvars
# The state of wilma is apart from the live example. Each plan, apply, destroy, and console of it
# reads the names of the tenant from the file of the lab.
export TF_DIR=$platform
for command in plan apply destroy console; do
  export "TF_CLI_ARGS_$command=-var-file=$live/terraform/terraform.tfvars"
done
export STRATA_QUIET=1

# The second project goes to each plan, apply, and destroy. It is STRATA_LIVE_SECOND_PROJECT, or the
# project that the state keeps from the last apply. A plan with another project replaces pebbles, so
# the script stops when the two differ.
saved_project=$("$live/tf.sh" output -json 2> /dev/null | jq -r '.second_project.value // empty' 2> /dev/null)
export TF_VAR_second_project=${STRATA_LIVE_SECOND_PROJECT:-$saved_project}
[ -z "$saved_project" ] || [ "$saved_project" = "$TF_VAR_second_project" ] \
  || die "the state holds pebbles in another project than STRATA_LIVE_SECOND_PROJECT; run platform.sh down with the project of the state first"

# ---- the outputs ----------------------------------------------------------------------------------

# platform_outputs: read the outputs, and set W, R, name, and the project of wilma. With pebbles in
# the state, it also sets P and workload, the uid and the name of pebbles. P2 is the uid of the second
# project. Each of the three is empty with no pebbles.
platform_outputs() {
  load_outputs
  W=$(out cluster_uid)
  R=$(out release_profile_uid)
  name=$(out cluster_name)
  PALETTE_PROJECT_UID=$(out project_uid)
  for v in W R name PALETTE_PROJECT_UID; do
    [ -n "${!v}" ] || die "the tofu output behind $v is empty; run examples/live/platform.sh up"
  done
  export PALETTE_PROJECT_UID
  P=$(out workload_uid)
  workload=$(out workload_name)
  P2=$(out second_project_uid)
  C=$W
  export STRATA_BIN=$strata strata C
}
# in_second COMMAND...: run COMMAND with the second project, as each read of pebbles needs
in_second() { PALETTE_PROJECT_UID=$P2 "$@"; }
# teams: the teams of the seven profiles, one on each line
teams() { out profile_uids | jq -r 'keys[]'; }
# team_uid TEAM: the uid of the profile of TEAM that the state holds
team_uid() { out profile_uids | jq -r --arg t "$1" '.[$t] // empty'; }

# other_version PACK: a version of PACK in the registry of its pin, other than the pin. It is the
# first such version with no state in the order of strata, so the newest one.
other_version() {
  "$strata" packs "$1" --cloud aws --json -q | jq -r --arg n "$1" --argjson p "$(out packs)" \
    '[.registries[] | select(.registry == $p[$n].registry) | .versions[]
      | select(.state == null and .version != $p[$n].version)][0].version // empty'
}

# ---- the readiness of the packs -------------------------------------------------------------------

# One line for each pack of the profiles in $want, from the cluster document: ready, waiting, or
# missing, with the reason of Palette. A pack is ready when its condition has the status True.
ready_jq='(.status.packs // []) as $have | $want[] | . as $w
  | ([$have[] | select(.name == $w.name and .profileUid == $w.uid)] | first) as $p
  | if $p == null then "missing \($w.name): Palette reports no status for it"
    elif ($p.condition.status // "") == "True" then "ready \($w.name): \($p.condition.reason // "")"
    else "waiting \($w.name): \($p.condition.reason // $p.condition.status // "no condition") \($p.condition.message // "" | .[:160])"
    end'
# packs_ready CLUSTER PROFILES: each pack of each add-on profile that the cluster CLUSTER runs is
# ready. PROFILES is the output that names those profiles. ready.txt keeps the lines.
packs_ready() {
  local uid
  "$live/api.sh" get "/v1/spectroclusters/$1" > ready-cluster.json || return 1
  for uid in $(out "$2" | jq -r '.[]'); do
    "$live/api.sh" get "/v1/clusterprofiles/$uid" | jq -c --arg u "$uid" '.spec.published.packs[] | {uid: $u, name}'
  done > ready-expected.jsonl || return 1
  jq -r --slurpfile want ready-expected.jsonl "$ready_jq" ready-cluster.json > ready.txt || return 1
  [ -s ready.txt ] && ! grep -qv '^ready ' ready.txt
}
# wait_ready [CLUSTER PROFILES]: wait up to 45 minutes until each pack of the cluster is ready. The
# cluster is wilma with the output on_cluster by default. The output is the last reading.
wait_ready() {
  local _
  for _ in $(seq 1 "${READY_TRIES:-135}"); do
    packs_ready "${1:-$W}" "${2:-on_cluster}" && { cat ready.txt; return 0; }
    sleep 20
  done
  cat ready.txt 2> /dev/null
  return 1
}

# ---- the applies of the cases ---------------------------------------------------------------------

# dirty: a case changed a variable of the state. ids.env keeps it, so that a run that resumes knows it.
dirty=
save() { echo "$1=$2" >> ids.env; }
# applied ARG...: an apply with the variables ARG. The output is the lines of the plan, of the end, and of
# each error. It returns the exit of tofu. The apply runs in this shell, so that it reads the outputs again.
applied() {
  local rc
  try_apply "$@" > case-apply.txt
  rc=$?
  grep -E '^(Plan|Apply complete|Error)|Code|code' case-apply.txt | head -n 20
  return "$rc"
}
# apply_case ARG...: an apply with the variables of one case
apply_case() {
  dirty=1 && save dirty 1
  applied "$@"
}
# apply_defaults: the defaults of terraform-platform/, which are the baseline of wilma and pebbles
apply_defaults() { applied && dirty= && save dirty ""; }
# baseline_ready: the defaults, and each pack of wilma is ready
baseline_ready() { apply_defaults && wait_ready; }

# require_platform: stop unless the state is at its baseline: a plan with the defaults changes nothing
require_platform() {
  tf_plan false > baseline-before.txt 2>&1 \
    || die "the platform is not at its baseline; see $PWD/baseline-before.txt (platform.sh down and up build it again)"
}

# case_stage N TITLE: the heading of case N, when the run holds case N
case_stage() { [ "$1" -le "$to" ] && stage "$@"; }

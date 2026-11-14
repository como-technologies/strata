#!/usr/bin/env bash
# The repave test (#73): `compare --repave` asks Palette if a change repaves
# cluster A, and prints the answer beside the level of the impact table. `compare --validate`
# asks Palette if it accepts the packs of the change (#134).
#
#   examples/live/repave.sh [FROM_STAGE]
#
#   1  at the baseline: a profile version equal to the copy of cluster A: Palette says no repave
#   2  the Kubernetes pack of the release version gets one more kubeadm command: Palette says
#      repave, for that pack and that key, and the impact table says it may repave
#   3  the same change, asked with tenant.sh: strata and the helper give one answer
#   4  a manifest of the add-on version changes: Palette says no repave, and the table says
#      in-place
#   5  a side that is not a cluster is a usage error, and the request changes nothing: cluster A
#      keeps its instances
#   6  a new value in the OS pack, and a changed value in the CSI pack, asked with tenant.sh and
#      with no edit of the profile: the answer of Palette for each layer that the impact table
#      calls may-repave and cluster-wide (#81)
#   7  `compare --validate` at the baseline: cluster A runs the release version, so strata asks the
#      route of that version (#453), and Palette accepts the packs of the version on
#      cluster A, and a side that is not a cluster is a usage error (#134)
#   8  version 1.0.1, which cluster A does not run, gets the profile variable strata_mode with the
#      default 600, through OpenTofu, and a kubeadm command of its Kubernetes pack reads it in
#      place of its 600. Palette is asked if 1.0.1 repaves cluster A: by strata, which sends no
#      variables, and by tenant.sh, with no variables, with the value 600, and with the value 644
#      as the control (#132). These three use the earlier body of tenant.sh, and one more question
#      uses the body of a swap (#864). Then 1.0.1 back. Cluster A takes no update
#   9  validate/repave saves the address of the console of the VM Orchestrator pack (#717). It
#      runs only with VMO_CLUSTER=<uid> of a cluster with the pack virtual-machine-orchestrator
#      and no cluster template. It reads the console address of the pack before and after each
#      question. A body with a mark in consoleBaseAddress moves the console to the mark, which
#      answers 502. A body with the values of the cluster moves it back. The stage stops before
#      the first question when a check before it fails. The forward of Palette keeps the address
#      that it read, usually for at most 15 minutes. So the stage waits VMO_WAIT seconds (default
#      960) before the first question. After each question, it reads the console for at most
#      VMO_WAIT seconds
#
# AWS and GCP run one upgrade engine, so every answer here agrees with the table. A disagreement
# needs another engine, such as Edge, and no live run has one. Needs the live example at its baseline with
# cluster A (just live-up), the strata release binary, jq, and the key as verify.sh needs it. Each
# stage puts its change back and takes no update on cluster A. Results go under
# runs/<UTC time>/repave/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0 only when no
# check fails.
set -uo pipefail
script=repave
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4|5|6|7|8|9) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"


run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/repave}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  write_summary "Palette repave test" ", cluster \`${C:-}\`," \
    "release \`${R:-}\`, add-on \`${AD:-}\`."
}
trap summary EXIT

load_outputs
C=$(out cluster_uid) R=$(out release_profile_uid) AD=$(out addon_profile_uid)
CN=$(out cluster_names | jq -r .a)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
release=$(out profile_name)
addon=$(out addon_profile_name)

if stage 1 "no change"; then
  require_baseline
  check none 0 "#73: a profile version equal to the copy of cluster A: the comparison is IDENTICAL, and Palette says no repave" -- \
    "$strata" compare "cluster:$C/$release" "$R" --repave
  check none-says 0 "#73: the output has a palette line: no repave" -- grep -q '^palette    no repave$' none.out
  check none-json 0 "#73: --json carries paletteRepave, required false, and agrees true" -- \
    bash -c "\"$strata\" compare cluster:$C/$release $R --repave --json | jq -e '.paletteRepave | .required == false and .agrees == true and .clusterUid == \"$C\"'"
fi

if stage 2 "a kubeadm command in the Kubernetes pack"; then
  check k8s-edit 0 "the Kubernetes pack of $release gets one more kubeadm command, through the API" -- \
    "$live/api.sh" edit "$R" "$kubeadm_from" "$kubeadm_to"
  check k8s 1 "#73: compare of the copy with the edited version: CHANGED, and Palette says repave" -- \
    "$strata" compare "cluster:$C/$release" "$R" --repave
  check k8s-says 0 "#73: Palette names the pack, the reason, and the key" -- \
    grep -qxF "$kubeadm_repave" k8s.out
  check k8s-agrees 0 "#73: the impact table says may-repave, and there is no disagreement note" -- \
    bash -c "grep -q 'impact: .*may-repave\|impact: .*repave' k8s.out; ! grep -q 'the impact table says' k8s.out"
  check k8s-json 0 "#73: --json carries the reason, with its key" -- \
    bash -c "\"$strata\" compare cluster:$C/$release $R --repave --json | jq -e '.paletteRepave | .required == true and .agrees == true and (.reasons[0].keys | index(\"kubeadmconfig.postKubeadmCommands\"))'"
  check k8s-reversed 1 "#73: the two sides in the other order give the same answer" -- \
    "$strata" compare "$R" "cluster:$C/$release" --repave
  check k8s-reversed-says 0 "and the same line" -- \
    grep -q "^${kubeadm_repave%% (*}" k8s-reversed.out
fi

if stage 3 "the helper gives the same answer"; then
  check helper 1 "#73: tenant.sh asks Palette for the same change: repave" -- \
    "$live/tenant.sh" repave "$C" "$R" "$R" kubernetes "$kubeadm_from" "$kubeadm_to"
  check helper-says 0 "#73: and names the same key" -- grep -q 'kubeadmconfig.postKubeadmCommands' helper.out
  check k8s-undo 0 "the Kubernetes pack back" -- "$live/api.sh" edit "$R" "$kubeadm_to" "$kubeadm_from"
fi

if stage 4 "a manifest of the add-on"; then
  check addon-edit 0 "the manifest of $addon changes, through the API" -- "$live/api.sh" edit "$AD" "$addon_from" "$addon_to"
  check addon 1 "#73: compare of the copy with the edited add-on: CHANGED, and Palette says no repave" -- \
    "$strata" compare "cluster:$C/$addon" "$AD" --repave
  check addon-says 0 "#73: the palette line says no repave" -- grep -q '^palette    no repave$' addon.out
  check addon-agrees 0 "#73: the table says in-place, and there is no disagreement note" -- \
    bash -c "grep -q 'in-place' addon.out; ! grep -q 'the impact table says' addon.out"
  check addon-undo 0 "the manifest back" -- "$live/api.sh" edit "$AD" "$addon_to" "$addon_from"
fi

if stage 5 "what the option refuses, and what it changes"; then
  before=$(instances)
  check usage 2 "#73: two profile versions and no cluster: a usage error" -- \
    "$strata" compare "$R" "$AD" --repave
  check usage-says 0 "and it says what it needs" -- grep -q 'needs one side that is a cluster' usage.err
  check no-update 0 "the questions took no update: cluster A has no update waiting" -- \
    bash -c "\"$strata\" snapshot cluster:$C | jq -e '.source.cluster.updatesPending == false'"
  after=$(instances)
  check same-nodes 0 "#73: the same machines after each question: nothing was repaved" -- test -n "$before" -a "$before" = "$after"
  check undo-baseline 0 "the example is at its baseline" -- "$live/baseline.sh"
fi

if stage 6 "a value of the OS pack, and a value of the CSI pack"; then
  check release-record 0 "a record of $release" -- "$strata" snapshot "$R" -o release.json
  os_pack=$(jq -r '[.packs[] | select(.layer == "os")][0].name' release.json)
  csi_pack=$(jq -r '[.packs[] | select(.layer == "csi")][0].name' release.json)
  os_first=$(jq -r '[.packs[] | select(.layer == "os")][0].values' release.json | head -1)
  [ -n "$os_first" ] || die "the OS pack of $release has no values"
  check os-value 1 "#81: a new key in the values of the OS pack: Palette says repave, for any value of the OS layer on this engine" -- \
    "$live/tenant.sh" repave "$C" "$R" "$R" "$os_pack" "$os_first" "strataRepaveTest: \"true\"
$os_first"
  check csi-value 0 "#81: a changed value in the CSI pack: Palette says no repave, and checks no CSI layer for one" -- \
    "$live/tenant.sh" repave "$C" "$R" "$R" "$csi_pack" "$(out csi_reclaim_delete)" "$(out csi_reclaim_delete | sed 's/Delete/Retain/')"
fi

if stage 7 "Palette's check of the packs"; then
  require_baseline
  check validate 0 "#134, #453: a profile version equal to the copy of cluster A: the comparison is IDENTICAL, and the route of that version accepts its packs" -- \
    "$strata" compare "cluster:$C/$release" "$R" --validate
  check validate-says 0 "#134: the output has a palette line: accepts" -- grep -q '^palette    accepts$' validate.out
  check validate-json 0 "#134: --json carries paletteValidation, accepts true, and no error" -- \
    bash -c "\"$strata\" compare cluster:$C/$release $R --validate --json | jq -e '.paletteValidation | .accepts == true and .clusterUid == \"$C\" and (has(\"errors\") | not)'"
  check validate-usage 2 "#134: two profile versions and no cluster: a usage error" -- \
    "$strata" compare "$R" "$AD" --validate
  check validate-usage-says 0 "and it says what it needs" -- grep -q '^strata: --validate needs one side that is a cluster' validate-usage.err
fi

if stage 8 "a profile variable in a repave key of a version that cluster A does not run"; then
  [ "$from" = 8 ] && require_baseline
  N=$(out next_profile_uid)
  check var-record-before 0 "a record of 1.0.1 before the stage" -- "$strata" snapshot "$N" -o next-before.json
  check var-command 0 "#132: the Kubernetes pack of 1.0.1 has the kubeadm command with 600" -- \
    jq -e 'any(.packs[]; .layer == "k8s" and (.values | contains("chmod 600 /var/lib/kubelet/config.yaml")))' next-before.json
  apply -var next_variable=true
  apply -var next_variable=true -var next_variable_in_values=true
  check var-record 0 "#132: a record of 1.0.1 with the variable" -- "$strata" snapshot "$N" -o next-variable.json
  check var-in-values 0 "#132: the kubeadm command of 1.0.1 reads the variable strata_mode" -- \
    jq -e 'any(.packs[]; .layer == "k8s" and (.values | contains("chmod {{.spectro.var.strata_mode}} /var/lib/kubelet/config.yaml")))' next-variable.json
  check var-strata ? "#132: compare --repave of cluster A with 1.0.1: strata sends no variables" -- \
    "$strata" compare "cluster:$C/$release" "$N" --repave --json
  check var-strata-answer ? "#132: Palette says repave (0), or no repave (1), to the request of strata" -- \
    jq -e '.paletteRepave.required' var-strata.out
  # the three questions of #132 keep the earlier body of tenant.sh: 1.0.0 alone, with
  # replaceWithProfile 1.0.1, so that each answer stays the answer to the body that it names
  check var-none ? "#132: tenant.sh with no variables: no repave (0), or repave (1)" -- \
    env REPAVE_FORM=replace "$live/tenant.sh" repave "$C" "$R" "$N"
  check var-default ? "#132: tenant.sh with strata_mode 600, the default: no repave (0), or repave (1)" -- \
    env REPAVE_FORM=replace REPAVE_VARIABLES='[{"name": "strata_mode", "value": "600"}]' "$live/tenant.sh" repave "$C" "$R" "$N"
  check var-control 1 "#132: tenant.sh with strata_mode 644: the command changes, so Palette says repave" -- \
    env REPAVE_FORM=replace REPAVE_VARIABLES='[{"name": "strata_mode", "value": "644"}]' "$live/tenant.sh" repave "$C" "$R" "$N"
  check var-control-says 0 "#132: and it names the Kubernetes pack and the kubeadm key" -- \
    grep -q 'kubeadmconfig.postKubeadmCommands' var-control.out
  check var-swap ? "#864: tenant.sh with the body of a swap: each profile of cluster A, with 1.0.1 in place of 1.0.0" -- \
    "$live/tenant.sh" repave "$C" "$R" "$N"
  apply -var next_variable=true
  apply
  check var-undone 0 "1.0.1 is as before the stage: its record from before verifies" -- "$strata" verify next-before.json
fi

# The console address of the pack forwards to the address that Palette saved for it. A mark under
# .invalid has no host, so the forward of the console answers 502 when Palette saved the mark.
vmo_pack=virtual-machine-orchestrator
vmo_mark=https://strata-live-check.invalid
# The packs whose console address Palette saves at validate/repave: vmo_pack, its older name, and
# the pack of the move of virtual machines
vm_packs='["virtual-machine-orchestrator", "spectro-vm-dashboard", "vm-migration-assistant"]'

# console_is WANT URL: print the HTTP status of the console address URL. True when the console
# reaches WANT. For "host", the forward finds its record (no 404), the host answers (no 502), and
# the answer comes in 30 seconds (no 000). For "mark", the forward answers 502. The request sends
# no key, because the console forwards each header to the address that Palette saved.
console_is() {
  local status
  status=$(curl -sk -m30 -o /dev/null -w '%{http_code}' "$2/")
  echo "$status"
  case "$1:$status" in
    mark:502) return 0 ;;
    mark:*|host:404|host:502|host:000) return 1 ;;
  esac
}

# console_check ID WANT WHAT: the check ID that the console address reaches WANT in VMO_WAIT
# seconds. It reads at once, and then each 5 seconds. The forward of Palette keeps the address
# that it read, usually for at most 15 minutes. In some cases, it keeps it until Palette starts
# the forward service again. So when no read passes, the stage names the forward as a cause.
console_check() {
  local id=$1 want=$2 what=$3 before=$failures
  check "$id" 0 "$what" -- poll $((wait / 5 + 1)) console_is "$want" "$console"
  [ "$failures" = "$before" ] || echo "== $id: no read in $wait seconds reached the $want." \
    "The forward of Palette can keep the address that it read before the question." \
    "Then the console forwards there until Palette starts the forward service again" | tee -a steps.log
}

# vmo_stopped FAILURES: true when a check failed after the count FAILURES. It then writes the stop
# of stage 9 to steps.log.
vmo_stopped() {
  [ "$failures" != "$1" ] || return 1
  echo "== stage 9 stopped: a check failed before the first question, so the stage asks Palette nothing" | tee -a steps.log
}

# vmo_body MARK: the body of a question about the cluster in vmo-cluster.out. Each profile goes by
# its uid. Each profile with one of vm_packs names each of its packs with the values of the
# cluster. A profile with pack values keeps only the packs that it names. A profile by its uid
# alone can move the address that the console of such a pack forwards to. A MARK goes into
# consoleBaseAddress of vmo_pack alone.
vmo_body() {
  jq --arg p "$vmo_pack" --argjson vm "$vm_packs" --arg mark "$1" '{profiles: [.spec.clusterProfileTemplates[]
    | {uid} + (if any(.packs[]; .name | IN($vm[])) then {packValues: [.packs[] | {name, tag, type, values: (if .name == $p and $mark != ""
        then (.values | sub("(?<i>[ ]*)consoleBaseAddress:[^\n]*"; "\(.i)consoleBaseAddress: \($mark)")) else .values end)}]}
      else {} end)]}' vmo-cluster.out
}

# vmo_stage CLUSTER: the checks of stage 9 on CLUSTER. They stop before the first question when a
# check before it fails. For a cluster without vmo_pack, the body holds no mark, so the question
# shows nothing.
vmo_stage() {
  V=$1 wait=${VMO_WAIT:-960}
  local before=$failures
  check vmo-cluster 0 "#717: a read of the cluster $V" -- "$live/api.sh" get "/v1/spectroclusters/$V"
  # Palette answers the route of a cluster that a cluster template governs with no reasons, and saves nothing
  check vmo-runs 0 "#717: the cluster runs $vmo_pack, its values hold consoleBaseAddress, and no cluster template governs it" -- \
    jq -e --arg p "$vmo_pack" '(.spec.clusterTemplate.uid // "") == "" and any(.spec.clusterProfileTemplates[].packs[]; .name == $p and (.values // "" | test("consoleBaseAddress:")))' vmo-cluster.out
  vmo_stopped "$before" && return
  check vmo-summary 0 "#717: the summary of the cluster gives the console address of the service vm-dashboard" -- \
    "$live/api.sh" get "/v1/dashboard/spectroclusters/$V"
  console=$(jq -r '[.status.addOnServices[]? | select(.name == "vm-dashboard") | .endpoint][0] // empty' vmo-summary.out)
  [ -n "$console" ] || die "the summary of cluster $V names no console address of vm-dashboard"
  vmo_body "$vmo_mark" > vmo-mark.json
  vmo_body "" > vmo-own.json
  check vmo-mark-body 0 "#717: the body of the question holds the mark in the values of $vmo_pack" -- \
    jq -e --arg mark "$vmo_mark" 'any(.profiles[].packValues[]?; .values | contains("consoleBaseAddress: \($mark)"))' vmo-mark.json
  check vmo-before 0 "#717: before the question, the console address forwards to a host that answers" -- \
    console_is host "$console"
  vmo_stopped "$before" && return
  # The forward keeps the address that vmo-before read. It usually removes it in this time.
  sleep "$wait"
  check vmo-ask 0 "#717: validate/repave with the mark in the values of $vmo_pack" -- \
    "$live/api.sh" post-status "/v1/spectroclusters/$V/validate/repave" vmo-mark.json
  check vmo-ask-ok 0 "#717: Palette answers the question with 200" -- grep -qx 'HTTP 200' vmo-ask.out
  console_check vmo-saved mark "#717: the console address forwards to the mark: Palette saved it at the question, and the forward answers 502"
  check vmo-own 0 "#717: validate/repave with the values of the cluster: Palette saves the address of the cluster again" -- \
    "$live/api.sh" post-status "/v1/spectroclusters/$V/validate/repave" vmo-own.json
  check vmo-own-ok 0 "#717: Palette answers the second question with 200" -- grep -qx 'HTTP 200' vmo-own.out
  console_check vmo-back host "#717: the console address forwards to a host that answers again"
}

if stage 9 "validate/repave saves the address of the console of the VM Orchestrator pack"; then
  if [ -z "${VMO_CLUSTER:-}" ]; then
    echo "== stage 9 skipped: give VMO_CLUSTER=<uid of a cluster with the pack $vmo_pack>" | tee -a steps.log
  else
    vmo_stage "$VMO_CLUSTER"
  fi
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

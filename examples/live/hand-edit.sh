#!/usr/bin/env bash
# The hand edit test (#691): what Palette does when a person changes or deletes an object of a
# manifest layer with kubectl.
#
#   examples/live/hand-edit.sh
#
#   1  the add-on profile barney-config, with one manifest layer: a Deployment of one replica.
#      Cluster A runs it, and the Deployment runs in cluster A
#   2  the hand edit: kubectl scales the Deployment to 2 replicas. A read each READ_EVERY minutes,
#      for READ_MINUTES minutes, shows whether the field gets the value of the manifest again.
#      Then what strata and Palette say
#   3  the hand delete: kubectl deletes the Deployment. The same reads show whether Palette makes it
#      again. Then what Palette says, and the events of cluster A since the edit
#   4  the cleanup: barney-config leaves cluster A, the Deployment goes, and Palette deletes the
#      profile
#
# What varies: the object in the cluster, through kubectl. What holds: the profile and its content,
# the cluster, and the baseline that the run starts from and returns to (lib.sh). A check with `?`
# records a result without judging it, because the run finds the result. The reads stop at the
# first read that shows the result, so the wait is shorter when Palette restores the object.
#
# The cleanup runs at the end of each run, also after a failed check or a signal. It removes only
# what the run makes. Needs the live example at its baseline with cluster A (just live-up), the
# strata release binary, jq, kubectl, and the key of verify.sh. The kubeconfig lives in a temporary
# file and is never saved. READ_EVERY is 5 and READ_MINUTES is 70 by default. The deployment of
# Palette sets the period of the reconcile, so the reads cover more than one hour. Results go under
# runs/<UTC time>/hand-edit/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=hand-edit
from=1
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ $# = 0 ] || { usage; exit 2; }
[ -x "$strata" ] || die "no strata binary at $strata"
command -v kubectl > /dev/null || die "kubectl is needed to change the Deployment inside the cluster"

name=barney-config
deploy=barney-app
every=${READ_EVERY:-5}
minutes=${READ_MINUTES:-70}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/hand-edit}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
C=$(out cluster_uid) P=$(out project_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$P
# HP: the uid of barney-config, after stage 1 makes it. attached: 1 while cluster A runs it.
# edited_at: the time of the hand edit, in seconds since 1970.
HP="" attached="" edited_at=""

# replicas: the field spec.replicas of the Deployment, read inside the cluster
replicas() { kj get deployment "$deploy" -n default -o jsonpath='{.spec.replicas}' 2> /dev/null; }
# deployment_uid: the uid of the Deployment, read inside the cluster. It is empty when the
# Deployment is gone.
deployment_uid() { kj get deployment "$deploy" -n default -o jsonpath='{.metadata.uid}' 2> /dev/null; }
# read_until TEST...: run TEST at minute 0, and then each $every minutes, until it succeeds or
# $minutes minutes pass. TEST prints what it reads. The output has one line for each read, with
# its minute.
read_until() {
  local m=0 seen rc
  while :; do
    seen=$("$@")
    rc=$?
    echo "minute $m: $seen"
    [ "$rc" != 0 ] || return 0
    [ "$m" -lt "$minutes" ] || return 1
    sleep $((every * 60))
    m=$((m + every))
  done
}
# manifest_replicas: the field holds the value of the manifest, 1. The output is the field.
manifest_replicas() {
  local value
  value=$(replicas)
  echo "spec.replicas ${value:-none}"
  [ "$value" = 1 ]
}
# made_again: a Deployment of the name runs again, with a uid other than the uid before the delete
made_again() {
  local uid
  uid=$(deployment_uid)
  echo "uid ${uid:-none}"
  [ -n "$uid" ] && [ "$uid" != "$(cat delete-uid.out)" ]
}
# make_profile: barney-config, with the Deployment as its one manifest layer. A profile of the name
# from an earlier run goes first.
make_profile() {
  deployment_manifest "$deploy" "$pause_image" > deployment.yaml || return 1
  "$live/api.sh" delete-named "$name" > /dev/null || return 1
  HP=$("$live/api.sh" manifest-profile "$name" "$name" deployment.yaml) && [ -n "$HP" ] || return 1
  echo "$HP"
}
# attach_profile: cluster A runs barney-config, and is Running
attach_profile() {
  attached=1
  "$live/api.sh" attach "$C" "$HP" && wait_running "$C"
}
# applied: the Deployment runs in cluster A within 20 minutes, with the value of the manifest
applied() { poll 240 manifest_replicas; }
# hand_edit: kubectl scales the Deployment to 2 replicas. edited_at keeps the time.
hand_edit() {
  edited_at=$(date -u +%s)
  kc scale deployment "$deploy" -n default --replicas=2
}
# events_since_edit: the reason and the message of each event of cluster A since the hand edit,
# newest first. An event has its time in metadata.creationTimestamp.
events_since_edit() {
  "$live/api.sh" get "/v1/events/components/spectrocluster/$C?limit=100" | jq -c --argjson t "$edited_at" \
    '.items[]? | select((.metadata.creationTimestamp // "" | sub("[.][0-9]+"; "") | fromdateiso8601? // 0) >= $t)
      | {time: .metadata.creationTimestamp, reason, message}'
}
# detach_profile: barney-config leaves cluster A, and the cluster is Running
detach_profile() {
  "$live/api.sh" detach "$C" "$HP" && attached= && wait_running "$C"
}
# removed: the Deployment goes from cluster A within 20 minutes
removed() { poll 240 deployment_absent "$deploy"; }
# delete_profile: Palette deletes barney-config
delete_profile() { "$live/api.sh" delete "$HP" && HP=; }
# clean: no Deployment of the run in cluster A, and no version of barney-config in the project
clean() { deployment_absent "$deploy" && no_profile_named "$name"; }

summary() {
  write_summary "hand edit test" ", cluster $(md span "$C"), profile $(md span "$name")," \
    "Deployment $(md span "$deploy"), a read each $(md span "$every") minutes for" \
    "$(md span "$minutes") minutes."
}
# cleanup: remove what the run made, whatever stage it reached. Each step changes nothing where
# there is nothing to remove. The profile goes only after it leaves cluster A.
cleanup() {
  if [ -n "$attached" ]; then
    "$live/api.sh" detach "$C" "$HP" >> steps.log 2>&1 && attached=
    # a subshell, because wait_running stops the script when the cluster does not get there
    (wait_running "$C") >> steps.log 2>&1
  fi
  delete_deployment "$deploy" >> steps.log 2>&1
  [ -z "$HP" ] || [ -n "$attached" ] || delete_profile >> steps.log 2>&1
  summary
}
trap cleanup EXIT

if stage 1 "barney-config on cluster A"; then
  require_baseline
  check profile 0 "#691: Palette makes $name, with a Deployment of one replica in one manifest layer" -- make_profile
  [ -n "$HP" ] || die "no profile $name; see $run/profile.err"
  check record 0 "record $name" -- "$strata" snapshot "$HP" -o profile.json
  check attach 0 "#691: cluster A runs $name" -- attach_profile
  check deployed 0 "#691: the Deployment runs in cluster A within 20 minutes, with 1 replica" -- applied
fi

if stage 2 "the hand edit"; then
  check edit 0 "#691: kubectl scales the Deployment to 2 replicas" -- hand_edit
  check edit-reads ? "#691: a read each $every minutes. 0: the field is 1 again within $minutes minutes, 1: it stays 2" -- \
    read_until manifest_replicas
  check edit-compare 0 "#691: compare finds no change of $name on cluster A: strata reads only what Palette holds" -- \
    "$strata" compare profile.json "cluster:$C/$name"
  check edit-pack-status ? "#691: what Palette reports for the pack of $name after the reads" -- pack_status "$C" "$HP"
fi

if stage 3 "the hand delete"; then
  check delete-uid 0 "#691: the uid of the Deployment before the delete" -- deployment_uid
  check delete 0 "#691: kubectl deletes the Deployment" -- delete_deployment "$deploy"
  check delete-reads ? "#691: a read each $every minutes. 0: Palette makes the Deployment again within $minutes minutes, 1: it stays gone" -- \
    read_until made_again
  check delete-pack-status ? "#691: what Palette reports for the pack of $name after the reads" -- pack_status "$C" "$HP"
  check events ? "#691: the events of cluster A since the hand edit, newest first" -- events_since_edit
fi

if stage 4 "the cleanup"; then
  check detach 0 "$name leaves cluster A" -- detach_profile
  check deployment-gone 0 "the Deployment goes from cluster A within 20 minutes" -- removed
  check profile-delete 0 "Palette deletes $name" -- delete_profile
  check clean 0 "no Deployment $deploy in cluster A, and no version of $name in the project" -- clean
  check baseline 0 'the example is at its baseline' -- "$live/baseline.sh"
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

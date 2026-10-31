#!/usr/bin/env bash
# The macro repave test (#72): when a pack in an infrastructure layer reads a project macro, does
# a change of the macro value alone replace the nodes of cluster A, and what does Palette record
# about it? The answer decides whether a change of a macro is a change that no record shows.
#
#   examples/live/macro-repave.sh [FROM_STAGE]
#
#   1  at the baseline: the machines of cluster A, its open notifications, and its state
#   2  the Kubernetes pack of the release version reads a project macro in postKubeadmCommands,
#      through the API; cluster A takes the update, and holds the same instances for STABLE_MINUTES
#      (default 12). This edit is a change of the profile, and Palette notifies for it
#   3  only the macro gets another value. For WATCH_MINUTES (default 45), every minute: the state,
#      the instances, the open notifications, and the value that resolvedValues gives
#   4  what each record says: the instances, the notifications, the audit entries of any resource
#      since the change, and verify on a cluster record with and without the macro digests
#   5  undo: the profile back. While the repave waits for approval, Palette refuses the update
#      of cluster A and the delete of the macro. A record of cluster A holds the repave that
#      waits (#107), and verify names it. The script ends here, and live-down removes the example
#
# It measures. It names nothing to evade: the record of each signal, and where it is missing, is
# the result. Needs the live example at its baseline with cluster A (just live-up), the strata
# release binary, jq, and the key as verify.sh needs it.
# Billable: cluster A can replace its nodes twice, about 20 minutes each. Results go under
# runs/<UTC time>/macro-repave/, or OUT; FROM_STAGE resumes a run in OUT. Each check compares an
# exit code with the expected one; `?` records it without judging. Exit 0 only when no check fails.
set -uo pipefail
script=macro-repave
flag=false
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4|5) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"

macro=strata_probe
edit_to=${kubeadm_from}$'    - \'echo {{.spectro.macro.strata_probe}}\'\n'
stable=${STABLE_MINUTES:-12}
watch_minutes=${WATCH_MINUTES:-45}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/macro-repave}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  write_summary "macro repave test" ", cluster \`${C:-}\`," \
    "release \`${R:-}\`, project macro \`$macro\`."
}
trap summary EXIT

save() { echo "$1=$2" >> ids.env; }
[ -f ids.env ] && . ./ids.env

load_outputs
C=$(out cluster_uid) R=$(out release_profile_uid) P=$(out project_uid)
CN=$(out cluster_names | jq -r .a)
[ -n "$C" ] && [ -n "$CN" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$P

open_notifications() {
  "$live/api.sh" get "/v1/notifications/spectrocluster/$C" \
    | jq -c '[.items[]? | select(.action.isDone == false) | [.action.events[]? | .message]]'
}
resolved() {
  "$live/api.sh" get "/v1/spectroclusters/$C/packs/resolvedValues" \
    | jq -r --arg k "{{.spectro.macro.$macro}}" '[.profiles[].resolved[$k]] | first // "none"'
}
state() { "$live/tenant.sh" cluster "$C" 2>&1 | sed 's/^[^:]*: //'; }
# audits_since TIME: every audit entry since TIME, any resource, up to five pages, one line each
audits_since() {
  local page next="" i
  for i in 1 2 3 4 5; do
    page=$("$live/api.sh" get "/v1/audits?startTime=$1&limit=100${next:+&continue=$next}") || return 1
    jq -r '.items[]? | [.metadata.creationTimestamp, .spec.actor.actorType, (.spec.actor.serviceName // .spec.actor.user.emailId // "-"), .spec.actionType, .spec.resource.kind, (.spec.resource.name // "-"), (.spec.actionMsg // "")] | @tsv' <<<"$page"
    next=$(jq -r '.listmeta.continue // ""' <<<"$page")
    [ -n "$next" ] || return 0
  done
}
# hold LABEL MINUTES: log every minute the state and the instances, until the instances stay the
# same and the state stays Running for MINUTES in a row. Stops the script after two hours.
hold() {
  local label=$1 need=$2 held=0 last="" now st
  for _ in $(seq 1 120); do
    now=$(instances)
    st=$(state)
    echo "$(date -u +%H:%M:%S) state=$st instances=$now" >> "hold-$label.log"
    case "$st" in
      Running*) if [ "$now" = "$last" ]; then held=$((held + 1)); else held=0; fi ;;
      *) held=0 ;;
    esac
    last=$now
    [ "$held" -ge "$need" ] && return 0
    sleep 60
  done
  die "cluster A did not settle in two hours; see $PWD/hold-$label.log"
}

if stage 1 "the baseline"; then
  require_baseline
  hold baseline 2
  check base-instances 0 "the machines of cluster A" -- instances
  cp base-instances.out instances-0.txt
  check base-notifications 0 "the open notifications of cluster A" -- open_notifications
  check base-macro 0 "the project macro $macro does not exist" -- \
    bash -c "! \"$live/api.sh\" get /v1/projects/$P/macros | jq -e --arg n $macro '.macros[]? | select(.name == \$n)'"
fi

if stage 2 "a macro in an infrastructure layer"; then
  check macro-set 0 "the project macro $macro gets the value one" -- "$live/api.sh" macro-set "$macro" one
  check macro-edit 0 "the Kubernetes pack of the release version reads it in postKubeadmCommands" -- \
    "$live/api.sh" edit "$R" "$kubeadm_from" "$edit_to"
  check edit-notification 0 "Palette opens a notification for the edit of the profile" -- \
    bash -c "sleep 15; \"$live/api.sh\" get /v1/notifications/spectrocluster/$C | jq -e '[.items[]? | select(.action.isDone == false)] | length > 0'"
  check macro-accept 0 "cluster A takes the update" -- "$live/api.sh" apply "$C"
  hold accepted "$stable"
  check accepted-instances 0 "the machines after the update" -- instances
  check accepted-changed ? "the update replaced the nodes (0), or kept them (1)" -- \
    bash -c '! cmp -s instances-0.txt accepted-instances.out'
  check accepted-resolved 0 "resolvedValues gives the macro the value one" -- test "$(resolved)" = one
  check accepted-record 0 "record cluster A with the digests of its macro values" -- \
    "$strata" snapshot "cluster:$C" --macros -o cluster-1.json
  check accepted-record-plain 0 "record cluster A without them" -- "$strata" snapshot "cluster:$C" -o cluster-plain.json
  check accepted-names 0 "the record names the macro" -- jq -e --arg m "$macro" '.source.cluster.macroValues[$m] | startswith("sha256:")' cluster-1.json
fi

if stage 3 "only the macro gets another value"; then
  cp accepted-instances.out instances-1.txt 2> /dev/null || instances > instances-1.txt
  date -u +%Y-%m-%dT%H:%M:%SZ > changed-at.txt
  check change 0 "the project macro $macro gets the value two, and nothing else changes" -- \
    "$live/api.sh" macro-set "$macro" two
  for _ in $(seq 1 "$watch_minutes"); do
    echo "$(date -u +%H:%M:%S) state=$(state) instances=$(instances) resolved=$(resolved) notifications=$(open_notifications)" >> watch.log
    sleep 60
  done
  instances > instances-2.txt
fi

if stage 4 "what each record says"; then
  since=$(cat changed-at.txt)
  check replaced ? "the macro change alone replaced the nodes (0), or did not (1)" -- \
    bash -c '! cmp -s instances-1.txt instances-2.txt'
  check resolved-two 0 "resolvedValues gives the value two" -- test "$(resolved)" = two
  check notifications 0 "the open notifications of cluster A after the change" -- open_notifications
  check notifications-none ? "Palette opened a notification for the macro change (0), or none (1)" -- \
    bash -c "jq -e 'length > 0' notifications.out"
  check audits 0 "the audit entries of any resource since the change" -- audits_since "$since"
  check audits-macro ? "an audit entry names a macro (0), or none does (1)" -- \
    bash -c "grep -qiE 'macro' audits.out"
  check audits-cluster ? "an audit entry from a person names cluster A after the change (0), or none does (1)" -- \
    bash -c "awk -F'\t' -v c=$CN '\$2 == \"user\" && \$6 == c' audits.out | grep -q ."
  check verify-plain ? "a cluster record without the digests: unchanged (0), or drifted (1)" -- "$strata" verify cluster-plain.json
  check verify-macros ? "a cluster record with the digests: unchanged (0), or drifted (1)" -- "$strata" verify cluster-1.json
  check verify-macros-says ? "the verdict names the macro" -- grep -q "macro $macro has another value now" verify-macros.out
fi

if stage 5 "undo"; then
  check undo-edit 0 "the release version back" -- "$live/api.sh" edit "$R" "$edit_to" "$kubeadm_from"
  check undo-accept 2 "Palette refuses the update of cluster A while the repave waits for approval" -- \
    "$live/api.sh" apply "$C"
  check undo-accept-says 0 "and says why" -- grep -q "repave is pending for approval" undo-accept.err
  check undo-macro 2 "Palette refuses to delete the macro that cluster A reads" -- "$live/api.sh" macro-delete "$macro"
  check repave-record 0 "#107: record cluster A while the repave waits" -- \
    "$strata" snapshot "cluster:$C" -o cluster-repave.json
  check repave-names 0 "#107: the record says that a repave waits, with the pack and the code that Palette gives" -- \
    jq -e '.source.cluster.repavePending == true
      and any(.source.cluster.pendingRepave.reasons[]; .pack == "kubernetes" and .code == "PackValuesUpdated")' \
    cluster-repave.json
  check repave-verify 0 "#107: verify of the record without the digests" -- "$strata" verify cluster-plain.json
  check repave-verify-says 0 "#107: and the verdict names the repave that waits" -- \
    grep -q "a repave of cluster $CN waits for approval" repave-verify.out
  echo "cluster A holds a repave that waits for approval: just live-down removes the example, then" \
    "api.sh macro-delete $macro" | tee -a steps.log
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

#!/usr/bin/env bash
# The registry move of a running cluster (#135): the OS pack of the release version moves to
# another registry, at the same version and with the same values. What do the impact table and
# the repave check of Palette say, and does cluster A replace its nodes when it takes the change?
#
#   examples/live/registry-move-nodes.sh [FROM_STAGE]
#
#   1  at the baseline: a record of the release version, and the machines of cluster A
#   2  the OS pack of the release version comes from move_registry_name, through OpenTofu, in
#      place, and the record shows the move. compare --repave of the copy of cluster A with the
#      version gives the level of the impact table and the answer of Palette
#   3  cluster A takes the update. For WATCH_MINUTES (default 30), each minute: the state, the
#      machines, and whether a repave waits for approval. Then: the nodes are replaced, or not
#   4  undo: the release version back, and cluster A takes it. When a repave waits for approval,
#      Palette refuses the update: the script stops there, and live-down removes the example
#
# It measures: the replacement of the nodes, or none, is the result. Needs the live example at its
# baseline with cluster A (just live-up), the strata release binary, jq, and the key as verify.sh
# needs it. Billable: cluster A can replace its nodes twice, about 20 minutes each. Results go
# under runs/<UTC time>/registry-move-nodes/, or OUT; FROM_STAGE resumes a run in OUT. Each check
# compares an exit code with the expected one; `?` records it without judging. Exit 0 only when
# no check fails.
set -uo pipefail
script=registry-move-nodes
flag=false
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"

watch_minutes=${WATCH_MINUTES:-30}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/registry-move-nodes}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  write_summary "registry move of a running cluster" ", cluster \`${C:-}\`," \
    "release \`${R:-}\`."
}
trap summary EXIT

load_outputs
C=$(out cluster_uid) R=$(out release_profile_uid)
release=$(out profile_name)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)

state() { "$live/tenant.sh" cluster "$C" 2>&1 | sed 's/^[^:]*: //'; }
# repave_waits: a record of cluster A says that a repave waits for approval
repave_waits() { "$strata" snapshot "cluster:$C" | jq -e '.source.cluster.repavePending == true' > /dev/null; }

if stage 1 "the baseline"; then
  require_baseline
  check base-instances 0 "the machines of cluster A" -- instances
  check base-record 0 "a record of the release version" -- "$strata" snapshot "$R" -o release-0.json
fi

if stage 2 "the OS pack of the release version moves to another registry"; then
  apply -var move_os_registry=true
  check move-record 0 "#135: a record of the release version after the move" -- "$strata" snapshot "$R" -o release-moved.json
  check move-same-uid 0 "#135: the move is in place: the uid of the release version stays" -- \
    test "$(jq -r .source.uid release-moved.json)" = "$R"
  check move-identity 0 "#135: the OS pack has another registry and the type oci, and the same version and values" -- \
    jq -en --slurpfile a release-0.json --slurpfile b release-moved.json '
      [$a[0], $b[0]] | map(.packs[] | select(.layer == "os")) as [$x, $y]
      | $x.version == $y.version and $x.values == $y.values
        and $x.registryUid != $y.registryUid and $y.type == "oci"'
  check move-compare ? "#135: compare --repave of the copy of cluster A with the moved version" -- \
    "$strata" compare "cluster:$C/$release" "$R" --repave --json
  check move-impact ? "#135: the impact table says repave for the OS pack (0), or another level (1)" -- \
    jq -e '.packs[] | select(.key | startswith("os/")) | .impact.level == "repave"' move-compare.out
  check move-palette ? "#135: Palette says repave (0), or no repave (1)" -- jq -e '.paletteRepave.required' move-compare.out
  check move-reasons ? "#135: the reasons of Palette" -- jq -ec '.paletteRepave.reasons // []' move-compare.out
fi

if stage 3 "cluster A takes the update"; then
  cp base-instances.out instances-0.txt 2> /dev/null || instances > instances-0.txt
  check take 0 "#135: cluster A takes the update of the release version" -- "$live/api.sh" apply "$C"
  for _ in $(seq 1 "$watch_minutes"); do
    waits=no
    repave_waits && waits=yes
    echo "$(date -u +%H:%M:%S) state=$(state) instances=$(instances) repave-waits=$waits" >> watch.log
    sleep 60
  done
  instances > instances-1.txt
  check replaced ? "#135: the move alone replaced the nodes of cluster A (0), or did not (1)" -- \
    bash -c '! cmp -s instances-0.txt instances-1.txt'
  check waits ? "#135: a repave waits for approval (0), or does not (1)" -- repave_waits
  check after-record 0 "#135: a record of cluster A after the update" -- "$strata" snapshot "cluster:$C" -o cluster-moved.json
  check after-runs 0 "#135: cluster A runs the OS pack from the other registry" -- \
    jq -en --slurpfile b release-moved.json --slurpfile c cluster-moved.json '
      [$b[0], $c[0]] | map(.packs[] | select(.layer == "os") | .registryUid) | .[0] == .[1]'
fi

if stage 4 "undo"; then
  if repave_waits; then
    echo "a repave of cluster A waits for approval, and Palette refuses an update until a person approves it:" \
      "just live-down removes the example" | tee -a steps.log
  else
    apply
    check undo-take 0 "cluster A takes the release version back" -- "$live/api.sh" apply "$C"
    wait_running "$C"
    for _ in $(seq 1 "$watch_minutes"); do
      "$strata" compare "$R" "cluster:$C/$release" > /dev/null 2>&1 && break
      sleep 60
    done
    check undo-baseline 0 "the example is at its baseline" -- "$live/baseline.sh"
  fi
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

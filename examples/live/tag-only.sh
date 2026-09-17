#!/usr/bin/env bash
# The tag-only test (#19): what a release version that differs only in the tag name its OS pack is
# pinned under does to strata's hashes and to a cluster that moves to it.
#
#   AWS_PROFILE=<profile> examples/live/tag-only.sh [FROM_STAGE]
#
#   1  record the release (1.0.0, OS pack tag 24.04), cluster A's release selection, and cluster
#      A's EC2 instances
#   2  publish 1.0.2 with the same OS pack under the exact tag LTS__24.04, and 1.0.3 under the
#      floating tag LTS__24.x; record both and compare them with 1.0.0
#   3  move cluster A to 1.0.2 and watch until it runs 1.0.2 and has stayed Running for 15
#      minutes: were its instances replaced (a repave) or kept?
#   4  move cluster A back to 1.0.0 and watch the same way
#   5  rename 1.0.2's OS pack tag in place through the API, and verify the record taken before
#   6  delete 1.0.2 and 1.0.3, and check the baseline
#
# What varies: the tag name the OS pack is pinned under, and which of the two versions cluster A
# runs. What holds: the pack uid, every other pack, the values, the flag (off), and the baseline
# the run starts from and returns to (lib.sh). Every answer is recorded (`?`) until #19 is decided.
#
# Needs the live example at its baseline (baseline.sh), the strata release binary, jq, and an AWS
# profile that can describe EC2 instances. Billable: a repave replaces cluster A's instances, and
# the watch waits up to an hour per move. Results go under runs/<UTC time>/tag-only/, or OUT;
# FROM_STAGE resumes a run in OUT. Exit 0 only when no check fails.
set -uo pipefail
script=tag-only
flag=false
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"
[ -n "${AWS_PROFILE:-}" ] || die "set AWS_PROFILE to a profile that can describe EC2 instances"

exact=LTS__24.04
floating=LTS__24.x
renamed=24.04
versions="{\"1.0.2\"=\"$exact\",\"1.0.3\"=\"$floating\"}"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/tag-only}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  {
    echo "# tag-only test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`. Cluster A \`${C:-}\`, release 1.0.0 \`${P:-}\`,"
    echo "1.0.2 (\`$exact\`) \`${EXACT:-}\`, 1.0.3 (\`$floating\`) \`${FLOATING:-}\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

save() { echo "$1=$2" >> ids.env; }
[ -f ids.env ] && . ./ids.env
stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

load_outputs
if [ "$from" -le 1 ]; then
  require_baseline
  : > ids.env
  PALETTE_PROJECT_UID=$(out project_uid)
  C=$(out cluster_uid)
  CN=$(out cluster_names | jq -r .a)
  N=$(out profile_name)
  P=$(out release_profile_uid)
  for v in PALETTE_PROJECT_UID C CN N P; do
    [ -n "${!v}" ] || die "the terraform output behind $v is empty"
    save "$v" "${!v}"
  done
fi
export PALETTE_PROJECT_UID

# instances: cluster A's EC2 instances that are not terminated, sorted
instances() {
  aws ec2 describe-instances --region "$(out region)" \
    --filters "Name=tag-key,Values=sigs.k8s.io/cluster-api-provider-aws/cluster/$CN" \
    "Name=instance-state-name,Values=pending,running,stopping,stopped,shutting-down" \
    --query 'Reservations[].Instances[].InstanceId' --output text | tr '\t' '\n' | sort | paste -sd ' ' -
}

# watch LABEL RECORD: every minute, log cluster A's state (with its conditions and latest events
# while it is not Running), whether its release selection equals RECORD, and its instances to
# watch-LABEL.log, until the selection equals RECORD and the cluster has stayed Running for 15
# minutes. Stops the script after an hour.
watch() {
  local label=$1 record=$2 held=0 state same
  for _ in $(seq 1 60); do
    state=$("$live/tenant.sh" cluster "$C" 2>&1)
    if "$strata" compare "$record" "cluster:$C/$N" >/dev/null 2>&1; then same=yes; else same=no; fi
    {
      echo "$(date -u +%H:%M:%S) runs=$same instances=$(instances)"
      sed 's/^/    /' <<<"$state"
    } >> "watch-$label.log"
    state=$(head -1 <<<"$state")
    case "$same:$state" in
      yes:*": Running") held=$((held + 1)); [ "$held" -ge 15 ] && return 0 ;;
      *) held=0 ;;
    esac
    sleep 60
  done
  "$live/diagnose.sh" "$PWD/diagnostics" >&2
  die "cluster A did not run $record and stay Running within an hour; see watch-$label.log"
}

if stage 1 "record the baseline"; then
  check release1 0 "record 1.0.0, the OS pack under tag 24.04" -- "$strata" snapshot "$P" --raw-dir raw/release1 -o release1.json
  check a1 0 "record cluster A's release selection" -- "$strata" snapshot "cluster:$C/$N" --raw-dir raw/a1 -o a1.json
  check instances1 0 "cluster A's EC2 instances before any move" -- instances
fi

if stage 2 "publish 1.0.2 and 1.0.3, which differ only in the OS pack's tag"; then
  apply_profile spectrocloud_cluster_profile.tagged -var "tag_only_versions=$versions"
  EXACT=$(out tagged_profile_uids | jq -r '."1.0.2"')
  FLOATING=$(out tagged_profile_uids | jq -r '."1.0.3"')
  save EXACT "$EXACT"
  save FLOATING "$FLOATING"
  check exact1 0 "record 1.0.2" -- "$strata" snapshot "$EXACT" --raw-dir raw/exact1 -o exact1.json
  check floating1 0 "record 1.0.3" -- "$strata" snapshot "$FLOATING" --raw-dir raw/floating1 -o floating1.json
  check os-packs ? "#19: the OS pack's tag, version, and pack uid as Palette stored each version" -- \
    jq -r '.source.version as $v | .packs[] | select(.layer == "os") | "\($v) tag=\(.tag) version=\(.version) packUid=\(.packUid)"' \
      release1.json exact1.json floating1.json
  check compare-exact ? "#19: 1.0.0 against 1.0.2, the exact tag" -- "$strata" compare release1.json exact1.json
  check compare-exact-json ? "the same comparison as JSON" -- "$strata" compare release1.json exact1.json --json
  check compare-floating ? "#19: 1.0.0 against 1.0.3, the floating tag" -- "$strata" compare release1.json floating1.json
  check same-release-exact ? "#19: whether 1.0.0 and 1.0.2 are the same release" -- "$strata" same-release release1.json exact1.json
fi

if stage 3 "move cluster A to 1.0.2"; then
  apply_clusters -var "tag_only_versions=$versions" -var "cluster_release_profile_uid=$EXACT"
  watch to-exact exact1.json
  check a2 0 "record cluster A's release selection" -- "$strata" snapshot "cluster:$C/$N" --raw-dir raw/a2 -o a2.json
  check a2-runs-exact ? "#19: cluster A's selection against 1.0.2" -- "$strata" compare exact1.json a2.json
  check instances2 0 "cluster A's EC2 instances after the move" -- instances
  check repaved-to-exact ? "#19: the move to 1.0.2 replaced cluster A's instances (0) or kept them (1)" -- \
    bash -c '! cmp -s instances1.out instances2.out'
fi

if stage 4 "move cluster A back to 1.0.0"; then
  apply_clusters -var "tag_only_versions=$versions"
  watch to-release release1.json
  check a3 0 "record cluster A's release selection" -- "$strata" snapshot "cluster:$C/$N" --raw-dir raw/a3 -o a3.json
  check instances3 0 "cluster A's EC2 instances after the move back" -- instances
  check repaved-back ? "#19: the move back to 1.0.0 replaced cluster A's instances (0) or kept them (1)" -- \
    bash -c '! cmp -s instances2.out instances3.out'
fi

if stage 5 "rename 1.0.2's OS pack tag in place"; then
  check retag 0 "#19: rename 1.0.2's OS pack tag through the API, keeping its pack uid" -- \
    "$live/api.sh" retag "$EXACT" ubuntu-aws "$exact" "$renamed"
  check exact2 0 "record the renamed 1.0.2" -- "$strata" snapshot "$EXACT" --raw-dir raw/exact2 -o exact2.json
  check retag-kept-pack-uid 0 "the rename kept the pack uid" -- \
    jq -e -n --slurpfile a exact1.json --slurpfile b exact2.json \
      '[$a[0].packs[] | select(.layer == "os") | .packUid] == [$b[0].packs[] | select(.layer == "os") | .packUid]'
  check verify-exact1 ? "#19: verify of 1.0.2's record from before the rename" -- "$strata" verify exact1.json
fi

if stage 6 "delete 1.0.2 and 1.0.3, and check the baseline"; then
  apply_profile spectrocloud_cluster_profile.tagged
  check baseline-after 0 "the live example is back at its baseline" -- "$live/baseline.sh"
fi

echo
echo "$failures failed; details in $run"
[ "$failures" -eq 0 ]

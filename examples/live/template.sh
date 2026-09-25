#!/usr/bin/env bash
# The cluster template test (#66): Palette locks a profile version that a cluster template links.
# The script links the release version and the add-on version before any cluster runs them, tries
# an edit in place of a linked version, and then builds a cluster outside the template with the
# linked versions, to record what Palette answers. With the cluster Running, the start time of
# each add-on pack gives the order in which the cluster installs the packs of two add-on profiles.
#
#   just live-profiles && just live-template
#
#   1  a maintenance policy and a template link the two versions
#   2  an edit in place of the linked release version is refused, and the version verifies
#   3  a cluster outside the template, with a linked version, is refused
#   4  the template is removed, and the cluster is built with the same versions and a second
#      add-on profile: the edit in place is accepted again, and the start time of each add-on pack
#      gives the order in which the cluster installs the packs of two add-on profiles
#   5  the example is removed: the cluster and every profile
#
# Needs tofu, jq, and no cluster: the link must come before a cluster runs the versions. The script
# ends with the removal of the whole example, as live-down does, because step 4 edits a version
# that the cluster runs. Results go under runs/<UTC time>/template/, or OUT. Exit 0 only when no
# check fails.
set -uo pipefail
script=template
flag=false
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata; run just build"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/template}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
[ -z "$(out cluster_uid)" ] || die "a cluster exists; the link must come first, so run this after just live-profiles"
P=$(out release_profile_uid)
AD=$(out addon_profile_uid)
N=$(out profile_name)
names=$(out profile_names | jq -r '.[]')
PALETTE_PROJECT_UID=$(out project_uid)
export PALETTE_PROJECT_UID
for v in P AD N PALETTE_PROJECT_UID; do
  [ -n "${!v}" ] || die "the tofu output behind $v is empty; run just live-profiles"
done

summary() {
  {
    echo "# cluster template test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, release \`$N\` 1.0.0 (\`$P\`)."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

# ---- 1. the template links the versions ----------------------------------------------------------

echo "== 1. the template links the versions" | tee -a steps.log
check release-before 0 "a record of the release version before the link" -- "$strata" snapshot "$P" -o release.json
apply -var cluster=false -var template=true
T=$(out template_uid)
[ -n "$T" ] || die "no uid of the template after the apply"
check template 0 "#66: the template" -- "$live/api.sh" get "/v1/clusterTemplates/$T"
check template-links 0 "#66: it links the release version and the add-on version" -- \
  jq -e --arg p "$P" --arg a "$AD" '[.spec.profiles[].uid] | (index($p) != null) and (index($a) != null)' template.out
check release-doc 0 "the release version, as Palette holds it" -- "$live/api.sh" get "/v1/clusterprofiles/$P"
check release-names-template 0 "#66: the version names the template that uses it, and no cluster" -- \
  jq -e --arg t "$T" '([.status.inUseClusterTemplates[]?.uid] | index($t) != null) and ((.status.inUseClusters // []) | length == 0)' release-doc.out

# ---- 2. an edit in place of a linked version ----------------------------------------------------

echo "== 2. an edit in place of the linked release version" | tee -a steps.log
check lock-edit 2 "#66: an edit in place of a version that a template links is refused" -- \
  "$live/api.sh" edit "$P" "$(out csi_reclaim_delete)" "$(out csi_reclaim_delete | sed 's/Delete/Retain/')"
check lock-edit-says 0 "the refusal names the reason" -- \
  grep -q ClusterProfileUpdateValidationFailure lock-edit.err
check lock-verify 0 "#66: the release version is unchanged" -- "$strata" verify release.json

# ---- 3. a cluster outside the template -----------------------------------------------------------

echo "== 3. a cluster outside the template, with the linked versions" | tee -a steps.log
check cluster-outside-template 1 "#66: Palette refuses a cluster outside the template that uses a linked version" -- \
  "$live/tf.sh" apply -input=false -auto-approve -no-color -var template=true -var second_addon=true
check refusal-says 0 "#66: the refusal names the template" -- \
  grep -q 'ClusterCreateWithTemplateProfileForbidden' cluster-outside-template.err

# ---- 4. the template is removed, and the cluster is built --------------------------------------

echo "== 4. the template is removed, and the cluster runs the versions with a second add-on profile" | tee -a steps.log
apply -var second_addon=true
C=$(out cluster_uid)
A2=$(out addon2_profile_uid)
[ -n "$C" ] || die "no cluster after the apply"
wait_running "$C"
check cluster-runs-release 0 "the cluster runs the release version: IDENTICAL" -- \
  "$strata" compare release.json "cluster:$C/$N"
check cluster-doc 0 "the cluster document" -- "$live/api.sh" get "/v1/spectroclusters/$C"
check cluster-three-profiles 0 "the cluster runs the release version and the two add-on profiles" -- \
  jq -e --arg a "$AD" --arg b "$A2" '[.spec.clusterProfileTemplates[].uid] | (index($a) != null) and (index($b) != null)' cluster-doc.out
check lock-gone 0 "#66: with the template gone, the edit in place of the release version is accepted again" -- \
  "$live/api.sh" edit "$P" "$(out csi_reclaim_delete)" "$(out csi_reclaim_delete | sed 's/Delete/Retain/')"
check lock-gone-verify 1 "#66: the record from before reports DRIFT: the lock was the template's, not the version's" -- \
  "$strata" verify release.json
# The order the cluster lists the add-on profiles, and when the pack of each one started. The
# two packs start together, to the second, so the listed order is no install order.
listed() { jq -r '.spec.clusterProfileTemplates[] | select(.type == "add-on") | .uid' cluster-doc.out | tr '\n' ' '; }
starts() {
  jq -r '.status.packs | to_entries[] | .value | select(.startTime != null) | "\(.startTime) \(.profileUid) \(.name)"' cluster-doc.out \
    | sort | awk -v a="$AD" -v b="$A2" '$2 == a || $2 == b { if (!seen[$2]++) print }'
}
# start_gap: the seconds between the first start of a pack of each add-on profile
start_gap() {
  local first last
  first=$(starts | head -1 | cut -d' ' -f1)
  last=$(starts | tail -1 | cut -d' ' -f1)
  echo $(( $(date -u -d "$last" +%s) - $(date -u -d "$first" +%s) ))
}
install_order() {
  echo "listed:  $(listed)"
  echo "started:"
  starts
  echo "gap: $(start_gap)s"
  [ "$(start_gap)" -le 30 ]
}
check addon-order 0 "#66: the packs of two add-on profiles start within thirty seconds of each other, whatever order the cluster lists the profiles in" -- install_order

# ---- 5. the example is removed -------------------------------------------------------------------

echo "== 5. the example is removed" | tee -a steps.log
"$live/tf.sh" destroy -input=false -auto-approve -no-color > destroy.txt 2>&1 \
  || "$live/tf.sh" destroy -input=false -auto-approve -no-color >> destroy.txt 2>&1 \
  || { "$live/diagnose.sh" "$PWD/diagnostics" >&2; die "tofu destroy failed; see $PWD/destroy.txt"; }
command grep -E '^(Destroy complete|Error)' destroy.txt | tee -a steps.log
# shellcheck disable=SC2086
"$live/api.sh" delete-named $names | tee -a steps.log

echo
echo "$script: $(command grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

#!/usr/bin/env bash
# The registry test (#66): the example's own OCI pack registry, which no other user reads. The
# script pushes a pack into it, pins the pack in a profile, pushes the same version again with
# other bytes, and pushes a new version. After each push it records what Palette does: the pack
# listing, a record of the pack, and the published profile that pins it.
#
#   just live-profiles && just live-registry
#
#   1  push 1.0.0 and sync: the digest Palette lists is the digest of the push, and a record of
#      the pack carries it
#   2  a profile pins the version: its pack reference carries that digest
#   3  push 1.0.0 again with other bytes, and sync: the pack listing and the record of the pack;
#      the published profile with the values of the pack follows the push some minutes later,
#      and a profile with values of its own keeps its digest (#81)
#   4  push 1.0.1 and sync: the profile pinned to 1.0.0 stays; and the floating tag, when the
#      registry lists one, and who the audit log of Palette says moved the profile (#69)
#   5  a chart in the example's own OCI Helm registry: the digest Palette lists for it, and what a
#      push again does to the listing and to a profile that pins it
#   6  a pack that requires another pack: whether a profile that holds the one and not the other
#      is refused
#
# Needs tofu, aws with the profile terraform.tfvars names, oras, jq, and tar. The registry exists
# after any apply. The script leaves the example at its baseline: the profile of step 2 is gone,
# and the registry keeps the pushed versions until live-down. Results go under
# runs/<UTC time>/registry/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=registry
flag=false
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata; run just build"
for tool in aws oras jq tar; do
  command -v "$tool" > /dev/null || die "$tool is not installed"
done

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/registry}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
REG=$(out owned_registry_uid)
REG_NAME=$(out owned_registry_name)
HOST=$(out owned_registry_host)
REPO=$(out owned_repository)
PACK=$(out owned_pack_name)
REGION=$(out region)
AWS_PROFILE=$(out aws_profile)
PALETTE_PROJECT_UID=$(out project_uid)
export AWS_PROFILE PALETTE_PROJECT_UID
for v in REG REG_NAME HOST REPO PACK REGION AWS_PROFILE PALETTE_PROJECT_UID; do
  [ -n "${!v}" ] || die "the tofu output behind $v is empty; run just live-profiles"
done
# The script needs no cluster, so its applies keep the cluster as they find it: none after
# live-profiles, and cluster A after live-up. The default of the variable is a cluster.
cluster_var=()
[ -n "$(out cluster_uid)" ] || cluster_var=(-var cluster=false)

summary() {
  {
    echo "# registry test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, registry \`$REG_NAME\`,"
    echo "pack \`$PACK\`."
    echo
    summary_table
  } > summary.md
}
trap '[ -z "${M:-}" ] || "$live/api.sh" delete "$M" > /dev/null 2>&1; summary' EXIT

# ---- the pack, the push, the sync ----------------------------------------------------------------

# build VERSION [LINE]: PACK-VERSION.tar.gz from a copy of the pack source, with pack.json at
# VERSION and LINE added to values.yaml, so the bytes differ from the build before
build() {
  local version=$1 line=${2:-}
  local src="$live/pack/$PACK" dir="pack-$version"
  rm -rf "$dir" && mkdir -p "$dir" && cp -r "$src" "$dir/$PACK" || return 2
  jq --arg v "$version" '.version = $v' "$src/pack.json" > "$dir/$PACK/pack.json" || return 2
  [ -z "$line" ] || printf '%s\n' "$line" >> "$dir/$PACK/values.yaml"
  tar -czf "$PACK-$version.tar.gz" -C "$dir" "$PACK"
}
# push VERSION LABEL: push the archive under the tag VERSION, as the Palette documentation shows,
# and print the digest of the push. The output of oras goes to oras-LABEL.out.
push() {
  oras push "$REPO:$1" "$PACK-$1.tar.gz" > "oras-$2.out" 2>&1 || { cat "oras-$2.out" >&2; return 2; }
  grep -o 'sha256:[0-9a-f]\{64\}' "oras-$2.out" | tail -1
}
digest_of() { jq -r ".$2" "$1"; }
# wait_drift RECORD: verify the record again until it reports DRIFT, for up to ten minutes.
# Palette moves a published profile some minutes after the push, not within the sync that the
# script asks for, so a read right after that sync is too early. Exit 1 on DRIFT, as verify
# does; 0 when the record still verifies.
wait_drift() {
  local i
  for i in $(seq 1 60); do
    if ! "$strata" verify "$1"; then
      return 1
    fi
    sleep 10
  done
  "$strata" verify "$1"
}

aws ecr get-login-password --region "$REGION" | oras login --username AWS --password-stdin "$HOST" > login.out 2>&1 \
  || die "oras login to $HOST failed: $(cat login.out)"

# ---- 0. an empty registry ------------------------------------------------------------------------
# A run before this one can have left the profile of step 2 and the pushed versions behind. The
# baseline apply removes the profile; the repository is emptied; a sync makes Palette forget.

echo "== 0. an empty registry" | tee -a steps.log
tf_apply false "${cluster_var[@]}"
repository=${REPO#*/}
images=$(aws ecr list-images --region "$REGION" --repository-name "$repository" --query 'imageIds[*]' --output json) \
  || die "listing the images of $repository failed"
if [ "$(jq length <<<"$images")" != 0 ]; then
  echo "$images" > images-before.json
  check empty 0 "the repository starts empty: the images of the run before are removed" -- \
    aws ecr batch-delete-image --region "$REGION" --repository-name "$repository" --image-ids file://images-before.json
  check sync-0 0 "Palette reads $REG_NAME again, and forgets them" -- "$live/api.sh" sync-oci "$REG"
fi

# ---- 1. push, sync, read -------------------------------------------------------------------------

echo "== 1. push $PACK 1.0.0, and read it" | tee -a steps.log
build 1.0.0 || die "building $PACK 1.0.0 failed"
check push-1 0 "push $PACK 1.0.0 into $REG_NAME" -- push 1.0.0 first
A=$(cat push-1.out)
[ -n "$A" ] || die "no digest in the output of the first push"
check sync-1 0 "Palette reads $REG_NAME again" -- "$live/api.sh" sync-oci "$REG"
check packs 0 "#66: packs lists $PACK in $REG_NAME" -- "$strata" packs "$PACK" --json
check packs-version 0 "#66: and the one version is 1.0.0" -- \
  jq -e --arg r "$REG_NAME" '[.registries[] | select(.registry == $r) | .versions[].version] == ["1.0.0"]' packs.out
check leaf 0 "#66: a record of pack:$REG_NAME/$PACK@1.0.0" -- \
  "$strata" snapshot "pack:$REG_NAME/$PACK@1.0.0" -o leaf-1.json
check leaf-digest-is-push 0 "#66: the digest Palette lists for the version is the digest of the push" -- \
  test "$(digest_of leaf-1.json 'packs[0].hash.upstreamRaw')" = "$A"

# ---- 2. a profile pins it ------------------------------------------------------------------------

echo "== 2. a profile pins $PACK 1.0.0" | tee -a steps.log
apply "${cluster_var[@]}" -var owned_pack_profile=true
P=$(out owned_profile_uid)
[ -n "$P" ] || die "no uid of the profile that pins the pack after the apply"
check profile 0 "#66: a record of the profile that pins $PACK 1.0.0" -- "$strata" snapshot "$P" -o profile-1.json
check profile-digest-is-push 0 "#66: its pack reference carries the digest of the push" -- \
  test "$(digest_of profile-1.json 'packs[0].hash.upstreamRaw')" = "$A"
# a second profile pins the same version with values that differ from the defaults of the pack
cp "$live/pack/$PACK/values.yaml" modified-values.yaml && printf 'strataModified: true\n' >> modified-values.yaml
check pin-modified 0 "#81: a profile that pins $PACK 1.0.0 with values changed from the defaults, through the API" -- \
  "$live/api.sh" pin-profile strata-live-pinned-modified "$REG" oci "$PACK" 1.0.0 modified-values.yaml
M=$(cat pin-modified.out)
check profile-mod 0 "#81: a record of that profile" -- "$strata" snapshot "$M" -o profile-mod-1.json

# ---- 3. the same version again -------------------------------------------------------------------

echo "== 3. push $PACK 1.0.0 again, with other bytes" | tee -a steps.log
build 1.0.0 "strata: pushed again" || die "building $PACK 1.0.0 again failed"
check push-2 0 "push $PACK 1.0.0 again, with other bytes" -- push 1.0.0 again
B=$(cat push-2.out)
check push-2-differs 0 "the two pushes have two digests" -- test "$A" != "$B"
check sync-2 0 "Palette reads $REG_NAME again" -- "$live/api.sh" sync-oci "$REG"
check leaf-after 0 "#66: a record of the pack version after the push" -- \
  "$strata" snapshot "pack:$REG_NAME/$PACK@1.0.0" -o leaf-2.json
check listing-follows 0 "#66: Palette lists the digest of the new push for the version, under the same version" -- \
  test "$(digest_of leaf-2.json 'packs[0].hash.upstreamRaw')" = "$B"
check leaf-verify 1 "#66: the record of the pack from before the push reports DRIFT" -- \
  "$strata" verify leaf-1.json
# Palette moves a published profile some minutes after the sync that reads the push, when it
# moves it at all: wait_drift gives it ten minutes
check profile-follows 1 "#81: the published profile with the default values follows the push: its record from before reports DRIFT" -- \
  wait_drift profile-1.json
check profile-now 0 "a record of that profile now" -- "$strata" snapshot "$P" -o profile-1b.json
check profile-now-digest 0 "#81: its pack reference carries the digest of the second push" -- \
  test "$(digest_of profile-1b.json 'packs[0].hash.upstreamRaw')" = "$B"
check profile-mod-keeps 0 "#81: the profile with changed values keeps the digest it pinned: its record verifies" -- \
  "$strata" verify profile-mod-1.json
check profile-who 1 "#81: who moved the first profile: the audit log of Palette" -- \
  "$strata" verify profile-1.json --who
check profile-who-system 0 "#81: the log names the move: an update by the Palette service system" -- \
  grep -Eq '^  [0-9TZ:.-]+  system \(service\)  update' profile-who.out
check delete-modified 0 "delete the profile with changed values" -- "$live/api.sh" delete "$M"

# ---- 4. a new version -----------------------------------------------------------------------------

echo "== 4. push $PACK 1.0.1" | tee -a steps.log
check profile-after 0 "#66: a record of the profile after the push" -- "$strata" snapshot "$P" -o profile-2.json
build 1.0.1 || die "building $PACK 1.0.1 failed"
check push-3 0 "push $PACK 1.0.1" -- push 1.0.1 new
check sync-3 0 "Palette reads $REG_NAME again" -- "$live/api.sh" sync-oci "$REG"
check packs-two 0 "#66: packs lists the two versions" -- "$strata" packs "$PACK" --json
check packs-two-order 0 "#66: 1.0.1 first, then 1.0.0" -- \
  jq -e --arg r "$REG_NAME" '[.registries[] | select(.registry == $r) | .versions[].version] == ["1.0.1", "1.0.0"]' packs-two.out
check profile-stays 0 "#66: the profile pinned to 1.0.0 does not move to 1.0.1" -- "$strata" verify profile-2.json
floating=$(jq -r --arg r "$REG_NAME" '[.registries[] | select(.registry == $r) | .versions[].tags[]] | map(select(test("x"))) | first // ""' packs-two.out 2>/dev/null)
if [ -n "$floating" ]; then
  echo "== 4b. the floating tag $floating" | tee -a steps.log
  apply "${cluster_var[@]}" -var owned_pack_profile=true -var "owned_pack_tag=$floating" -var owned_pack_version=1.0.1
  check profile-floating 0 "#66: a record of the profile pinned under the tag $floating" -- "$strata" snapshot "$P" -o profile-3.json
  build 1.0.2 || die "building $PACK 1.0.2 failed"
  check push-4 0 "push $PACK 1.0.2, which the tag $floating now names" -- push 1.0.2 floating
  check sync-4 0 "Palette reads $REG_NAME again" -- "$live/api.sh" sync-oci "$REG"
  check floating-moves 1 "#66: the profile pinned under the floating tag moves to 1.0.2, after the sync: its record reports DRIFT" -- \
    wait_drift profile-3.json
  check floating-who 1 "#69: verify --who on the same record reads the audit log of the profile" -- \
    "$strata" verify profile-3.json --who
  check floating-who-system 0 "#69: the log names the move: an update by the Palette service system" -- \
    grep -Eq '^  [0-9TZ:.-]+  system \(service\)  update' floating-who.out
  check floating-who-versions 0 "#69: its summary names the two versions" -- \
    grep -Eq '^  [0-9TZ:.-]+  system \(service\)  update: .*1\.0\.1 > 1\.0\.2' floating-who.out
else
  printf '%-4s  %-32s %s\n' SEEN floating "$REG_NAME lists no floating tag for $PACK, so the floating tag case does not apply to this registry kind" | tee -a steps.log
  printf '%s\t%s\n' floating "no floating tag in this registry kind; the case does not apply" >> checks.tsv
fi

# ---- 5. a Helm chart in an OCI registry ----------------------------------------------------------
# The same repository host, read by Palette as an OCI Helm registry. What Palette lists for a
# chart there, and whether that follows a push, is what no run covered. Palette keeps the digest
# it listed first for a chart version, so the Palette registry object is made again at the start,
# and Palette reads the chart for the first time in each run.

HELM_REG=$(out owned_helm_registry_uid)
HELM_REG_NAME=$(out owned_helm_registry_name)
CHART_REPO=$(out owned_chart_repository)
CHART=$(out owned_chart_name)
for v in HELM_REG HELM_REG_NAME CHART_REPO CHART; do
  [ -n "${!v}" ] || die "the tofu output behind $v is empty; run just live-profiles"
done
chart_repository=${CHART_REPO#*/}
chart_base=$(dirname "$chart_repository")

echo "== 5. a Helm chart in an OCI registry" | tee -a steps.log
aws ecr get-login-password --region "$REGION" | helm registry login --username AWS --password-stdin "$HOST" > helm-login.out 2>&1 \
  || die "helm registry login to $HOST failed: $(cat helm-login.out)"
images=$(aws ecr list-images --region "$REGION" --repository-name "$chart_repository" --query 'imageIds[*]' --output json) \
  || die "listing the images of $chart_repository failed"
if [ "$(jq length <<<"$images")" != 0 ]; then
  echo "$images" > chart-images-before.json
  check chart-empty 0 "the chart repository starts empty" -- \
    aws ecr batch-delete-image --region "$REGION" --repository-name "$chart_repository" --image-ids file://chart-images-before.json
fi
tf_apply false "${cluster_var[@]}" -var owned_helm_registry=false
tf_apply false "${cluster_var[@]}"
HELM_REG=$(out owned_helm_registry_uid)
[ -n "$HELM_REG" ] || die "no uid of the Helm registry after the apply that made it again"
# build_chart VERSION [LINE]: chart-VERSION/CHART-VERSION.tgz from a copy of the chart source, at
# VERSION, with LINE added to values.yaml so the bytes differ from the build before
build_chart() {
  local version=$1 line=${2:-}
  local src="$live/chart/$CHART" dir="chart-$version"
  rm -rf "$dir" && mkdir -p "$dir" && cp -r "$src" "$dir/$CHART" || return 2
  [ -z "$line" ] || printf '%s\n' "$line" >> "$dir/$CHART/values.yaml"
  helm package "$dir/$CHART" --version "$version" --destination "$dir" > /dev/null
}
# push_chart VERSION LABEL: helm push to the base path, as the Palette documentation shows, and
# print the manifest digest of the push. The digest of the archive itself is archive_digest.
push_chart() {
  helm push "chart-$1/$CHART-$1.tgz" "oci://$HOST/$chart_base" > "helm-$2.out" 2>&1 || { cat "helm-$2.out" >&2; return 2; }
  grep -o 'sha256:[0-9a-f]\{64\}' "helm-$2.out" | tail -1
}
archive_digest() { sha256sum "chart-$1/$CHART-$1.tgz" | cut -d' ' -f1; }
# which_digest FILE MANIFEST ARCHIVE: which digest a record carries for the chart: none, the
# digest of the archive, the manifest digest of the push, or another value
which_digest() {
  local d
  d=$(digest_of "$1" 'packs[0].hash.upstreamRaw')
  d=${d#sha256:}
  case "$d" in
    "") echo none ;;
    "${2#sha256:}") echo manifest ;;
    "$3") echo archive ;;
    *) echo "other $d" ;;
  esac
}
# digest_is FILE MANIFEST ARCHIVE WANT: print which digest the record carries, and exit 0 when it
# is WANT
digest_is() {
  local got
  got=$(which_digest "$1" "$2" "$3")
  echo "$got"
  [ "$got" = "$4" ]
}

build_chart 1.0.0 || die "building $CHART 1.0.0 failed"
check chart-push-1 0 "push $CHART 1.0.0 into $HELM_REG_NAME" -- push_chart 1.0.0 first
M1=$(cat chart-push-1.out)
H1=$(archive_digest 1.0.0)
check chart-sync-1 0 "Palette reads $HELM_REG_NAME again" -- "$live/api.sh" sync-oci "$HELM_REG"
check chart-packs 0 "#66: packs lists $CHART in $HELM_REG_NAME" -- "$strata" packs "$CHART" --json
check chart-leaf 0 "#66: a record of pack:$HELM_REG_NAME/$CHART@1.0.0" -- \
  "$strata" snapshot "pack:$HELM_REG_NAME/$CHART@1.0.0" -o chart-leaf-1.json
check chart-digest 0 "#66: Palette lists the manifest digest of the push for a chart in an OCI registry, as for a pack" -- \
  digest_is chart-leaf-1.json "$M1" "$H1" manifest
apply "${cluster_var[@]}" -var owned_chart_profile=true
HP=$(out owned_helm_profile_uid)
[ -n "$HP" ] || die "no uid of the profile that pins the chart after the apply"
check chart-profile 0 "#66: a record of the profile that pins $CHART 1.0.0" -- "$strata" snapshot "$HP" -o chart-profile-1.json
check chart-profile-digest 0 "#66: the profile's pack reference carries the manifest digest of the push" -- \
  digest_is chart-profile-1.json "$M1" "$H1" manifest
build_chart 1.0.0 "pushedAgain: yes" || die "building $CHART 1.0.0 again failed"
check chart-push-2 0 "push $CHART 1.0.0 again, with other bytes" -- push_chart 1.0.0 again
M2=$(cat chart-push-2.out)
H2=$(archive_digest 1.0.0)
check chart-push-2-differs 0 "the two pushes have two digests" -- test "$M1" != "$M2"
check chart-sync-2 0 "Palette reads $HELM_REG_NAME again" -- "$live/api.sh" sync-oci "$HELM_REG"
check chart-leaf-after 0 "#66: a record of the chart version after the push" -- \
  "$strata" snapshot "pack:$HELM_REG_NAME/$CHART@1.0.0" -o chart-leaf-2.json
# same_digest A B: the two records carry one digest for the chart
same_digest() { [ "$(digest_of "$1" 'packs[0].hash.upstreamRaw')" = "$(digest_of "$2" 'packs[0].hash.upstreamRaw')" ]; }
check chart-listing-keeps 0 "#66: Palette keeps the digest it listed first: the listing does not follow the push" -- \
  same_digest chart-leaf-1.json chart-leaf-2.json
check chart-listing-not-new 0 "the listing does not carry the manifest digest of the new push" -- \
  test "$(which_digest chart-leaf-2.json "$M2" "$H2")" != manifest
check chart-leaf-verify 0 "#66: the record of the chart from before the push verifies: Palette lists what it listed" -- \
  "$strata" verify chart-leaf-1.json
check chart-profile-verify 0 "#66: the published profile that pins the chart does not follow the push" -- \
  "$strata" verify chart-profile-1.json
build_chart 1.0.1 || die "building $CHART 1.0.1 failed"
check chart-push-3 0 "push $CHART 1.0.1" -- push_chart 1.0.1 third
check chart-sync-3 0 "Palette reads $HELM_REG_NAME again" -- "$live/api.sh" sync-oci "$HELM_REG"
check chart-packs-two 0 "packs lists the chart" -- "$strata" packs "$CHART" --json
check chart-packs-two-versions 0 "#66: the sync reads a new version: packs lists 1.0.1 first, then 1.0.0" -- \
  jq -e '[.registries[0].versions[].version] == ["1.0.1", "1.0.0"]' chart-packs-two.out

# ---- 6. a pack that requires another pack ------------------------------------------------------
# pack.json can name a pack that this pack requires (constraints.dependencies, type required).
# Whether Palette looks for that pack beyond the profile that holds the requiring pack is what no
# run covered: a profile with the requiring pack alone, then one with both.

DEP=$(out dep_pack_name)
DEP_REPO=$(out owned_dep_repository)
for v in DEP DEP_REPO; do
  [ -n "${!v}" ] || die "the tofu output behind $v is empty; run just live-profiles"
done

echo "== 6. $PACK 1.1.0 requires $DEP" | tee -a steps.log
# build_requiring VERSION: PACK-VERSION.tar.gz as build makes it, with a required dependency on
# DEP 1.0.0 or newer in pack.json
build_requiring() {
  local version=$1
  local src="$live/pack/$PACK" dir="pack-$version"
  rm -rf "$dir" && mkdir -p "$dir" && cp -r "$src" "$dir/$PACK" || return 2
  jq --arg v "$version" --arg d "$DEP" \
    '.version = $v | .constraints.dependencies = [{packName: $d, layer: "addon", minVersion: "1.0.0", maxVersion: "", type: "required"}]' \
    "$src/pack.json" > "$dir/$PACK/pack.json" || return 2
  tar -czf "$PACK-$version.tar.gz" -C "$dir" "$PACK"
}
# build_dep: DEP-1.0.0.tar.gz from the source of the second pack
build_dep() {
  local dir=dep-1.0.0
  rm -rf "$dir" && mkdir -p "$dir" && cp -r "$live/pack/$DEP" "$dir/$DEP" || return 2
  tar -czf "$DEP-1.0.0.tar.gz" -C "$dir" "$DEP"
}
# push_dep: push DEP 1.0.0 into its own repository, and print the digest of the push
push_dep() {
  oras push "$DEP_REPO:1.0.0" "$DEP-1.0.0.tar.gz" > oras-dep.out 2>&1 || { cat oras-dep.out >&2; return 2; }
  grep -o 'sha256:[0-9a-f]\{64\}' oras-dep.out | tail -1
}
# the pack names the newest refusal of tofu gives, from the output of try_apply
refusal_names() { command grep -q "$1" "$2"; }

build_dep || die "building $DEP 1.0.0 failed"
check dep-push 0 "push $DEP 1.0.0 into $REG_NAME" -- push_dep
build_requiring 1.1.0 || die "building $PACK 1.1.0 failed"
check dep-push-requiring 0 "push $PACK 1.1.0, which requires $DEP 1.0.0 or newer" -- push 1.1.0 requiring
check dep-sync 0 "Palette reads $REG_NAME again" -- "$live/api.sh" sync-oci "$REG"
check dep-packs 0 "packs lists $DEP in $REG_NAME" -- "$strata" packs "$DEP" --json
check dep-profile-alone 1 "#66: a profile with $PACK 1.1.0 and not $DEP is refused at creation" -- \
  try_apply "${cluster_var[@]}" -var dep_profile=alone
check dep-profile-alone-says 0 "the refusal names $DEP" -- refusal_names "$DEP" dep-profile-alone.out
check dep-profile-both 0 "#66: a profile with both packs is accepted" -- \
  try_apply "${cluster_var[@]}" -var dep_profile=both
DP=$(out dep_profile_uid)
[ -n "$DP" ] || die "no uid of the profile that holds both packs after the apply"
check dep-record 0 "a record of the profile that holds both" -- "$strata" snapshot "$DP" -o dep-profile.json
check dep-record-packs 0 "the record lists the two packs, the requiring one first, as authored" -- \
  jq -e --arg p "$PACK" --arg d "$DEP" '[.packs[].name] == [$p, $d]' dep-profile.json

# ---- back to the baseline -------------------------------------------------------------------------

echo "== back to the baseline" | tee -a steps.log
tf_apply false "${cluster_var[@]}"

echo
echo "$script: $(command grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

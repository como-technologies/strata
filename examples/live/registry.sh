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
#   3  push 1.0.0 again with other bytes, and sync: the pack listing, the record of the pack, and
#      the published profile, each against its record from before the push
#   4  push 1.0.1 and sync: the profile pinned to 1.0.0 stays; and the floating tag, when the
#      registry lists one
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
trap summary EXIT

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
check profile-verify 0 "#66: the published profile that pins the version does not follow the push: its record from before verifies" -- \
  "$strata" verify profile-1.json
check profile-keeps-pin 0 "#66: its pack reference still carries the digest of the first push" -- \
  test "$(digest_of profile-1.json 'packs[0].hash.upstreamRaw')" = "$A"

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
  check floating-moves 1 "#66: the profile pinned under the floating tag moves to 1.0.2: its record reports DRIFT" -- \
    "$strata" verify profile-3.json
else
  printf '%-4s  %-32s %s\n' SEEN floating "$REG_NAME lists no floating tag for $PACK, so the floating tag case does not apply to this registry kind" | tee -a steps.log
  printf '%s\t%s\n' floating "no floating tag in this registry kind; the case does not apply" >> checks.tsv
fi

# ---- back to the baseline -------------------------------------------------------------------------

echo "== back to the baseline" | tee -a steps.log
tf_apply false "${cluster_var[@]}"

echo
echo "$script: $(command grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

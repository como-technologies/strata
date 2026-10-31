#!/usr/bin/env bash
# The registry kinds test (#75): the digest that Palette gives a pack of each registry kind, and
# what a push again of the same version, with other bytes, does to it. The kinds that the example
# does not get from ECR come from the registry host of terraform/registry-kinds.tf.
#
#   examples/live/registry-kinds.sh
#
#   1  push version 1.0.0 into each kind, sync, and record the pack: the digest Palette lists, and
#      the provider type that a record of an OCI pack names
#   2  a Helm registry with credentials, which Palette does not synchronize: a candidate with a
#      chart from it, a profile that pins the chart, the pack reference with no digest, and what
#      verify and same-release say
#   3  push 1.0.0 again into each kind, with other bytes, and sync: the digest Palette lists now,
#      and the record from before, verified again
#
# The expected exit of each verify in stage 3 is the point of the check: the kinds differ.
#
#   kind                              the digest Palette lists        after a push again
#   Helm repository, synchronized     sha256 of the chart archive     the first digest stays
#   OCI, provider type pack           manifest digest of the push     the digest of the new push
#   OCI, provider type helm           manifest digest of the push     the first digest stays
#   OCI, provider type zarf           none                            none
#   Spectro pack registry             digest of the push              the digest of the new push
#   Helm with credentials             none: not synchronized          none
#
# Needs tofu, the AWS profile of terraform.tfvars for the host, oras, helm, zarf, spectro, curl,
# and jq. It needs no cluster. It turns registry_kinds on, and off again at its end, which removes
# the host and the registries; the Spectro pack registry and the profile of stage 2 go through the
# API. Results go under runs/<UTC time>/registry-kinds/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=registry-kinds
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"
for tool in oras helm zarf spectro curl jq tar; do
  command -v "$tool" > /dev/null || die "needs $tool"
done

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/registry-kinds}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

CHART=strata-live-chart
PACK=strata-live-pack
ZARF=strata-live-zarf
PACK_REG=""
PINNED=""
# The script needs no cluster, so its applies keep the cluster as they find it: none after
# live-profiles, and cluster A after live-up. The default of the variable is a cluster.
cluster_var=()

summary() {
  write_summary "registry kinds test" ", host \`${H:-}\`."
}
# cleanup: the objects of the API first, then the host and the registries of OpenTofu
cleanup() {
  [ -z "$PINNED" ] || "$live/api.sh" delete "$PINNED" >> steps.log 2>&1
  [ -z "$PACK_REG" ] || "$live/api.sh" pack-registry-delete "$PACK_REG" >> steps.log 2>&1
  tf_apply false "${cluster_var[@]}" >> steps.log 2>&1
  summary
}
trap cleanup EXIT

load_outputs
[ -n "$(out cluster_uid)" ] || cluster_var=(-var cluster=false)
AWS_PROFILE=${AWS_PROFILE:-$(out aws_profile)}
export AWS_PROFILE
[ -n "$AWS_PROFILE" ] || die "no AWS profile: set aws_profile in terraform.tfvars"
echo "== 0. the registry host" | tee -a steps.log
tf_apply false "${cluster_var[@]}" -var registry_kinds=true || die "the apply of the registry host failed"
H=$(out kinds_host)
U=$(out kinds_user)
KINDS_PASSWORD=$("$live/tf.sh" output -raw kinds_password)
export KINDS_PASSWORD
PALETTE_PROJECT_UID=$(out project_uid)
export PALETTE_PROJECT_UID
regf() { out kinds_registries | jq -r --arg k "$1" --arg f "$2" '.[$k][$f]'; }
for v in H U KINDS_PASSWORD PALETTE_PROJECT_UID; do
  [ -n "${!v}" ] || die "the tofu output behind $v is empty"
done

# the logins: the password goes on standard input only
echo "$KINDS_PASSWORD" | oras login "oci.$H" -u "$U" --password-stdin > login-oras.out 2>&1 || die "oras login failed"
echo "$KINDS_PASSWORD" | helm registry login "oci.$H" -u "$U" --password-stdin > login-helm.out 2>&1 || die "helm login failed"
echo "$KINDS_PASSWORD" | zarf tools registry login "oci.$H" -u "$U" --password-stdin > login-zarf.out 2>&1 || die "zarf login failed"
echo "$KINDS_PASSWORD" | spectro registry login "packs.$H" -u admin --password-stdin > login-spectro.out 2>&1 || die "spectro login failed"

# ---- the builds and the pushes -------------------------------------------------------------------

# build_chart LINE: CHART-1.0.0.tgz, with LINE added to values.yaml when given
build_chart() {
  rm -rf chart && mkdir chart && cp -r "$live/chart/$CHART" chart/ || return 2
  [ -z "${1:-}" ] || printf '%s\n' "$1" >> "chart/$CHART/values.yaml"
  helm package "chart/$CHART" --version 1.0.0 --destination chart > /dev/null
}
# build_pack LINE: PACK-1.0.0.tar.gz and the directory legacy/PACK, with LINE added to values.yaml
build_pack() {
  rm -rf pack legacy && mkdir pack legacy && cp -r "$live/pack/$PACK" pack/ || return 2
  [ -z "${1:-}" ] || printf '%s\n' "$1" >> "pack/$PACK/values.yaml"
  tar -czf "$PACK-1.0.0.tar.gz" -C pack "$PACK" || return 2
  # the spectro CLI needs a logo in the pack: one transparent pixel
  cp -r "pack/$PACK" legacy/ && base64 -d > "legacy/$PACK/logo.png" \
    <<< 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=='
}
# build_zarf LINE: the Zarf package 1.0.0, with LINE added to the data of its ConfigMap
build_zarf() {
  rm -rf zarf-src zarf-out && mkdir zarf-src zarf-out && cp -r "$live/zarf/$ZARF" zarf-src/ || return 2
  [ -z "${1:-}" ] || printf '  %s\n' "$1" >> "zarf-src/$ZARF/configmap.yaml"
  zarf package create "zarf-src/$ZARF" --architecture amd64 --confirm --output zarf-out > zarf-create.out 2>&1
}
# the push of each kind; each prints the digest of the push, when the tool gives one
push_charts() { curl -sSf -u "$U:$KINDS_PASSWORD" --data-binary "@chart/$CHART-1.0.0.tgz" "https://$1.$H/api/charts?force=true"; }
push_oci_pack() {
  oras push "oci.$H/strata-pack/spectro-packs/archive/$PACK:1.0.0" "$PACK-1.0.0.tar.gz" > oras.out 2>&1 || { cat oras.out; return 2; }
  grep -o 'sha256:[0-9a-f]\{64\}' oras.out | tail -1
}
push_oci_helm() {
  helm push "chart/$CHART-1.0.0.tgz" "oci://oci.$H/strata-helm" > helm-push.out 2>&1 || { cat helm-push.out; return 2; }
  grep -o 'sha256:[0-9a-f]\{64\}' helm-push.out | tail -1
}
push_oci_zarf() { zarf package publish "zarf-out/zarf-package-$ZARF-amd64-1.0.0.tar.zst" "oci://oci.$H/strata-zarf"; }
# push_packs: --force and --skip-digest-check, so that the push again of stage 3, and a run on a
# host that holds 1.0.0 already, write the version with no question
push_packs() {
  spectro pack push "legacy/$PACK" --registry-server "packs.$H" --force --skip-digest-check \
    --message "strata registry kinds test"
}

archive_digest() { sha256sum "chart/$CHART-1.0.0.tgz" | cut -d' ' -f1; }
upstream() { jq -r '.packs[0].hash.upstreamRaw' "$1"; }
field() { jq -r ".packs[0].$2 // \"\"" "$1"; }

# ---- 1. the first push -------------------------------------------------------------------------

echo "== 1. push 1.0.0 into each kind, sync, and record it" | tee -a steps.log
build_chart || die "building the chart failed"
build_pack || die "building the pack failed"
build_zarf || die "building the Zarf package failed"
check push-charts 0 "push $CHART 1.0.0 into the Helm repository" -- push_charts charts
check push-charts-auth 0 "push $CHART 1.0.0 into the Helm repository with credentials" -- push_charts charts-auth
check push-oci-pack 0 "push $PACK 1.0.0 into the OCI registry, base path strata-pack" -- push_oci_pack
check push-oci-helm 0 "push $CHART 1.0.0 into the OCI registry, base path strata-helm" -- push_oci_helm
check push-oci-zarf 0 "publish $ZARF 1.0.0 into the OCI registry, base path strata-zarf" -- push_oci_zarf
check push-packs 0 "push $PACK 1.0.0 into the Spectro pack registry, as admin" -- push_packs
H1=$(archive_digest)
M1_PACK=$(cat push-oci-pack.out)
M1_HELM=$(cat push-oci-helm.out)

check packs-registry 0 "a Palette registry over the Spectro pack registry, read as $U" -- \
  "$live/api.sh" pack-registry-create "strata-live-packs" "https://packs.$H" "$U"
PACK_REG=$(tail -1 packs-registry.out)
[ -n "$PACK_REG" ] && [ "$PACK_REG" != null ] || die "no uid of the Spectro pack registry"

# kind, the Palette registry, the sync kind, and the pack
KINDS="charts:$(regf charts uid):helm:$CHART
oci-pack:$(regf oci-pack uid):oci-basic:$PACK
oci-helm:$(regf oci-helm uid):oci-basic:$CHART
oci-zarf:$(regf oci-zarf uid):oci-basic:$ZARF
packs:$PACK_REG:pack:$PACK"
name_of() { case "$1" in packs) echo "strata-live-packs" ;; *) regf "$1" name ;; esac; }

sync_and_record() { # LABEL
  local kind uid sync pack
  # the list comes on descriptor 3: a command in the loop can read its standard input
  while IFS=: read -r -u 3 kind uid sync pack; do
    check "sync-$kind-$1" 0 "Palette reads the $kind registry again" -- "$live/api.sh" sync "$sync" "$uid"
    check "record-$kind-$1" 0 "#75: a record of pack:$(name_of "$kind")/$pack@1.0.0" -- \
      "$strata" snapshot "pack:$(name_of "$kind")/$pack@1.0.0" -o "$kind-$1.json"
  done 3<<< "$KINDS"
}
sync_and_record first

# the digest of each push of the Spectro pack registry, from the output of the spectro CLI
spectro_digest() { grep -o 'digest: sha256:[0-9a-f]*' "$1" | tail -1 | cut -d' ' -f2; }
check charts-digest 0 "#75: the Helm repository: the digest is the sha256 of the chart archive, bare hex" -- \
  test "$(upstream charts-first.json)" = "$H1"
check oci-pack-digest 0 "#75: OCI pack: the digest is the manifest digest of the push" -- \
  test "$(upstream oci-pack-first.json)" = "$M1_PACK"
check oci-helm-digest 0 "#75: OCI helm: the digest is the manifest digest of the push" -- \
  test "$(upstream oci-helm-first.json)" = "$M1_HELM"
check oci-zarf-digest 0 "#75: OCI zarf: Palette gives no digest" -- \
  test "$(field oci-zarf-first.json hash.upstreamSource)" = none
# no_digest_says FILE: the output of strata packs of the Zarf pack says no digest on the row of
# 1.0.0, and no digests in the heading
no_digest_says() {
  grep -qE "^$ZARF in .*, no digests\)$" "$1" && grep -qE '^  1\.0\.0 .*no digest$' "$1"
}
check packs-zarf 0 "#128: strata packs of the Zarf pack" -- \
  "$strata" packs "$ZARF" --registry "$(name_of oci-zarf)"
check packs-zarf-says 0 "#128: the row of 1.0.0 says no digest, and the heading says no digests" -- \
  no_digest_says packs-zarf.out
check packs-digest 0 "#75: the Spectro pack registry: the digest of the push" -- \
  test "$(upstream packs-first.json)" = "$(spectro_digest push-packs.out)"
check provider-types 0 "#75: each record of an OCI pack names the provider type of its registry, and no other names one" -- \
  test "$(field charts-first.json registryProviderType)/$(field oci-pack-first.json registryProviderType)/$(field oci-helm-first.json registryProviderType)/$(field oci-zarf-first.json registryProviderType)/$(field packs-first.json registryProviderType)" = "/pack/helm/zarf/"

# ---- 2. a Helm registry with credentials -------------------------------------------------------

echo "== 2. a chart from the Helm registry with credentials" | tee -a steps.log
# Palette lists a chart of this registry only while a profile pins it by name
check auth-listed ? "#75: whether Palette lists the chart of the Helm registry it does not synchronize" -- \
  "$strata" snapshot "pack:$(regf charts-auth name)/$CHART@1.0.0" -o auth-leaf.json
# the candidate of that profile, before the create: Palette makes the pack at the create (#154)
cat > auth-candidate.yaml <<EOF
name: strata-live-probe-auth
version: 1.0.0
type: add-on
cloudType: all
layers:
  - name: $CHART
    version: 1.0.0
    registryUid: $(regf charts-auth uid)
EOF
check auth-candidate 0 "#154: a candidate with the chart builds a record, before any profile pins it" -- \
  "$strata" snapshot --from-candidate auth-candidate.yaml -o auth-candidate.json
check auth-candidate-minted 0 "#154: its pack has no uid, no digest, the layer addon, and the tag as the version" -- \
  jq -e '.packs[0] | .packUid == "" and .hash.upstreamSource == "none" and .layer == "addon" and .type == "helm" and .version == "1.0.0"' auth-candidate.json
# the listing has no chart of the registry, and the error says why (#123)
check auth-packs 2 "#123: packs of the chart in the Helm registry Palette does not synchronize is pack-not-found" -- \
  "$strata" packs "$CHART" --registry "$(regf charts-auth name)"
check auth-packs-says 0 "#123: and the error names the registry that Palette does not sync" -- \
  grep -q "Palette does not sync the registry '$(regf charts-auth name)'" auth-packs.err
check auth-behind 2 "#123: behind cannot check the pin of the candidate record: the listing has no chart of the registry" -- \
  "$strata" behind auth-candidate.json
check auth-behind-says 0 "#123: and the line under the pin names the registry that Palette does not sync" -- \
  grep -q "Palette does not sync the registry '$(regf charts-auth name)'" auth-behind.out
check auth-pin 0 "a profile that pins $CHART 1.0.0 from it by name, made and published through the API" -- \
  "$live/api.sh" pin-profile "strata-live-probe-auth" "$(regf charts-auth uid)" helm "$CHART" 1.0.0
PINNED=$(tail -1 auth-pin.out)
check auth-record 0 "a record of that profile" -- "$strata" snapshot "$PINNED" --raw-dir raw/auth -o auth.json
check auth-candidate-same 0 "#154: same-release passes the candidate and the published version, with --allow no-digest" -- \
  "$strata" same-release auth-candidate.json auth.json --allow no-digest
"$strata" same-release auth-candidate.json auth.json --allow no-digest --json > auth-candidate-same.json
check auth-candidate-minted-only 0 "#154: and names the pack in mintedUidOnly" -- \
  jq -e --arg k "addon/$CHART" '.pairs[0].mintedUidOnly == [$k]' auth-candidate-same.json
check auth-candidate-again 0 "#154: a record of the candidate after the create" -- \
  "$strata" snapshot --from-candidate auth-candidate.yaml -o auth-candidate-again.json
check auth-candidate-listed 0 "#154: that record has the published root, as the listing now has the chart" -- \
  jq -e --slurpfile p auth.json '.root == $p[0].root' auth-candidate-again.json
check auth-no-digest 0 "#75 #71: the pack reference carries no digest, and Palette made a pack uid" -- \
  jq -e '.packs[0] | .hash.upstreamSource == "none" and .refDigest == "" and .registryDigest == "" and .packUid != ""' auth.json
check auth-verify 1 "#71: verify says UNVERIFIED" -- "$strata" verify auth.json
check auth-verify-says 0 "and names the pack" -- grep -q "UNVERIFIED: .*strata-live-chart has no upstream digest" auth-verify.out
check auth-verify-allowed 0 "#71: --allow-no-digest accepts it" -- "$strata" verify auth.json --allow-no-digest
check auth-same-release 1 "#71: same-release refuses it" -- "$strata" same-release auth.json auth.json
check auth-same-release-allowed 0 "#71: --allow no-digest accepts it" -- "$strata" same-release auth.json auth.json --allow no-digest

# ---- 3. the push again -------------------------------------------------------------------------

echo "== 3. push 1.0.0 again into each kind, with other bytes" | tee -a steps.log
build_chart "pushedAgain: yes" || die "building the chart again failed"
build_pack "pushedAgain: yes" || die "building the pack again failed"
build_zarf "pushedAgain: yes" || die "building the Zarf package again failed"
check push2-charts 0 "push $CHART 1.0.0 again into the Helm repository" -- push_charts charts
check push2-oci-pack 0 "push $PACK 1.0.0 again into strata-pack" -- push_oci_pack
check push2-oci-helm 0 "push $CHART 1.0.0 again into strata-helm" -- push_oci_helm
check push2-oci-zarf 0 "publish $ZARF 1.0.0 again into strata-zarf" -- push_oci_zarf
check push2-packs 0 "push $PACK 1.0.0 again into the Spectro pack registry" -- push_packs
M2_PACK=$(cat push2-oci-pack.out)
check push2-differs 0 "the two pushes of the OCI pack have two digests" -- test "$M1_PACK" != "$M2_PACK"
sync_and_record second

check charts-keeps 0 "#75: the Helm repository keeps the first digest" -- \
  test "$(upstream charts-second.json)" = "$H1"
check charts-verify 0 "#75: and the record from before verifies" -- "$strata" verify charts-first.json
check oci-pack-follows 0 "#75: OCI pack follows the push: the digest is the digest of the new push" -- \
  test "$(upstream oci-pack-second.json)" = "$M2_PACK"
check oci-pack-verify 1 "#75: and the record from before says DRIFT" -- "$strata" verify oci-pack-first.json
check oci-helm-keeps 0 "#75: OCI helm keeps the first digest" -- \
  test "$(upstream oci-helm-second.json)" = "$M1_HELM"
check oci-helm-verify 0 "#75: and the record from before verifies" -- "$strata" verify oci-helm-first.json
check oci-zarf-none 0 "#75: OCI zarf still gives no digest" -- \
  test "$(field oci-zarf-second.json hash.upstreamSource)" = none
check oci-zarf-verify 1 "#75 #71: and the record from before says UNVERIFIED" -- "$strata" verify oci-zarf-first.json
check packs-follows 0 "#75: the Spectro pack registry follows the push: the digest of the new push" -- \
  test "$(upstream packs-second.json)" = "$(spectro_digest push2-packs.out)"
check packs-verify 1 "#75: and the record from before says DRIFT" -- "$strata" verify packs-first.json

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

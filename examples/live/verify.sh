#!/usr/bin/env bash
# Run every strata live check against the cluster and profile versions that
# examples/live/terraform built, and write a summary. Exit 0 only when every check gets its
# expected exit code.
#
#   just live-up && just live-verify
#
# Needs terraform, jq, curl, helm, crane, and the Palette API key in PALETTE_API_KEY or
# ~/.palette.env; crane reads the OCI registry with the credentials a `crane auth login` or
# `docker login` stored. The strata binary is target/release/strata unless STRATA_BIN says
# otherwise. Results go under examples/live/runs/<UTC time>/, which is gitignored: raw API
# responses can hold variable values.
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
strata=${STRATA_BIN:-$repo/target/release/strata}
[ -x "$strata" ] || { echo "verify: no strata binary at $strata; run cargo build --release" >&2; exit 2; }

outputs=$("$here/tf.sh" output -json) || { echo "verify: no terraform outputs; run just live-up first" >&2; exit 2; }
out() { jq -r --arg k "$1" '.[$k].value // empty' <<<"$outputs"; }
C=$(out cluster_uid)
P=$(out release_profile_uid)
P2=$(out next_profile_uid)
N=$(out profile_name)
R=$(out registries_profile_uid)
AD=$(out addon_profile_uid)
AN=$(out addon_profile_name)
PALETTE_PROJECT_UID=$(out project_uid)
export PALETTE_PROJECT_UID
for v in C P P2 N R AD AN PALETTE_PROJECT_UID; do
  [ -n "${!v}" ] || { echo "verify: the terraform output behind $v is empty" >&2; exit 2; }
done

run=${OUT:-$here/runs/$(date -u +%Y%m%dT%H%M%SZ)}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

failures=0
rows=()
# check <id> <expected exit> <what it shows> -- command...
check() {
  local id=$1 want=$2 what=$3
  shift 4
  "$@" > "$id.out" 2> "$id.err"
  local got=$? verdict=PASS
  if [ "$got" != "$want" ]; then verdict=FAIL; failures=$((failures + 1)); fi
  rows+=("| $verdict | \`$id\` | $want | $got | $what |")
  printf '%-4s  %-26s expected %s, got %s\n' "$verdict" "$id" "$want" "$got"
}

# ---- #1: release versus run
check snapshot-cluster 0 "strata reads what a live cluster runs" -- \
  "$strata" snapshot "cluster:$C" --raw-dir raw/cluster -o cluster.json
check cluster-runs-release 0 "the cluster reports the release profile version" -- \
  jq -e --arg p "$P" '[.source.cluster.profiles[].uid] | index($p) != null' cluster.json
check cluster-runs-two-profiles 0 "#23: the cluster runs the release profile, then the add-on profile" -- \
  jq -e --arg p "$N" --arg a "$AN" '[.source.cluster.profiles[].name] == [$p, $a]' cluster.json
check snapshot-release 0 "record the version the cluster runs" -- \
  "$strata" snapshot "$P" --raw-dir raw/release -o release.json
check compare-release-cluster 0 "#1: a cluster that runs the recorded version is IDENTICAL" -- \
  "$strata" compare release.json "cluster:$C/$N"
check snapshot-addon 0 "record the add-on profile version the cluster runs" -- \
  "$strata" snapshot "$AD" --raw-dir raw/addon -o addon.json
check compare-addon-cluster 0 "#23: the add-on record is IDENTICAL to the cluster's add-on selection" -- \
  "$strata" compare addon.json "cluster:$C/$AN"
check snapshot-next 0 "record the newer version" -- \
  "$strata" snapshot "$P2" --raw-dir raw/next -o next.json
check compare-next-cluster 1 "#1: against a newer record, the cluster shows the change" -- \
  "$strata" compare next.json "cluster:$C/$N"
check compare-next-cluster-json 1 "the same comparison as JSON" -- \
  "$strata" compare next.json "cluster:$C/$N" --json
check only-the-edited-value 0 "#1: the only change is csi-aws-ebs reclaimPolicy" -- \
  jq -e '[.packs[] | select(.status != "unchanged") | .name] == ["csi-aws-ebs"]
         and ([.packs[] | .values.diff[]? | .path] | length == 1 and (.[0] | endswith("reclaimPolicy")))
         and .summary.variablesChanged == false' compare-next-cluster-json.out

# ---- #12: consumers
check consumers-release 1 "#12: the release version has a consumer" -- \
  "$strata" consumers "$P"
check consumers-release-json 1 "the same listing as JSON" -- \
  "$strata" consumers "$P" --json
check consumer-is-the-cluster 0 "#12: listed with its name, project, and state" -- \
  jq -e --arg c "$C" --arg p "$PALETTE_PROJECT_UID" \
    '(.consumers | map(select(.uid == $c))) as $m
     | ($m | length) == 1 and $m[0].name != "" and $m[0].projectUid == $p and $m[0].state != "unknown"' \
    consumers-release-json.out
check consumers-next 0 "#12: nothing runs the newer version" -- \
  "$strata" consumers "$P2"

# ---- #4: candidate
check candidate-file 0 "#21: write the newer version's record as a candidate file" -- \
  "$strata" candidate next.json -o candidate/profile.yaml --files-dir candidate/files
check snapshot-candidate 0 "#4: snapshot the candidate through registry lookups" -- \
  "$strata" snapshot --from-candidate candidate/profile.yaml -o candidate-snapshot.json
check candidate-same-release 0 "#4: the candidate's root equals the published root" -- \
  "$strata" same-release candidate-snapshot.json next.json

# ---- verify
check verify-release 0 "an untouched published version verifies" -- \
  "$strata" verify release.json
check verify-cluster-record 0 "a cluster record verifies against the cluster" -- \
  "$strata" verify cluster.json

# ---- #2: pack-ref digests of Helm and OCI packs
check snapshot-registries 0 "record the profile with a Helm pack and an OCI pack" -- \
  "$strata" snapshot "$R" --raw-dir raw/registries -o registries.json
check registries-kinds 0 "#2: the profile holds one helm pack and one oci pack" -- \
  jq -e '[.packs[].type] | sort == ["helm", "oci"]' registries.json
while IFS=$'\t' read -r kind reg name version; do
  check "registry-digest-$kind" 0 "#2: read the $kind pack's digest from the registry itself" -- \
    "$here/tenant.sh" digest "$kind" "$reg" "$name" "$version"
done < <(jq -r '.packs[] | select(.type == "helm" or .type == "oci") | [.type, .registryUid, .name, .version] | @tsv' \
           registries.json 2>/dev/null)

# ---- summary
{
  echo "# strata live verification"
  echo
  echo "Run \`$(basename "$run")\` with \`$("$strata" --version)\`. Cluster \`$C\`, project \`$PALETTE_PROJECT_UID\`,"
  echo "profile \`$N\` versions 1.0.0 (\`$P\`) and 1.0.1 (\`$P2\`), registries profile \`$R\`."
  echo
  echo "| result | check | expected exit | got | what it shows |"
  echo "|---|---|---|---|---|"
  printf '%s\n' "${rows[@]}"
  echo
  echo "## Pack-ref digests (#2, #3)"
  echo
  echo "The ref digest is the one Palette stamps on the profile's pack ref, Palette's digest the one"
  echo "it lists for the pack version, and the registry's digest the one \`tenant.sh digest\` read from"
  echo "the registry itself: the sha256 of the chart archive for Helm, the manifest digest for OCI."
  echo "Digests compare with any \`sha256:\` prefix removed."
  echo
  if [ -s release.json ] || [ -s registries.json ]; then
    echo "| pack | type | ref digest | Palette's digest | registry's digest | fed UPSTREAM | |"
    echo "|---|---|---|---|---|---|---|"
    for k in helm oci; do
      [ -s "registry-digest-$k.out" ] && jq -n --arg k "$k" --arg d "$(head -n 1 "registry-digest-$k.out")" '{($k): $d}'
    done | jq -s 'add // {}' > registry-digests.json
    jq -rs --slurpfile own registry-digests.json '
      def bare: sub("^sha256:"; "");
      def cell: if . == "" then "-" else "`" + .[0:(if startswith("sha256:") then 19 else 12 end)] + "`" end;
      .[].packs[]
      | (.refDigest // "") as $ref | (.registryDigest // "") as $pal | ($own[0][.type] // "") as $reg
      | ([$ref, $pal, $reg] | map(select(. != "") | bare) | unique) as $distinct
      | "| \(.name) | \(.type) | \($ref | cell) | \($pal | cell) | \($reg | cell) | \(.hash.upstreamSource) | \(
          if $ref == "" then "no ref digest" elif ($distinct | length) == 1 then "equal" else "differ" end) |"
    ' $([ -s release.json ] && echo release.json) $([ -s registries.json ] && echo registries.json)
  else
    echo "No release or registries record; see snapshot-release.err and snapshot-registries.err."
  fi
} > summary.md

echo
echo "$(( ${#rows[@]} - failures )) of ${#rows[@]} checks passed; details in $run"
[ "$failures" -eq 0 ]

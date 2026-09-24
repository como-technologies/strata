#!/usr/bin/env bash
# Run every strata live check against the cluster and profile versions that
# examples/live/terraform built, and write a summary. Read-only: it needs the live example at its
# baseline (baseline.sh) and leaves it there. Exit 0 only when every check gets its expected exit
# code.
#
#   just live-up && just live-verify
#
# Needs tofu, jq, curl, helm, crane, and the Palette API key in PALETTE_API_KEY or
# ~/.palette.env; crane reads the OCI registry with the credentials a `crane auth login` or
# `docker login` stored. The strata binary is target/release/strata unless STRATA_BIN says
# otherwise. Results go under examples/live/runs/<UTC time>/, which is gitignored: raw API
# responses can hold variable values.
set -uo pipefail
script=verify
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata; run cargo build --release"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
require_baseline
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
  [ -n "${!v}" ] || die "the tofu output behind $v is empty"
done

# ---- #1: release versus run
check snapshot-cluster 0 "strata reads what a live cluster runs" -- \
  "$strata" snapshot "cluster:$C" --raw-dir raw/cluster -o cluster.json
check cluster-runs-release 0 "the cluster reports the release profile version" -- \
  jq -e --arg p "$P" '[.source.cluster.profiles[].uid] | index($p) != null' cluster.json
# Palette lists the profiles of a cluster in either order, so the check compares the set.
check cluster-runs-two-profiles 0 "#23: the cluster runs the release profile and the add-on profile" -- \
  jq -e --arg p "$N" --arg a "$AN" '([.source.cluster.profiles[].name] | sort) == ([$p, $a] | sort)' cluster.json
check snapshot-release 0 "record the version the cluster runs" -- \
  "$strata" snapshot "$P" --raw-dir raw/release -o release.json
check compare-release-cluster 0 "#1: a cluster that runs the recorded version is IDENTICAL" -- \
  "$strata" compare release.json "cluster:$C/$N"
check consumers-line-names-the-cluster 0 "the compare of a record against its cluster names that cluster among the consumers of the version" -- \
  grep -qE '^consumers  .* has 1 consumer; .* is one of them$' compare-release-cluster.out
check snapshot-addon 0 "record the add-on profile version the cluster runs" -- \
  "$strata" snapshot "$AD" --raw-dir raw/addon -o addon.json
check compare-addon-cluster 0 "#23: the add-on record is IDENTICAL to the cluster's add-on selection" -- \
  "$strata" compare addon.json "cluster:$C/$AN"

# ---- #66: the cluster variables endpoint. The add-on profile declares one variable, so the
# copy that the cluster holds has a variables hash to compare with the profile's.
check addon-has-a-variable 0 "#66: the add-on profile declares one variable, purpose" -- \
  jq -e '[.variables[].name] == ["purpose"] and .variablesUnavailable != true' addon.json
check cluster-variables-endpoint 0 "#66: GET /v1/spectroclusters/<uid>/variables serves the variable of the add-on profile" -- \
  jq -e '[.. | objects | select(.name? == "purpose")] | length >= 1' raw/cluster/variables.json
check snapshot-addon-copy 0 "#66: record the copy of the add-on profile that the cluster holds" -- \
  "$strata" snapshot "cluster:$C/$AN" -o addon-copy.json
check addon-copy-variables-hash 0 "#66: the copy has the variables hash of the profile, so the cluster variables endpoint repeats the definition fields" -- \
  jq -e --slurpfile a addon.json \
    '.variablesUnavailable != true and .variablesHash == $a[0].variablesHash and [.variables[].name] == ["purpose"]' \
    addon-copy.json
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

# ---- #63: the profiles a cluster applies
check list-applied 0 "#63: list --applied names the profiles each cluster runs" -- \
  "$strata" list clusters --applied
check list-applied-json 0 "the same listing as JSON" -- \
  "$strata" list clusters --applied --json
check applied-is-what-the-cluster-runs 0 "#63: the listed profiles are those the cluster record names" -- \
  jq -e --slurpfile record cluster.json --arg c "$C" \
    '(.clusters | map(select(.uid == $c))) as $m
     | ($m | length) == 1
     and ([$m[0].profiles[] | {uid, name, version}]
          == [$record[0].source.cluster.profiles[] | {uid, name, version}])' \
    list-applied-json.out
check cluster-listing 0 "#63: read the cluster listing that strata lists clusters from" -- \
  "$live/tenant.sh" listing
check listing-profile-fields 0 "#63: the fields of the cluster listing whose name holds 'profile'" -- \
  jq -er '[(if type == "array" then . else (.items // []) end)[] | paths | join(".")]
          | map(select(test("profile"; "i"))) | unique | join(" ")' \
    cluster-listing.out

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
    "$live/tenant.sh" digest "$kind" "$reg" "$name" "$version"
done < <(jq -r '.packs[] | select(.type == "helm" or .type == "oci") | [.type, .registryUid, .name, .version] | @tsv' \
           registries.json 2>/dev/null)

# ---- summary
{
  echo "# strata live verification"
  echo
  echo "Run \`$(basename "$run")\` with \`$("$strata" --version)\`. Cluster \`$C\`, project \`$PALETTE_PROJECT_UID\`,"
  echo "profile \`$N\` versions 1.0.0 (\`$P\`) and 1.0.1 (\`$P2\`), registries profile \`$R\`."
  echo
  summary_table
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

total=$(command grep -cE '^(PASS|FAIL) ' steps.log)
echo
echo "$((total - failures)) of $total checks passed; details in $run"
[ "$failures" -eq 0 ]

#!/usr/bin/env bash
# The knurl adapter against the live tenant: each method of the platform port, on the real profile
# versions, the real registries, and each cluster that runs a version. It reads, and it writes
# nothing in Palette. It needs the live profiles (just live-profiles). The instance checks need a
# cluster that runs the release version (just live-up); without one, they are SEEN and not judged.
#
#   examples/live/knurl.sh
#
# Each check runs the knurl side, `adapter/examples/observe.rs`, and the strata command for the
# same object, and compares the two answers: the exact digest is the root, the content digest is
# contentDigest, the content of a leaf is the digest of the pack in its registry, and pinned-by
# is consumers. So the checks say that the port gives what strata gives, against a tenant and not
# a stand-in. The rules the port itself states are in adapter/tests/conformance.rs.
#
#   1  a profile version: the two digests, its pins, verify, and the identity of its definition
#   2  each cluster that runs it: the state, and the copies it holds against its cluster records
#   3  a leaf, and a leaf that two registries hold as two archives (#55)
#   4  a version that the tenant does not have
#
# Results go under runs/<UTC time>/knurl/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=knurl
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata; run just build"

repo=$(cd "$live/../.." && pwd)
observe=$repo/target/adapter/debug/examples/observe
export CARGO_NET_GIT_FETCH_WITH_CLI=true
cargo build --quiet --manifest-path "$repo/adapter/Cargo.toml" --target-dir "$repo/target/adapter" --example observe \
  || die "the observe example does not build"

load_outputs
N=$(out profile_name)
V=1.0.0
# The one-registry leaf is the OCI pack of the registries profile: the tenant has it in the one
# registry that terraform.tfvars names. The release packs are in two registries of this tenant,
# Public Repo and Palette Registry, with two archives each (packs.sh, registries-differ), so the
# csi pack is the two-archive leaf.
R=strata-live-registries
two=csi-aws-ebs
two_version=1.65.0

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/knurl}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

"$strata" snapshot "$R@1.0.0" -o registries.json || die "no record of $R@1.0.0; run just live-profiles"
leaf=$(jq -r '.packs[] | select(.type == "oci") | .name' registries.json | head -1)
leaf_version=$(jq -r --arg n "$leaf" '.packs[] | select(.name == $n) | .version' registries.json)
[ -n "$leaf" ] || die "$R@1.0.0 has no oci pack to read as a leaf"

summary() {
  {
    echo "# knurl adapter test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, profile \`$N@$V\`,"
    echo "leaf \`$leaf $leaf_version\`, two archives \`$two $two_version\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

# ---- the comparisons. Each one is a check, so its exit code is the verdict. ----------------------

# field FILE KEY: the value on the line `KEY value` that observe prints; the first such line
field() { awk -v k="$2" '$1 == k { print $2; exit }' "$1"; }
# digest_of FILE KEY: the sha256 that a strata record holds at .KEY, in the written form of knurl
digest_of() { echo "sha256:$(jq -r ".$2" "$1" | sed 's/^sha256://')"; }

exact_is_root() { [ "$(field unit.out exact)" = "$(digest_of record.json root)" ]; }
content_is_digest() { [ "$(field unit.out content)" = "$(digest_of record.json contentDigest)" ]; }
# Each pin: a pack from a registry, with its version and the digest of the registry.
pins_are_packs() {
  diff <(awk '$1 == "pin" { print $2, $3, $4 }' unit.out | sort) \
       <(jq -r '.packs[] | select(.type != "manifest") | "\(.name) \(.version) sha256:\(.hash.upstreamRaw | sub("^sha256:"; ""))"' record.json | sort)
}
pinned_by_is_consumers() {
  diff <(awk 'NR > 1 { print $1 }' pinned-by.out | sort) \
       <(jq -r '.consumers[] | if .name == "" then .uid else .name end' consumers.out | sort)
}
# copies_match CLUSTER UID: each `holds NAME VERSION` of knurl, with its exact and its content,
# against the record of that copy, cluster:<uid>/<name>. The copy is the cluster's, and not the
# published version.
copies_match() {
  local cluster=$1 uid=$2 name exact content
  awk '$1 == "holds" { name = $2 } $1 == "exact" { e[name] = $2 } $1 == "content" { print name, e[name], $2 }' "instance-$cluster.out" |
  while read -r name exact content; do
    "$strata" snapshot "cluster:$uid/$name" -o "copy-$cluster-$name.json" || return 1
    [ "$exact" = "$(digest_of "copy-$cluster-$name.json" root)" ] || { echo "$name: exact $exact is not the root"; return 1; }
    [ "$content" = "$(digest_of "copy-$cluster-$name.json" contentDigest)" ] || { echo "$name: content $content is not contentDigest"; return 1; }
    echo "$name: the two digests are those of the cluster record"
  done
}
leaf_exact_is_root() { [ "$(field leaf.out exact)" = "$(digest_of leaf.json root)" ]; }
leaf_content_is_archive() { [ "$(field leaf.out content)" = "$(digest_of leaf.json 'packs[0].hash.upstreamRaw')" ]; }
leaf_has_no_pin() { ! grep -q '^pin ' leaf.out; }
two_archives_named() {
  grep -q 'Terminal failure' two-archives.err &&
  grep -q 'is 2 archives under one version' two-archives.err &&
  grep -q 'Palette Registry has sha256:' two-archives.err &&
  grep -q 'Public Repo has sha256:' two-archives.err
}
not_found_is_terminal() { grep -q 'Terminal failure' not-found.err; }

# ---- the checks ---------------------------------------------------------------------------------

echo "== a profile version" | tee -a steps.log
check unit 0 "knurl: observe_unit reads $N@$V" -- \
  "$observe" unit intermediate "$N" "$V"
check unit-record 0 "strata: the record of $N@$V" -- \
  "$strata" snapshot "$N@$V" -o record.json
check unit-exact-is-root 0 'the exact digest of knurl is the root of strata' -- exact_is_root
check unit-content-is-digest 0 'the content digest of knurl is contentDigest of strata' -- content_is_digest
check unit-pins-are-packs 0 'each pin of knurl is a pack of the record from a registry, with its version and its digest' -- pins_are_packs
check verify 0 "knurl: verify says Unchanged of $N@$V" -- \
  "$observe" verify "$N" "$V"
check verify-strata 0 'strata: verify says the same of the record' -- \
  "$strata" verify record.json
check identity 0 "knurl: the definition of $N@$V, resolved against the registries, gives its content digest" -- \
  "$observe" identity "$N" "$V"
check identity-says-same 0 'and the two sides are the same' -- \
  grep -q '^SAME:' identity.out

echo "== each cluster that runs it" | tee -a steps.log
check pinned-by 0 "knurl: pinned_by names each cluster that runs $N@$V" -- \
  "$observe" pinned-by "$N" "$V"
check consumers '?' "strata: consumers of $N@$V; 1 when a cluster runs it, 0 when none does" -- \
  "$strata" consumers "$N@$V" --json
check pinned-by-is-consumers 0 'the instances of knurl are the consumers of strata, by name' -- pinned_by_is_consumers
clusters=$(jq -r '.consumers[] | if .name == "" then .uid else .name end' consumers.out 2>/dev/null)
if [ -z "$clusters" ]; then
  printf '%-4s  %-32s no cluster runs %s; run just live-up for the instance checks\n' SEEN instance "$N@$V" | tee -a steps.log
  printf '%s\t%s\n' instance "no cluster runs $N@$V, so observe_instance was not checked" >> checks.tsv
fi
for cluster in $clusters; do
  uid=$(jq -r --arg n "$cluster" '.consumers[] | select(.name == $n or .uid == $n) | .uid' consumers.out)
  check "instance-$cluster" 0 "knurl: observe_instance reads the cluster $cluster" -- \
    "$observe" instance "$cluster"
  check "instance-$cluster-running" 0 "the state of $cluster is Running" -- \
    grep -q '^state *Running$' "instance-$cluster.out"
  check "instance-$cluster-copies" 0 "each copy that $cluster holds has the two digests of its cluster record" -- \
    copies_match "$cluster" "$uid"
done

echo "== a leaf" | tee -a steps.log
check leaf 0 "knurl: observe_unit reads the pack $leaf $leaf_version" -- \
  "$observe" unit leaf "$leaf" "$leaf_version"
check leaf-record 0 "strata: the record of pack:$leaf@$leaf_version, in the one registry that has it" -- \
  "$strata" snapshot "pack:$leaf@$leaf_version" -o leaf.json
check leaf-exact-is-root 0 'the exact digest of a leaf in one registry is the root of its record' -- leaf_exact_is_root
check leaf-content-is-archive 0 'the content digest of a leaf is the digest of the pack in its registry' -- leaf_content_is_archive
check leaf-no-pins 0 'a leaf pins nothing' -- leaf_has_no_pin
check two-archives 2 "#55: $two $two_version is two archives in two registries, so knurl cannot say which one an address means" -- \
  "$observe" unit leaf "$two" "$two_version"
check two-archives-says 0 'the failure is terminal, and names each registry with its digest' -- two_archives_named

echo "== a version the tenant does not have" | tee -a steps.log
check not-found 2 "a version of $N that Palette does not have is a failure" -- \
  "$observe" unit intermediate "$N" 9.9.9
check not-found-terminal 0 'and the failure is terminal: a retry does not help' -- not_found_is_terminal

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

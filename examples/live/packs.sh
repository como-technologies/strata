#!/usr/bin/env bash
# The pack ref test (#55): `pack:` refs read against the tenant's own registries. It reads and
# writes nothing in Palette, builds no cluster, and needs no OpenTofu, so it runs whenever the key
# works.
#
#   examples/live/packs.sh
#
#   1  the packs the profiles use, and one pack version against another from the registry
#   2  a pack version against itself, a record of one, and the record verified again
#   3  the refs that name a registry or a tag, and the errors for a version and a registry that
#      the tenant does not have
#
# The pack versions below are the ones the live profiles pin, and the two csi versions that the
# offline examples also use. Results go under runs/<UTC time>/packs/, or OUT. Exit 0 only when no
# check fails.
set -uo pipefail
script=packs
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

# What the tenant holds: the csi pack of the live profiles, the version before it, and the same
# pack in two registries, which the registry-move run put there. A bare ref is ambiguous here, so
# every reading ref names its registry.
pack=csi-aws-ebs
old=1.63.1
new=1.65.0
reg="Public Repo"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/packs}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  {
    echo "# pack ref test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, pack \`$pack\`,"
    echo "versions \`$old\` and \`$new\`, registry \`$reg\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

echo "== the packs the profiles use" | tee -a steps.log
check list-packs 0 '#55: the profiles list the pack versions they pin' -- \
  "$strata" list --packs

echo "== one pack version against another" | tee -a steps.log
check compare-versions 1 "#55: pack:$reg/$pack@$old against @$new differs" -- \
  "$strata" compare "pack:$reg/$pack@$old" "pack:$reg/$pack@$new"
check compare-json 1 '#55: the diff document names the pack on both sides' -- \
  "$strata" compare "pack:$reg/$pack@$old" "pack:$reg/$pack@$new" --json
check compare-same 0 "#55: pack:$reg/$pack@$new against itself is identical" -- \
  "$strata" compare "pack:$reg/$pack@$new" "pack:$reg/$pack@$new"

echo "== a record of a pack version" | tee -a steps.log
check record 0 '#55: snapshot writes a record of a pack version' -- \
  "$strata" snapshot "pack:$reg/$pack@$new" -o pack.json
check record-title 0 '#55: the record names the pack, not a profile' -- \
  grep -q "\"ref\": \"pack:$reg/$pack@$new\"" pack.json
check verify-record 0 '#55: the record verifies against the registry it came from' -- \
  "$strata" verify pack.json

echo "== the other forms, and the errors" | tee -a steps.log
check latest 0 "#55: pack:$reg/$pack@latest resolves to a version" -- \
  "$strata" snapshot "pack:$reg/$pack@latest" -o latest.json
check latest-is-newest 0 '#55: latest gives the newest version the registry offers' -- \
  grep -q "\"version\": \"$new\"" latest.json
check two-registries 2 '#55: a bare ref for a pack in two registries is an error that names both' -- \
  "$strata" compare "pack:$pack@$old" "pack:$pack@$new"
check two-registries-says 0 '#55: the error names each registry and the ref that fixes it' -- \
  grep -q "is in several registries: Palette Registry, Public Repo; name one, as in pack:<registry>/" two-registries.err
check other-registry 0 '#55: the same pack version reads from the other registry too' -- \
  "$strata" snapshot "pack:Palette Registry/$pack@$new" -o oci.json
check registries-differ 1 '#55: the two registries hold different builds of the version' -- \
  "$strata" compare pack.json oci.json
check unknown-version 2 '#55: a version the registry does not have is an error' -- \
  "$strata" snapshot "pack:$reg/$pack@0.0.0-nope" -o nope.json
check unknown-registry 2 '#55: a registry the tenant does not have is an error' -- \
  "$strata" snapshot "pack:No Such Registry/$pack@$new" -o nope2.json

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

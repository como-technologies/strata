#!/usr/bin/env bash
# The pack ref test (#55): `pack:` refs read against the tenant's own registries. It reads and
# writes nothing in Palette, and builds no cluster. It reads the CSI pack of the cloud and its two
# versions from the OpenTofu outputs, so it needs `just live-profiles` or `just live-up` once.
#
#   examples/live/packs.sh
#
#   1  the packs the profiles use, and one pack version against another from the registry
#   2  a pack version against itself, a record of one, and the record verified again
#   3  the refs that name a registry or a tag, and the errors for a version and a registry that
#      the tenant does not have
#
# The CSI versions are the one the live profiles pin and an older one, from the file of the cloud
# in terraform/. On AWS they are the two that the offline examples also use. Results go under runs/<UTC time>/packs/, or OUT. Exit 0 only when no
# check fails.
set -uo pipefail
script=packs
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

# What the tenant holds: the csi pack of the live profiles, the version before it, and the same
# pack in two registries, which the registry-move run put there. A bare ref is ambiguous here, so
# every reading ref names its registry.
load_outputs
pack=$(out csi_pack_name)
old=$(out csi_previous_version)
new=$(out csi_version)
reg="Public Repo"
[ -n "$pack" ] && [ -n "$old" ] && [ -n "$new" ] || die "no CSI pack in the tofu outputs; run just live-profiles"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/packs}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1
# the newest version the registry offers today; it moves when a new version is published
newest=$("$strata" packs "$pack" --registry "$reg" --latest --json | jq -r '.registries[0].versions[0].version')

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
check compare-images 0 "#77: the diff flags IMAGES, and names each image that moved, by repository" -- \
  jq -e '.packs[0] | .images and (.imageChanges | length > 0) and .authored' compare-json.out
check compare-images-says 0 "#77: the report has an image line under the pack, and the count under root" -- \
  bash -c "grep -qE '^      [~+-] image ' compare-versions.out && grep -qE '^images     [0-9]+ image' compare-versions.out"
check compare-same 0 "#55: pack:$reg/$pack@$new against itself is identical" -- \
  "$strata" compare "pack:$reg/$pack@$new" "pack:$reg/$pack@$new"

echo "== a record of a pack version" | tee -a steps.log
check record 0 '#55: snapshot writes a record of a pack version' -- \
  "$strata" snapshot "pack:$reg/$pack@$new" -o pack.json
check record-title 0 '#55: the record names the pack, not a profile' -- \
  grep -q "\"ref\": \"pack:$reg/$pack@$new\"" pack.json
check verify-record 0 '#55: the record verifies against the registry it came from' -- \
  "$strata" verify pack.json
check record-images 0 '#77: the record carries the image list of the values' -- \
  jq -e '.packs[0].images | length > 0' pack.json
check old-record 0 '#77: a record written before the image list: strata reads the list from its values' -- \
  bash -c "jq 'del(.packs[].images, .packs[].charts)' pack.json > pack-old.json && \"$strata\" verify pack-old.json"

echo "== the other forms, and the errors" | tee -a steps.log
check latest 0 "#55: pack:$reg/$pack@latest resolves to a version" -- \
  "$strata" snapshot "pack:$reg/$pack@latest" -o latest.json
check latest-is-newest 0 '#55: latest gives the newest version the registry offers' -- \
  grep -q "\"version\": \"$newest\"" latest.json
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

echo "== the versions a registry offers (#54)" | tee -a steps.log
check packs-one 0 '#54: packs lists the versions of one registry, newest first' -- \
  "$strata" packs "$pack" --registry "$reg"
check packs-newest 0 '#54: the newest version is the first line, and it carries the floating tag' -- \
  bash -c "head -2 packs-one.out | tail -1 | grep -q '$newest' && head -2 packs-one.out | tail -1 | grep -q '1.x'"
check packs-latest 0 '#54: --latest gives one line for each registry that has the pack' -- \
  "$strata" packs "$pack" --latest
check packs-two-registries 0 '#54: both registries of this tenant answer' -- \
  bash -c "grep -c '^$pack in ' packs-latest.out | grep -qx 2"
check packs-paged 0 '#54: a pack with more than 100 versions lists completely' -- \
  "$strata" packs kubernetes --registry "$reg"
check packs-paged-count 0 '#54: the listing passes the 100 of one page' -- \
  bash -c "head -1 packs-paged.out | grep -oE '[0-9]+ versions' | grep -oE '^[0-9]+' | awk '{exit !(\$1 > 100)}'"
check packs-cloud 0 '#54: --cloud keeps the versions of one cloud' -- \
  "$strata" packs kubernetes --registry "$reg" --cloud "$(out cloud)" --latest
check packs-cloud-none 0 '#54: a cloud no version supports is an empty answer, not an error' -- \
  "$strata" packs kubernetes --registry "$reg" --cloud no-such-cloud
check packs-cloud-none-says 0 '#54: and it says so' -- \
  grep -q 'no version of kubernetes supports no-such-cloud' packs-cloud-none.out
check packs-json 0 '#54: --json writes strata-packs/1' -- \
  bash -c "\"$strata\" packs $pack --registry '$reg' --latest --json | jq -e '.schema == \"strata-packs/1\" and (.registries[0].versions[0].tags | length > 0)'"
check packs-unknown-registry 2 '#54: an unknown registry is an error that lists the visible ones' -- \
  "$strata" packs "$pack" --registry "No Such Registry"
check packs-unknown-pack 2 '#54: a pack no registry has is pack-not-found' -- \
  "$strata" packs no-such-pack-anywhere

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

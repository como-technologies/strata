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
#   4  the options of a pack template, and a candidate whose values select one (#151)
#   5  the annotations of the OS pack: the pack ref, the registry, and a record (#189, #253)
#   6  the time of a pack in the listing, and no such time in verify (#191)
#   7  the architectures of a version, and what a move to a newer version drops (#126)
#   8  a pin that is current against the copy of a registry whose last sync did not complete (#122)
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
reg=$public_repo
[ -n "$pack" ] && [ -n "$old" ] && [ -n "$new" ] || die "no CSI pack in the tofu outputs; run just live-profiles"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/packs}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1
# the newest version the registry offers today; it moves when a new version is published
newest=$("$strata" packs "$pack" --registry "$reg" --latest --json | jq -r '.registries[0].versions[0].version')

summary() {
  write_summary "pack ref test" ", pack \`$pack\`," \
    "versions \`$old\` and \`$new\`, registry \`$reg\`."
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
check verify-sync 0 '#141, #192: the verdict ends with the last sync of the registry: its start, a sync in progress, or a failure' -- \
  grep -qE '; (the last complete sync of the registry started |a sync of the registry is in progress|the last sync of the registry failed)' verify-record.out
check verify-sync-json 0 '#141: --json gives verify.registrySync with lastSyncedTime and status' -- \
  bash -c "\"$strata\" verify pack.json --json | jq -e '.verify.registrySync | .status and has(\"lastSyncedTime\")'"
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
# Palette serves /v1/packs 100 entries at a time. strata reads each entry of the name in all
# registries, and then sorts the entries by registry. So the sum of all registries passes one
# page, also when no single registry holds more than 100 versions of the pack.
check packs-paged-listing 0 '#180: the listing of kubernetes holds more entries than one page' -- \
  bash -c "\"$live/api.sh\" get '/v1/packs?filters=spec.name=kubernetes&limit=1' | jq -e '.listmeta.count > 100'"
check packs-paged 0 '#54: a pack with more than 100 entries in the listing lists completely' -- \
  "$strata" packs kubernetes
check packs-paged-count 0 '#180: the versions of all registries pass the 100 of one page' -- \
  bash -c "grep -oE '^kubernetes in .* \([0-9]+ versions?' packs-paged.out | grep -oE '[0-9]+ versions?\$' | awk '{s += \$1} END {exit !(s > 100)}'"
check packs-cloud 0 '#54: --cloud keeps the versions of one cloud' -- \
  "$strata" packs kubernetes --registry "$reg" --cloud "$(out cloud)" --latest
check packs-cloud-none 0 '#54: a cloud no version supports is an empty answer, not an error' -- \
  "$strata" packs kubernetes --registry "$reg" --cloud no-such-cloud
check packs-cloud-none-says 0 '#54: and it says so' -- \
  grep -q 'no version of kubernetes supports no-such-cloud' packs-cloud-none.out
check packs-json 0 '#54: --json writes strata-packs/1' -- \
  bash -c "\"$strata\" packs $pack --registry '$reg' --latest --json | jq -e '.schema == \"strata-packs/1\" and (.registries[0].versions[0].tags | length > 0)'"
check packs-palette-latest 0 '#124: the listing marks one version of the registry as the latest of Palette, and --latest --json gives it paletteLatest' -- \
  bash -c "\"$strata\" packs kubernetes --registry '$reg' --latest --json | jq -e '[.registries[0].versions[] | select(.paletteLatest)] | length == 1'"
check packs-rev-floating-tag 0 '#124: the floating tag 1.19.x of cert-manager names 1.19.3, and not the rebuild 1.19.3-rev1' -- \
  bash -c "\"$strata\" packs cert-manager --registry '$reg' --json | jq -e '[.registries[0].versions[] | select(.tags | index(\"1.19.x\")) | .version] == [\"1.19.3\"]'"
check packs-unknown-registry 2 '#54: an unknown registry is an error that lists the visible ones' -- \
  "$strata" packs "$pack" --registry "No Such Registry"
check packs-unknown-pack 2 '#54: a pack no registry has is pack-not-found' -- \
  "$strata" packs no-such-pack-anywhere

# headlamp 0.45.0 has the template parameter access, and its option public has the preset
# charts.headlamp.service.type: LoadBalancer. No route of Palette gives the presets.
echo "== the presets of a template option (#151)" | tee -a steps.log
preset_key=pack.palette.config.headlamp.access
check preset-listing 0 '#151: the listing gives each option of the access parameter of headlamp 0.45.0, and no option has presets' -- \
  bash -c "\"$live/api.sh\" get '/v1/packs?filters=spec.name=headlamp&limit=100' | jq -e '[.items[] | select(.spec.version == \"0.45.0\") | .spec.template.parameters.inputParameters[]? | select(.targetKey == \"$preset_key\") | .options[]] | length > 0 and all(has(\"presets\") | not)'"
cat > preset-candidate.yaml <<EOF
name: barney
version: 1.0.0
type: add-on
registry: $reg
layers:
  - name: headlamp
    version: 0.45.0
    values: |
      pack:
        palette:
          config:
            headlamp:
              access: public
EOF
check preset-warning 0 '#151: a candidate whose values select the option public records it' -- \
  env -u STRATA_QUIET "$strata" snapshot --from-candidate preset-candidate.yaml -o preset-candidate.json
check preset-warning-says 0 '#151: the warning names the pack, the option, the parameter, and the key' -- \
  grep -qF "warning: headlamp: the values select the option public of access ($preset_key)" preset-warning.err
check preset-recorded 0 '#151: the record has packs[].presetOptions[] with that selection' -- \
  jq -e --arg k "$preset_key" '.packs[0].presetOptions == [{"parameter": "access", "targetKey": $k, "option": "public"}]' preset-candidate.json

# A record keeps only the annotation imageId of a pack reference, outside the root (#253). Palette
# copies the annotations from the pack version in the registry, and the OS pack of the Public Repo
# names no imageId, so its record keeps none.
echo "== the annotations of the OS pack (#189, #253)" | tee -a steps.log
check os-record 0 "#189: a record of the release profile, with its $(out os_pack_name) OS pack" -- \
  "$strata" snapshot "$(out release_profile_uid)" -o release.json
check os-annotations 0 '#253: the record keeps no annotation of the OS pack, because the pack ref has no imageId' -- \
  jq -e '[.packs[] | select(.layer == "os")] | length == 1 and (.[0] | has("annotations") | not)' release.json
os_uid=$(jq -r '.packs[] | select(.layer == "os") | .packUid' release.json)
check os-profile 0 '#189: the document of the release profile, with the pack ref of the OS pack' -- \
  "$live/api.sh" get "/v1/clusterprofiles/$(out release_profile_uid)"
check os-annotations-ref 0 '#189: the pack ref of the OS pack has annotations, and no imageId' -- \
  jq -e '[.spec.published.packs[] | select(.layer == "os") | .annotations] | length == 1 and (.[0] | length > 0 and (has("imageId") | not))' os-profile.out
check os-pack-registry 0 '#189: the pack version of the OS pack in the registry' -- \
  "$live/api.sh" get "/v1/packs/$os_uid"
check os-annotations-registry 0 '#189: the annotations of the pack reference are those of the pack version in the registry' -- \
  jq -e --slurpfile r os-pack-registry.out --arg u "$os_uid" \
  '[$r[0].packValues[] | select(.packUid == $u) | .annotations] == [.spec.published.packs[] | select(.layer == "os") | .annotations]' os-profile.out

# The listing gives each pack version a lastModifiedTimestamp. It is the create time of the
# version, so verify gives no time of the last change for a pack record.
echo "== the time of a pack (#191)" | tee -a steps.log
check pack-time-listing 0 "#191: the listing gives each version of $pack a lastModifiedTimestamp equal to its creationTimestamp" -- \
  bash -c "\"$live/api.sh\" get '/v1/packs?filters=spec.name=$pack&limit=100' | jq -e '[.items[].metadata] | length > 0 and all(.lastModifiedTimestamp == .creationTimestamp)'"
check verify-no-time 0 '#191: verify --json gives no verify.lastModified for a pack record' -- \
  bash -c "\"$strata\" verify pack.json --json | jq -e '.verify | has(\"lastModified\") | not'"

# The listing names the architectures of a version. In the Public Repo, loki 2.10.2 supports
# amd64 and arm64, and 3.6.7 supports amd64 alone, so behind says what the move drops.
echo "== the architectures of a version, and what a move drops (#126)" | tee -a steps.log
check arch-listing 0 '#126: the listing names the architectures of loki 2.10.2 as amd64,arm64, and of 3.6.7 as amd64' -- \
  bash -c "\"$live/api.sh\" get '/v1/packs?filters=spec.name=loki&limit=100' | jq -e '.items | (map(select(.spec.version == \"2.10.2\") | .spec.annotations.architecture) | index(\"amd64,arm64\") != null) and (map(select(.spec.version == \"3.6.7\") | .spec.annotations.architecture) | index(\"amd64\") != null)'"
check arch-record 0 '#126: a record of loki 2.10.2 from the Public Repo' -- \
  "$strata" snapshot "pack:$reg/loki@2.10.2" -o loki.json
check arch-behind 1 '#126: the record is behind 3.6.7' -- \
  "$strata" behind loki.json --no-impact
check arch-behind-says 0 '#126: the line says that the move to 3.6.7 drops arm64' -- \
  grep -Eq '^~ \[addon\] loki +2\.10\.2 +Public Repo +behind: .*3\.6\.7 \(drops arm64\)' arch-behind.out
check arch-behind-json 0 '#126: --json gives newer[].drops on 3.6.7' -- \
  bash -c "\"$strata\" behind loki.json --no-impact --json | jq -e '[.pins[0].newer[] | select(.version == \"3.6.7\") | .drops] == [[\"arm64\"]]'"

# A pin that is current is current against the copy of its registry in Palette. When the last sync
# of the registry did not complete, the copy is the copy of an earlier sync, and the pin names that
# sync (#122). The Bitnami registry of the tenant is the case: its sync stays InProgress. The check
# reads the state of the sync first, so it holds when the sync completes one day.
echo "== a pin that is current against the copy of an older sync (#122)" | tee -a steps.log
check bitnami-sync 0 '#122: the Helm summary listing gives the sync of the Bitnami registry' -- \
  "$live/api.sh" get /v1/registries/helm/summary
bitnami_sync=$(jq -r '.items[] | select(.metadata.name == "Bitnami") | .status.sync.status' bitnami-sync.out)
nginx=$("$strata" packs nginx --registry Bitnami --latest --json | jq -r '.registries[0].versions[0].version')
check stale-record 0 "#122: a record of nginx@$nginx from Bitnami, whose last sync is $bitnami_sync" -- \
  "$strata" snapshot "pack:Bitnami/nginx@$nginx" -o nginx.json
check stale-current 0 '#122: the newest version of the copy is current' -- \
  "$strata" behind nginx.json --no-impact
if [ "$bitnami_sync" = Completed ]; then
  check stale-no-note 0 '#122: the last sync completed, so the pin has no sync note' -- \
    bash -c "! grep -q 'sync of the registry' stale-current.out"
else
  check stale-note 0 '#122: the pin is current against the copy of an older sync, and its line names that sync' -- \
    grep -qE 'current.*(a sync of the registry is in progress since .*, and the last complete sync of the registry started|the last complete sync of the registry started .*; the last sync of the registry failed)' stale-current.out
  check stale-json 0 '#122: --json gives pins[0].registrySync with the status that Palette gives' -- \
    bash -c "\"$strata\" behind nginx.json --no-impact --json | jq -e --arg s \"$bitnami_sync\" '.pins[0].registrySync.status == \$s'"
fi

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

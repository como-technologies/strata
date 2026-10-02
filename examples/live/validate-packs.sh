#!/usr/bin/env bash
# The profile validate test (#152): what Palette answers when strata asks whether it takes the
# packs of a profile, and what `snapshot --validate` reports. It builds no cluster, creates no
# profile, and needs no OpenTofu: each request is a POST to a validate route, which changes
# nothing. So it runs whenever the key works.
#
#   examples/live/validate-packs.sh [STAGE]
#
#   1  a candidate whose packs Palette takes: POST /v1/clusterprofiles/validate/packs
#   2  a candidate whose values are not YAML: strata refuses it before any request (#153), and
#      Palette refuses the same body as the whole request; and a body with no pack: Palette refuses
#      the whole request
#   3  a version that Palette holds, as an update of itself: POST
#      /v1/clusterprofiles/{uid}/validate/packs. It needs PROFILE=<name>@<version>, a version in
#      the project of the key, for example one that `just live-up` publishes; with no PROFILE it
#      is skipped.
#
# Results go under runs/<UTC time>/validate-packs/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=validate-packs
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

stage=${1:-all}
registry="Public Repo"
pack=cert-manager
version=1.21.1

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/validate-packs}
mkdir -p "$run" && cd "$run" || exit 2

summary() {
  {
    echo "# profile validate test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`: \`$pack@$version\` in \`$registry\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

# candidate NAME VALUES: an add-on candidate with the one pack, and VALUES when it is not empty
candidate() {
  {
    echo "name: $1"
    echo "version: 1.0.0"
    echo "type: add-on"
    echo "cloudType: all"
    echo "registry: $registry"
    echo "layers:"
    echo "  - name: $pack"
    echo "    version: $version"
    [ -n "$2" ] && echo "    values: $(jq -Rn --arg v "$2" '$v')"
  } > "$1.yaml"
}

if [ "$stage" = all ] || [ "$stage" = 1 ]; then
  echo "== a candidate whose packs Palette takes (#152)" | tee -a steps.log
  candidate strata-validate-barney ""
  check validate-accepts 0 '#152: snapshot --from-candidate --validate writes the record when Palette takes the packs' -- \
    "$strata" snapshot --from-candidate strata-validate-barney.yaml --validate -o barney.json
  check validate-accepts-says 0 '#152: it prints palette accepts' -- \
    grep -qx 'palette    accepts' validate-accepts.err
  check validate-accepts-record 0 '#152: the record holds source.paletteValidation.accepts true' -- \
    jq -e '.source.paletteValidation == {"accepts": true}' barney.json
fi

if [ "$stage" = all ] || [ "$stage" = 2 ]; then
  echo "== a candidate whose values are not YAML, and a body with no pack (#152)" | tee -a steps.log
  candidate strata-validate-fred 'installCRDs: [true'
  check validate-not-yaml 2 '#152, #153: values that are not YAML are candidate-invalid, so strata exits 2 and writes no record' -- \
    "$strata" snapshot --from-candidate strata-validate-fred.yaml --validate -o fred.json
  check validate-not-yaml-says 0 '#153: strata refuses the values before any request, with the code of Palette' -- \
    grep -q "the values of layer 1 ($pack) are not YAML that Palette takes: .*(PackValuesYamlInvalid)$" validate-not-yaml.err
  check validate-not-yaml-no-record 1 '#152: strata wrote no record' -- \
    test -e fred.json
  # the same body, with no retry and no strata between: the HTTP status of the refusal. The record
  # of the candidate with good values gives the uids of the pack.
  candidate strata-validate-barney ""
  "$strata" snapshot --from-candidate strata-validate-barney.yaml -o barney-plain.json 2> /dev/null ||
    die "no record of the candidate strata-validate-barney"
  body=$(jq -c --arg v 'installCRDs: [true' '{cloudType: "all", type: "add-on",
    packs: [.packs[] | {name, tag, uid: .packUid, registryUid, type, layer, values: $v}]}' barney-plain.json)
  check validate-not-yaml-status 2 '#152: Palette sends the refusal as HTTP 500, the status of a server fault' -- \
    "$live/api.sh" post /v1/clusterprofiles/validate/packs "$body"
  check validate-not-yaml-status-says 0 '#152: HTTP 500 with the code PackValuesYamlInvalid' -- \
    grep -q 'answered HTTP 500: {"code":"PackValuesYamlInvalid"' validate-not-yaml-status.err
  # a body with no pack: the converter of the route refuses it before the checks of the packs
  check validate-no-pack 2 '#152: Palette refuses a body with no pack' -- \
    "$live/api.sh" post /v1/clusterprofiles/validate/packs '{"cloudType": "all", "type": "add-on", "packs": []}'
  check validate-no-pack-says 0 '#152: HTTP 400 with the code ShouldNotBeEmpty' -- \
    grep -q 'answered HTTP 400: {"code":"ShouldNotBeEmpty"' validate-no-pack.err
fi

if [ "$stage" = all ] || [ "$stage" = 3 ]; then
  if [ -z "${PROFILE:-}" ]; then
    echo "== stage 3 skipped: give PROFILE=<name>@<version>" | tee -a steps.log
  else
    echo "== a version that Palette holds, as an update of itself (#152)" | tee -a steps.log
    check validate-version 0 "#152: snapshot $PROFILE --validate asks the route of an update, and Palette takes the version as it is" -- \
      "$strata" snapshot "$PROFILE" --validate -o version.json
    check validate-version-says 0 '#152: it prints palette accepts' -- \
      grep -qx 'palette    accepts' validate-version.err
  fi
fi

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

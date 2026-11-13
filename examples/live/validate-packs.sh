#!/usr/bin/env bash
# The profile validate test (#152): what Palette answers when strata asks whether it takes the
# packs of a profile, and what `snapshot --validate` reports. It builds no cluster, and needs no
# OpenTofu. Stages 1 to 3 create no profile: each request is a POST to a validate route, which
# changes nothing. So they run whenever the key works.
#
#   examples/live/validate-packs.sh [STAGE]
#
#   1  a candidate whose packs Palette takes: POST /v1/clusterprofiles/validate/packs
#   2  values that are not YAML, a body with no pack, and a pack with the mark `disabled` of
#      Spectro Cloud (#125): Palette refuses the whole request
#   3  a version that Palette holds, as an update of itself: POST
#      /v1/clusterprofiles/{uid}/validate/packs. It needs PROFILE=<name>@<version>, a version in
#      the project of the key, for example one that `just live-up` publishes; with no PROFILE it
#      is skipped. A candidate with the name and the version of PROFILE: the name route refuses it
#      (#273)
#   4  a pack with another layer than its registry (#210, #254): strata refuses the candidate,
#      and the validate route takes the packs. Then a profile with that layer, made and published
#      through the API, gets the layer of the registry. This stage creates a profile and deletes
#      it, with no cluster and no cost. It runs only when you give its number.
#   5  the name route and the import validate route (#273): GET /v1/clusterprofiles/validate/name
#      and POST /v1/clusterprofiles/import/validate. Each candidate has one fault that the routes
#      refuse: the version, a pack twice, the name of a manifest, or a manifest twice
#   6  the bump of a pin (#166): `strata behind` asks the route of the version, POST
#      /v1/clusterprofiles/{uid}/validate/packs, whether it accepts the move of kubernetes to the
#      newest version, on a cluster that runs the version. It needs PROFILE=<name>@<version>, a
#      version that pins kubernetes two minor versions or more below the newest, and a cluster
#      that runs it, for example the release version of `just live-up` with an older Kubernetes.
#      With CLUSTER=<name>, `behind` asks the same route for the record of that cluster, and the
#      route of the cluster gets a body with a tag that no registry has (#453): Palette keeps the
#      tag of the copy of the cluster, and checks no sent tag. It asks and
#      writes nothing. It runs only when you give its number.
#   7  the update that no validate route checks (#623). It needs CLUSTER=<name>, a cluster of the
#      lab. SKIP=<name>@<version> is a version of a profile that the cluster runs, and it pins
#      Kubernetes two minor versions above the cluster. VARIABLES=<name>@<version> is such a version
#      that adds a required variable with no default value, and moves Kubernetes one minor
#      version at most. For each one that you give, `strata compare --validate` asks the route.
#      Then the update PATCH /v1/spectroclusters/{uid}/profiles puts the version in the place of
#      the version of its profile that the cluster runs. Palette refuses each update, so the cluster
#      does not change. When Palette accepts one, the cluster moves: give a cluster of the lab
#      only. It runs only when you give its number.
#
# Results go under runs/<UTC time>/validate-packs/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=validate-packs
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

stage=${1:-all}
registry=$public_repo
pack=$addon_pack
version=$addon_pack_version

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/validate-packs}
mkdir -p "$run" && cd "$run" || exit 2

summary() {
  write_summary "profile validate test" ": \`$pack@$version\` in \`$registry\`."
}
trap summary EXIT

# candidate NAME VALUES [LAYER]: an add-on candidate with the one pack, and VALUES and LAYER when
# they are not empty
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
    [ -n "${3:-}" ] && echo "    layer: $3"
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
  check validate-not-yaml 2 '#152, #242: Palette refuses the values, so strata exits 2 and writes no record' -- \
    "$strata" snapshot --from-candidate strata-validate-fred.yaml --validate -o fred.json
  check validate-not-yaml-says 0 '#152: the refusal is PackValuesYamlInvalid, of the whole request, with no pack' -- \
    grep -q "^palette    refuses: PackValuesYamlInvalid: Pack '$pack' values yaml is invalid" validate-not-yaml.err
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
  # a body with no pack: Palette refuses it before it checks the packs
  check validate-no-pack 2 '#152: Palette refuses a body with no pack' -- \
    "$live/api.sh" post /v1/clusterprofiles/validate/packs '{"cloudType": "all", "type": "add-on", "packs": []}'
  check validate-no-pack-says 0 '#152: HTTP 400 with the code ShouldNotBeEmpty' -- \
    grep -q 'answered HTTP 400: {"code":"ShouldNotBeEmpty"' validate-no-pack.err
  # a pack with the mark `disabled: "true"` and no system_state: Palette serves it, and refuses
  # a profile that holds it. Strata does not copy the rule, and the route answers it
  {
    echo "name: strata-validate-$marked"
    echo "version: 1.0.0"
    echo "type: add-on"
    echo "cloudType: all"
    echo "registry: $registry"
    echo "layers:"
    echo "  - name: $marked"
    echo "    version: $marked_version"
  } > "strata-validate-$marked.yaml"
  check validate-disabled-mark 2 "#125: Palette refuses a candidate with $marked $marked_version, which carries the mark disabled" -- \
    "$strata" snapshot --from-candidate "strata-validate-$marked.yaml" --validate -o "$marked.json"
  check validate-disabled-mark-says 0 "#125: the refusal is DisabledPacksForbidden, of the whole request, and names the pack" -- \
    grep -q "^palette    refuses: DisabledPacksForbidden: Disabled packs '$marked' are forbidden" validate-disabled-mark.err
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
    echo "== a candidate with the name and the version of $PROFILE (#273)" | tee -a steps.log
    {
      echo "name: ${PROFILE%@*}"
      echo "version: ${PROFILE#*@}"
      echo "type: add-on"
      echo "cloudType: all"
      echo "registry: $registry"
      echo "layers:"
      echo "  - name: $pack"
      echo "    version: $version"
    } > exists.yaml
    check validate-exists 2 "#273: the name route refuses the name and the version of $PROFILE, so strata exits 2" -- \
      "$strata" snapshot --from-candidate exists.yaml --validate -o exists.json
    check validate-exists-says 0 '#273: the refusal is ClusterProfileAlreadyExists' -- \
      grep -q '^palette    refuses: ClusterProfileAlreadyExists: ' validate-exists.err
  fi
fi

if [ "$stage" = 4 ]; then
  echo "== a pack with another layer than its registry (#210, #254)" | tee -a steps.log
  layered=strata-live-layer
  candidate "$layered" ""
  "$strata" snapshot --from-candidate "$layered.yaml" -o layer-candidate.json 2> /dev/null ||
    die "no record of the candidate $layered"
  check layer-registry 0 "#210: the registry gives $pack the layer addon" -- \
    jq -e --arg k "addon/$pack" '[.packs[].key] == [$k]' layer-candidate.json
  candidate strata-validate-wilma "" csi
  check layer-refused 2 '#210: strata refuses a candidate that gives the pack the layer csi' -- \
    "$strata" snapshot --from-candidate strata-validate-wilma.yaml -o wilma.json
  check layer-refused-says 0 '#210: the message names the layer, csi, addon, and the rule of Palette' -- \
    grep -qF "layer 1 ($pack) has the layer csi, and its registry gives the layer addon; Palette takes the layer from the registry" layer-refused.err
  body=$(jq -c '{cloudType: "all", type: "add-on",
    packs: [.packs[] | {name, tag, uid: .packUid, registryUid, type, layer: "csi", values}]}' layer-candidate.json)
  check layer-validate 0 '#254: the validate route takes the pack with the layer csi' -- \
    "$live/api.sh" post /v1/clusterprofiles/validate/packs "$body"
  check layer-validate-says 0 '#254: the answer holds no result for the pack' -- \
    jq -e '.packs.results == []' layer-validate.out
  # the profile gets the values of the candidate, so that the two roots can be equal
  jq -j '.packs[0].values' layer-candidate.json > layer-values.yaml
  "$live/api.sh" delete-named "$layered" > /dev/null
  check layer-create 0 "#254: Palette makes and publishes $layered@1.0.0 with the pack in the layer csi, with no error" -- \
    "$live/api.sh" pin-profile "$layered" "$(jq -r '.packs[0].registryUid' layer-candidate.json)" \
    "$(jq -r '.packs[0].type' layer-candidate.json)" "$pack" "$version" layer-values.yaml csi
  uid=$(cat layer-create.out)
  check layer-profile 0 "#254: read the version $uid" -- "$live/api.sh" get "/v1/clusterprofiles/$uid"
  check layer-published 0 '#254: the published pack has the layer addon of the registry, not csi' -- \
    jq -e --arg p "$pack" '[.spec.published.packs[] | select(.name == $p) | .layer] == ["addon"]' layer-profile.out
  check layer-record 0 "#254: a record of $layered@1.0.0" -- "$strata" snapshot "$uid" -o layer.json
  check layer-record-key 0 '#254: the record holds the pack in the layer addon' -- \
    jq -e --arg k "addon/$pack" '[.packs[].key] == [$k]' layer.json
  check layer-root 0 '#254: the version has the root of the candidate with the layer of the registry' -- \
    "$strata" compare layer-candidate.json layer.json
  check layer-delete 0 "the profile $layered is deleted" -- "$live/api.sh" delete "$uid"
fi

# create_check ID WHAT CODE: snapshot --validate of ID.yaml exits 2, prints the refusal CODE, and
# writes no record
create_check() {
  check "$1" 2 "#273: $2: strata exits 2" -- \
    "$strata" snapshot --from-candidate "$1.yaml" --validate -o "$1.json"
  check "$1-says" 0 "#273: the refusal is $3" -- \
    grep -q "^palette    refuses: $3: " "$1.err"
  check "$1-no-record" 1 '#273: strata wrote no record' -- \
    test -e "$1.json"
}

# layers_yaml NAME VERSION LAYERS: an add-on candidate with the layers LAYERS, in YAML
layers_yaml() {
  printf 'name: %s\nversion: %s\ntype: add-on\ncloudType: all\nregistry: %s\nlayers:\n%s\n' \
    "$1" "$2" "$registry" "$3"
}

if [ "$stage" = all ] || [ "$stage" = 5 ]; then
  echo "== the name route and the import validate route (#273)" | tee -a steps.log
  one="  - name: $pack
    version: $version"
  check validate-name-free 0 '#273: the name route takes a name and a version that the project does not hold, with HTTP 204' -- \
    "$live/api.sh" get "/v1/clusterprofiles/validate/name?name=strata-validate-barney&version=1.0.0"
  check validate-name-short 0 '#273: the name route takes the version 1.1' -- \
    "$live/api.sh" get "/v1/clusterprofiles/validate/name?name=strata-validate-barney&version=1.1"
  check validate-name-version 2 '#273: the name route refuses the version 2.2.2.develop' -- \
    "$live/api.sh" get "/v1/clusterprofiles/validate/name?name=strata-validate-barney&version=2.2.2.develop"
  check validate-name-version-says 0 '#273: HTTP 500 with the code VersionParseFailure' -- \
    grep -q 'answered HTTP 500: {"code":"VersionParseFailure"' validate-name-version.err
  layers_yaml strata-validate-barney 2.2.2.develop "$one" > validate-bad-version.yaml
  create_check validate-bad-version 'Palette refuses the version 2.2.2.develop' VersionParseFailure
  layers_yaml strata-validate-barney 1.0.0 "$one
    values: 'a: 1'
$one
    values: 'a: 2'" > validate-pack-twice.yaml
  create_check validate-pack-twice 'the import validate route refuses a pack twice' DuplicatePacksForbidden
  layers_yaml strata-validate-barney 1.0.0 "$one
$one" > validate-same-pack-twice.yaml
  create_check validate-same-pack-twice 'the API schema refuses two equal packs with HTTP 422' 610
  layers_yaml strata-validate-barney 1.0.0 "$one
    manifests:
      - name: Wilma_1
        content: 'a: 1'" > validate-manifest-name.yaml
  create_check validate-manifest-name 'the import validate route refuses a manifest name that is not a DNS label' InvalidName
  layers_yaml strata-validate-barney 1.0.0 "$one
    manifests:
      - name: wilma
        content: 'a: 1'
      - name: wilma
        content: 'a: 2'" > validate-manifest-twice.yaml
  create_check validate-manifest-twice 'the import validate route refuses a manifest name twice in a pack' DuplicatePackManifestsForbidden
fi

# The jq filter for the pin of kubernetes that is behind, in a `behind --json` report
kubernetes_pin='.pins[] | select(.name == "kubernetes" and .behind)'

# A tag that no registry has, and the jq filter for the body of the route of a cluster, from the
# document of the cluster: each profile by uid, and for the profile that holds kubernetes, its
# packs as the cluster runs them, with the tag $tag in the place of the tag of kubernetes
absent_tag=9.99.9
cluster_body='{profiles: [.spec.clusterProfileTemplates[] | {uid} + (
  if any(.packs[]?; .name == "kubernetes")
  then {packValues: [.packs[] | {name, tag: (if .name == "kubernetes" then $tag else .tag end), type, values}]}
  else {} end)]}'

if [ "$stage" = 6 ]; then
  if [ -z "${PROFILE:-}" ]; then
    echo "== stage 6 skipped: give PROFILE=<name>@<version>" | tee -a steps.log
  else
    echo "== behind asks Palette whether it accepts the bump of $PROFILE (#166)" | tee -a steps.log
    check bump-profile-record 0 "#166: a record of $PROFILE" -- "$strata" snapshot "$PROFILE" -o bump-profile.json
    # exit 1: a pin is behind. The record can pin nothing that is behind, and then the checks below fail
    check bump-behind '?' '#166: behind asks Palette about the bump of each pin that is behind' -- \
      "$strata" behind bump-profile.json --json
    check bump-pair 0 '#166, #623: strata says that the move of kubernetes to the newest version skips a minor version' -- \
      jq -e "$kubernetes_pin | .bump.updateChecks.skippedMinor != null" bump-behind.out
    check bump-asked 0 '#166: the bump of kubernetes holds an answer of Palette' -- \
      jq -e "[$kubernetes_pin | select(.bump.paletteValidation != null)] | length == 1" bump-behind.out
    check bump-refused 0 '#166: Palette refuses the move with DependencyPackUpgradeNotSupported' -- \
      jq -e "$kubernetes_pin | .bump.paletteValidation | .accepts == false and ([.errors[].code] | index(\"DependencyPackUpgradeNotSupported\") != null)" bump-behind.out
    check bump-clusters 0 '#166: the message of the error names a cluster that runs the version' -- \
      jq -e "$kubernetes_pin | .bump.paletteValidation.errors | map(.clusters | length) | max > 0" bump-behind.out
    check bump-text '?' '#166: behind prints the answer under the bump' -- "$strata" behind bump-profile.json
    check bump-text-says 0 '#166: palette refuses, with the code and the clusters' -- \
      grep -Eq '^      palette refuses: DependencyPackUpgradeNotSupported on .+' bump-text.out
    check bump-text-skip 0 '#623: behind says that the move skips a minor version, and that the update refuses it' -- \
      grep -Eq '^      kubernetes .+ skips a minor version: Palette refuses the update with ' bump-text.out
    check bump-no-impact '?' '#166: --no-impact sends no request to a validate route' -- \
      "$strata" behind bump-profile.json --no-impact --json
    check bump-no-impact-says 0 '#166: the report holds no bump, so no answer of Palette' -- \
      jq -e '[.pins[] | select(.bump)] | length == 0' bump-no-impact.out
    if [ -n "${CLUSTER:-}" ]; then
      echo "== the record of the cluster $CLUSTER, and the route of the cluster (#166, #453)" | tee -a steps.log
      check bump-cluster-record 0 "#166: a record of cluster $CLUSTER" -- "$strata" snapshot "cluster:$CLUSTER" -o bump-cluster.json
      check bump-cluster '?' '#453: behind asks the route of the version that the cluster runs about the bump' -- \
        "$strata" behind bump-cluster.json --json
      # The pin of the cluster is the pin of the profile, so the bump is the same: the version, the
      # impact, and the flags. An answer that differs is the answer of the route
      check bump-cluster-same 0 '#419: the bump of kubernetes in the record of the cluster is the bump in the record of the profile' -- \
        jq -e --slurpfile profile bump-behind.out "[$kubernetes_pin | select(.bump != null) | .bump | {to, impact, flags}] == [\$profile[0] | $kubernetes_pin | select(.bump != null) | .bump | {to, impact, flags}]" bump-cluster.out
      check bump-cluster-asked 0 '#166: the bump of kubernetes holds an answer of Palette' -- \
        jq -e "[$kubernetes_pin | select(.bump.paletteValidation != null)] | length == 1" bump-cluster.out
      # For a record of a cluster, behind asks the route of the profile version that the cluster
      # runs, and Palette checks the move on each cluster that runs it, this one too (#453)
      check bump-cluster-refused 0 '#453: the route of the version that the cluster runs refuses the move, on this cluster' -- \
        jq -e --arg c "$CLUSTER" "$kubernetes_pin | .bump.paletteValidation | .accepts == false and ([.errors[] | (.clusters // [])[]] | index(\$c) != null)" bump-cluster.out
      # Palette keeps the tag of the copy that the cluster holds, for a profile that the cluster
      # runs, and checks no sent tag. So the route of the cluster takes a tag that no registry
      # has, and says nothing about the move above. The body names each profile of the cluster by
      # uid. The profile that holds kubernetes has its packs as the cluster runs them, with the
      # tag $absent_tag in the place of the tag of kubernetes
      cluster_uid=$(jq -r '.source.cluster.uid' bump-cluster.json)
      check cluster-route-doc 0 "#453: the document of cluster $CLUSTER" -- \
        "$live/api.sh" get "/v1/spectroclusters/$cluster_uid"
      # The body goes in a file, as one argument cannot hold the values of the packs of a real
      # cluster (#466)
      jq -c --arg tag "$absent_tag" "$cluster_body" cluster-route-doc.out > cluster-route-body.json
      check cluster-route-tag 0 "#453: the route of the cluster answers a body with the tag $absent_tag for kubernetes" -- \
        "$live/api.sh" post-status "/v1/spectroclusters/$cluster_uid/validate/packs" cluster-route-body.json
      check cluster-route-tag-says 0 '#453: the answer is HTTP 200 with no error: the route does not check the sent tag' -- \
        bash -c "head -n 1 cluster-route-tag.out | grep -qx 'HTTP 200' && sed 1d cluster-route-tag.out | jq -e '(.profiles // []) | length == 0'"
    fi
  fi
fi

# move_check ID REF: ask the route about the move of cluster $CLUSTER to the profile version REF,
# then send the update. The version takes the place of the version of its profile that the
# cluster runs. Writes ID.json, ID-validate.out, and ID-update.out
move_check() {
  local id=$1 ref=$2 from to
  check "$id-record" 0 "#623: a record of $ref" -- "$strata" snapshot "$ref" -o "$id.json"
  check "$id-validate" '?' "#623: compare --validate asks the route about the move of cluster $CLUSTER to $ref" -- \
    "$strata" compare "cluster:$CLUSTER" "$ref" --validate --json
  check "$id-asked" 0 '#623: the comparison holds an answer of the route' -- \
    jq -e '.paletteValidation | has("accepts")' "$id-validate.out"
  from=$(jq -r --arg n "${ref%@*}" '[.source.cluster.profiles[] | select(.name == $n) | .uid] | first // empty' move-cluster.json)
  to=$(jq -r '.source.uid' "$id.json")
  check "$id-update" 0 "#623: the update of cluster $CLUSTER from $from to $to" -- \
    "$live/api.sh" replace-status "$cluster_uid" "$from" "$to"
}

if [ "$stage" = 7 ]; then
  if [ -z "${CLUSTER:-}" ] || { [ -z "${SKIP:-}" ] && [ -z "${VARIABLES:-}" ]; }; then
    echo "== stage 7 skipped: give CLUSTER=<name>, and SKIP=<name>@<version> or VARIABLES=<name>@<version>" | tee -a steps.log
  else
    check move-cluster-record 0 "#623: a record of cluster $CLUSTER" -- \
      "$strata" snapshot "cluster:$CLUSTER" -o move-cluster.json
    cluster_uid=$(jq -r '.source.cluster.uid' move-cluster.json)
    if [ -n "${SKIP:-}" ]; then
      echo "== a move of Kubernetes that skips a minor version: $SKIP on $CLUSTER (#623)" | tee -a steps.log
      move_check skip "$SKIP"
      check skip-says 0 '#623: strata says that the move skips a minor version' -- \
        jq -e '.updateChecks.skippedMinor.code != null' skip-validate.out
      # the update refuses the move with the code that strata names
      check skip-refused 0 '#623: Palette refuses the update with HTTP 400 and K8sUpgradeNotSupported' -- \
        bash -c "head -n 1 skip-update.out | grep -qx 'HTTP 400' && sed 1d skip-update.out | jq -e --slurpfile d skip-validate.out '.code == \$d[0].updateChecks.skippedMinor.code'"
    fi
    if [ -n "${VARIABLES:-}" ]; then
      echo "== a move that adds a required variable with no default value: $VARIABLES on $CLUSTER (#623)" | tee -a steps.log
      move_check variables "$VARIABLES"
      check variables-accepts 0 '#623: the route accepts the packs of the move' -- \
        jq -e '.paletteValidation.accepts' variables-validate.out
      check variables-says 0 '#623: strata names the required variables that the move adds' -- \
        jq -e '.updateChecks.requiredVariables.names | length > 0' variables-validate.out
      # the update refuses the move with the code that strata names, and the message names a
      # variable that strata names
      check variables-refused 0 '#623: Palette refuses the update with HTTP 500 and InvalidVariable' -- \
        bash -c "head -n 1 variables-update.out | grep -qx 'HTTP 500' && sed 1d variables-update.out | jq -e --slurpfile d variables-validate.out '\$d[0].updateChecks.requiredVariables as \$r | .code == \$r.code and (.message as \$m | any(\$r.names[]; . as \$v | \$m | contains(\$v)))'"
    fi
  fi
fi

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

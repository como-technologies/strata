#!/usr/bin/env bash
# The writes to Palette that the live example's scripts make outside OpenTofu.
#
#   api.sh edit PROFILE_UID FROM TO
#       Replace the text FROM with TO in the one pack values or manifest that contains it, in a
#       published profile version, in place: PUT the version, keeping each manifest's uid, then
#       publish it. This is an edit
#       through the API, which Palette accepts whatever the Spectro Cloud provider's
#       immutable-clusterprofiles flag says. Exit 2 when no place, or more than one, contains
#       FROM.
#   api.sh draft PROFILE_UID FROM TO
#       The change of api.sh edit, with no publish: PUT the version only. Palette writes the
#       change to the draft of the version, and the published template stays.
#   api.sh publish PROFILE_UID
#       Publish the draft of a version: PATCH /v1/clusterprofiles/{uid}/publish.
#   api.sh create-unpublished PROFILE_UID NAME
#       Create the profile NAME with the version, packs, values, and manifests of a published
#       version, with no publish: POST /v1/clusterprofiles only. Print the uid of the new profile.
#       Each manifest goes with no uid, so the new profile gets its own manifests.
#   api.sh edit-manifest PROFILE_UID FROM TO
#       The same change of one manifest, through the route of that manifest: PUT
#       /v1/clusterprofiles/{uid}/packs/{pack}/manifests/{manifest uid}, then publish. Exit 2
#       unless exactly one manifest holds FROM.
#   api.sh reorder PROFILE_UID
#       Reverse the order of the packs of a published profile version, in place: PUT the version
#       with the same packs, values, and manifests in the other order, then publish it. Nothing
#       else changes.
#   api.sh reorder-cluster CLUSTER_UID
#       PUT the profiles of a cluster in the other order, with the uids it already has and each
#       pack's values and manifests as api.sh apply sends them, and without resolveNotification.
#   api.sh retag PROFILE_UID PACK FROM_TAG TO_TAG
#       Rename the tag of one pack in a published profile version, in place, keeping its pack uid,
#       values, and manifests: PUT the version, then publish it. Exit 2 when the version has no
#       such pack under FROM_TAG.
#   api.sh apply CLUSTER_UID [--keep-notification]
#       Accept the waiting update of every profile a cluster runs: PUT the cluster's profiles with
#       the uids it already has, each pack's values and manifests read from its profile, with
#       resolveNotification=true. As the Spectro Cloud provider does, each manifest keeps the uid of
#       the cluster's own copy, so the content lands in that copy. --keep-notification sends the
#       same PUT without resolveNotification; in the recorded runs Palette changed the cluster's
#       copy either way.
#   api.sh override CLUSTER_UID FROM TO
#       Replace the text FROM with TO in the values and manifests that a cluster runs, as a person
#       who overrides a value on the cluster does: the PUT of the cluster's profiles, with the
#       content of each profile and the one change, and without resolveNotification. The profile
#       keeps what it holds. Exit 2 unless exactly one value or manifest holds FROM.
#   api.sh attach CLUSTER_UID PROFILE_UID
#       The same PUT with PROFILE_UID added, which puts that profile on the cluster.
#   api.sh detach CLUSTER_UID PROFILE_UID
#       The same PUT without PROFILE_UID, which takes that profile off the cluster.
#   api.sh delete PROFILE_UID
#       Delete a profile version; a missing one is not an error.
#   api.sh delete-named NAME...
#       Delete every version of each profile named exactly NAME, found in the profile listing:
#       the versions skip_destroy kept out of the OpenTofu state. A name with no versions is not
#       an error.
#   api.sh delete-cluster CLUSTER_UID [--force]
#       Ask Palette to delete a cluster, by the API and not the provider: DELETE the cluster. The
#       deletion runs after the request ends. --force asks for forceDelete, which removes the
#       records of a cluster stuck in Deleting and no cloud resource: empty the cloud first.
#   api.sh edge-host-create UID PROFILE_UID...
#       Register an edge host UID with no device, as for a device that is not paired yet, and
#       bind the profiles to it. Nothing runs on it.
#   api.sh edge-host-delete UID
#   api.sh filter-create NAME KEY VALUE
#       Make a saved tag filter NAME of the tenant that selects the label KEY=VALUE, and print its
#       uid.
#   api.sh filter-delete UID
#   api.sh app-profile-create NAME [ORDER...]
#       Make an app profile NAME@1.0.0 with one manifest tier, and print its uid. With ORDERs,
#       make one manifest tier for each ORDER in its place: tier1 with the install order of the
#       first ORDER, tier2 with the second, and so on. The content of a tier does not name the
#       app profile, so two app profiles with the same number of ORDERs have the same tiers.
#   api.sh app-profile-edit UID FROM TO
#       Replace FROM with TO in the manifest of the app profile, by a plain update.
#   api.sh app-profile-order UID ORDER
#       Give the first tier of the app profile the install order ORDER, by a plain update of
#       that tier. The update sends the values and the properties of the tier as they are, and
#       no manifest, so the manifests of the tier stay. Then read the tier again: exit 1 when
#       its order is not ORDER. Palette answers the update with 2xx and keeps the order.
#   api.sh app-profile-delete UID
#   api.sh sync-oci REGISTRY_UID
#       Ask Palette to read an OCI pack registry in ECR again, and wait until that sync ends.
#       Prints the sync status. Exit 2 when the sync does not end in five minutes.
#   api.sh sync KIND REGISTRY_UID
#       The same for a registry of KIND: oci-ecr, oci-basic (an OCI registry with basic auth),
#       helm, or pack (a Spectro pack registry).
#   KINDS_PASSWORD=... api.sh pack-registry-create NAME ENDPOINT USER
#       Make a Spectro pack registry over ENDPOINT with basic auth, and print its uid.
#   api.sh pack-registry-delete REGISTRY_UID
#       Remove a Spectro pack registry.
#   api.sh pin-profile NAME REGISTRY_UID TYPE PACK VERSION [VALUES_FILE]
#       Make and publish an add-on profile NAME@1.0.0 that pins PACK at VERSION by name, with no
#       pack uid, as the console does for a registry that Palette does not synchronize, and with
#       the values of VALUES_FILE, or none. Prints its uid.
#   api.sh patch PATH JSON
#       PATCH one document of the API with the body JSON, such as
#       /v1/spectroclusters/<uid>/clusterConfig/timezone '{"timezone": "America/Chicago"}'.
#   api.sh put PATH JSON
#       PUT the body JSON to one document of the API, such as /v1/spectroclusters/<uid>/config/rbacs
#       '{"rbacs": []}'. The body replaces the document.
#   api.sh post PATH JSON
#       POST the body JSON to a search of the API, such as /v1/dashboard/edgehosts/search, or to
#       a validate route, such as /v1/clusterprofiles/validate/packs, and print the answer. A
#       POST to another path can create something: use a search path or a validate path only.
#   api.sh probe-profile NAME REGISTRY_UID VERSION
#       Ask Palette to create an add-on profile that pins one pack version, with the values the
#       registry lists for it. When Palette creates it, delete it at once, print `accepted`, and
#       exit 0. When Palette refuses it, print `refused`, the HTTP status, and the error code, and
#       exit 1. Exit 2 when the registry lists no such version.
#   api.sh macro-set NAME VALUE
#       Give the project macro NAME the value VALUE: create it, or change it when it exists. The
#       project is PALETTE_PROJECT_UID.
#   api.sh macro-delete NAME
#       Delete the project macro NAME; a missing one is not an error.
#   api.sh get-kubeconfig CLUSTER_UID
#       Print the kubeconfig of a cluster, for a caller that writes it to a temporary file and
#       deletes it. It is a credential: do not save it, and do not print it to a log.
#   api.sh get PATH
#       GET one path, `/v1/...`, and print the answer. A read, for a check.
#   api.sh export PROFILE_UID [yaml|json]
#       Print the export file of a profile version: GET /v1/clusterprofiles/{uid}/export. A read.
#       Palette sends the file only to a request that accepts a file; to a request that accepts
#       JSON, it sends `{"Reader":{}}`. Palette can refuse an export that follows another one
#       at once with HTTP 429.
#
# The key comes from PALETTE_API_KEY or ~/.palette.env and is never printed; the address from
# PALETTE_API (default https://api.spectrocloud.com). PALETTE_PROJECT_UID scopes the requests.
set -uo pipefail

die() { echo "api.sh: $*" >&2; exit 2; }
from_env_file() {
  [ -f "$HOME/.palette.env" ] || return 0
  (set -a; . "$HOME/.palette.env" >/dev/null 2>&1; printf %s "${!1:-}")
}
KEY=${PALETTE_API_KEY:-$(from_env_file PALETTE_API_KEY)}
[ -n "$KEY" ] || die "set PALETTE_API_KEY or put it in ~/.palette.env"
API=${PALETTE_API:-$(from_env_file PALETTE_API)}
API=${API:-https://api.spectrocloud.com}
case "$API" in http://*|https://*) ;; *) API="https://$API" ;; esac
API=${API%/}
PROJECT=${PALETTE_PROJECT_UID:-$(from_env_file PALETTE_PROJECT_UID)}

# call METHOD PATH [BODYFILE] -> the response body on stdout; exit 2 unless the status is 2xx
call() {
  local method=$1 path=$2 body=${3:-} out status
  out=$(mktemp)
  status=$(curl -sS -o "$out" -w '%{http_code}' -X "$method" \
    -H "ApiKey: $KEY" -H "Accept: ${ACCEPT:-application/json}" -H 'Content-Type: application/json' \
    ${PROJECT:+-H "ProjectUid: $PROJECT"} ${body:+--data-binary "@$body"} "$API$path")
  if [ "${status:0:1}" != 2 ]; then
    echo "api.sh: $method $path answered HTTP $status: $(head -c 300 "$out")" >&2
    rm -f "$out"
    return 2
  fi
  cat "$out"
  rm -f "$out"
}

# fetch_published UID DIR: DIR/profile, and DIR/contents with each pack's manifests, uid and
# content included. A PUT keeps each manifest uid: a manifest sent without one replaces the old
# manifest, and a cluster that runs a copy of the old one drops that copy when its update is
# accepted.
fetch_published() {
  call GET "/v1/clusterprofiles/$1" > "$2/profile" || exit 2
  call GET "/v1/clusterprofiles/$1/packs/manifests" > "$2/manifests" || exit 2
  jq '[(.spec.packs // [])[] | {key: .spec.name, value: [(.spec.manifests // [])[] | {name, uid, content}]}]
      | from_entries' "$2/manifests" > "$2/contents"
}

# body DIR PACK_FILTER [JQ ARG...]: the PUT body for DIR/profile, each pack passed through
# PACK_FILTER, which sees the pack with its manifests from DIR/contents
body() {
  local dir=$1 filter=$2
  shift 2
  jq "$@" --slurpfile c "$dir/contents" "
    {
      metadata: {name: .metadata.name, uid: .metadata.uid},
      spec: {
        version: .spec.version,
        template: {
          type: .spec.published.type,
          cloudType: .spec.published.cloudType,
          packs: [.spec.published.packs[] | {
            name, tag, type, layer, registryUid, uid: .packUid,
            values: (.values // \"\"),
            manifests: (\$c[0][.name] // [])
          } | $filter]
        }
      }
    }" "$dir/profile"
}

# put_published UID DIR: PUT DIR/body as the published version, then publish it
put_published() {
  call PUT "/v1/clusterprofiles/$1" "$2/body" > /dev/null || exit 2
  call PATCH "/v1/clusterprofiles/$1/publish" > /dev/null || exit 2
}

reorder() {
  local uid=${1:-} tmp
  [ -n "$uid" ] || die "usage: api.sh reorder PROFILE_UID"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  fetch_published "$uid" "$tmp"
  body "$tmp" '.' | jq '.spec.template.packs |= reverse' > "$tmp/body"
  put_published "$uid" "$tmp"
  echo "reversed the packs of $uid in place: $(jq -r '[.spec.template.packs[].name] | join(", ")' "$tmp/body")"
}

reorder_cluster() {
  local cluster=${1:-} tmp
  [ -n "$cluster" ] || die "usage: api.sh reorder-cluster CLUSTER_UID"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  profiles_body "$cluster" "$tmp"
  jq '.profiles |= reverse' "$tmp/body" > "$tmp/reversed" && mv "$tmp/reversed" "$tmp/body"
  call PUT "/v1/spectroclusters/$cluster/profiles" "$tmp/body" > /dev/null || exit 2
  echo "sent the profiles of cluster $cluster in the other order: $(jq -r '[.profiles[].uid] | join(", ")' "$tmp/body")"
}

# edit_body UID FROM TO DIR: DIR/body, the published version UID with the text FROM changed to
# TO in the one pack values or manifest that holds it
edit_body() {
  local uid=$1 from=$2 to=$3 dir=$4 matches
  fetch_published "$uid" "$dir"
  matches=$(jq --arg from "$from" --slurpfile c "$dir/contents" '
      [.spec.published.packs[] | (.values // ""), ($c[0][.name] // [] | .[].content)]
      | map(select(contains($from))) | length' "$dir/profile")
  [ "$matches" = 1 ] || die "$matches values and manifests of $uid contain the text to replace; want exactly 1"
  body "$dir" '.values |= (split($from) | join($to)) | .manifests |= map(.content = ((.content // "") | split($from) | join($to)))' \
    --arg from "$from" --arg to "$to" > "$dir/body"
}

edit() {
  local uid=${1:-} from=${2:-} to=${3:-} tmp
  [ -n "$uid" ] && [ -n "$from" ] || die "usage: api.sh edit PROFILE_UID FROM TO"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  edit_body "$uid" "$from" "$to" "$tmp"
  put_published "$uid" "$tmp"
  echo "edited $uid in place: '$from' -> '$to'"
}

draft() {
  local uid=${1:-} from=${2:-} to=${3:-} tmp
  [ -n "$uid" ] && [ -n "$from" ] || die "usage: api.sh draft PROFILE_UID FROM TO"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  edit_body "$uid" "$from" "$to" "$tmp"
  call PUT "/v1/clusterprofiles/$uid" "$tmp/body" > /dev/null || exit 2
  echo "edited the draft of $uid, with no publish: '$from' -> '$to'"
}

publish() {
  local uid=${1:-}
  [ -n "$uid" ] || die "usage: api.sh publish PROFILE_UID"
  call PATCH "/v1/clusterprofiles/$uid/publish" > /dev/null || exit 2
  echo "published the draft of $uid"
}

create_unpublished() {
  local uid=${1:-} name=${2:-} tmp
  [ -n "$uid" ] && [ -n "$name" ] || die "usage: api.sh create-unpublished PROFILE_UID NAME"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  fetch_published "$uid" "$tmp"
  body "$tmp" '.manifests |= map({name, content})' \
    | jq --arg n "$name" '.metadata = {name: $n}' > "$tmp/body"
  call POST /v1/clusterprofiles "$tmp/body" | jq -er '.uid' || exit 2
}

edit_manifest() {
  local uid=${1:-} from=${2:-} to=${3:-} tmp hit pack muid name
  [ -n "$uid" ] && [ -n "$from" ] || die "usage: api.sh edit-manifest PROFILE_UID FROM TO"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  fetch_published "$uid" "$tmp"
  hit=$(jq -c --arg from "$from" '[to_entries[] | .key as $p | .value[] | select((.content // "") | contains($from))
      | {pack: $p, uid, name, content}]' "$tmp/contents")
  [ "$(jq length <<<"$hit")" = 1 ] || die "$(jq length <<<"$hit") manifests of $uid contain the text to replace; want exactly 1"
  pack=$(jq -r '.[0].pack' <<<"$hit"); muid=$(jq -r '.[0].uid' <<<"$hit"); name=$(jq -r '.[0].name' <<<"$hit")
  jq --arg from "$from" --arg to "$to" '.[0] | {name, content: (.content | split($from) | join($to))}' <<<"$hit" > "$tmp/manifest"
  call PUT "/v1/clusterprofiles/$uid/packs/$pack/manifests/$muid" "$tmp/manifest" > /dev/null || exit 2
  call PATCH "/v1/clusterprofiles/$uid/publish" > /dev/null || exit 2
  echo "edited manifest $name of pack $pack in $uid through its own route: '$from' -> '$to'"
}

retag() {
  local uid=${1:-} pack=${2:-} from=${3:-} to=${4:-} tmp
  [ -n "$to" ] || die "usage: api.sh retag PROFILE_UID PACK FROM_TAG TO_TAG"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  fetch_published "$uid" "$tmp"
  jq -e --arg p "$pack" --arg t "$from" '[.spec.published.packs[] | select(.name == $p and .tag == $t)] | length == 1' \
    "$tmp/profile" > /dev/null || die "no pack $pack with tag $from in $uid"
  body "$tmp" 'if .name == $p and .tag == $from then .tag = $to else . end' \
    --arg p "$pack" --arg from "$from" --arg to "$to" > "$tmp/body"
  put_published "$uid" "$tmp"
  echo "retagged $pack in $uid in place: $from -> $to"
}

# profiles_body CLUSTER DIR [EXCLUDE_UID] [EXTRA_UID] -> DIR/body: the PUT body for the cluster's
# profiles, without EXCLUDE_UID and with EXTRA_UID
profiles_body() {
  local cluster=$1 dir=$2 exclude=${3:-} extra=${4:-} uid
  call GET "/v1/spectroclusters/$cluster" > "$dir/cluster" || exit 2
  call GET "/v1/spectroclusters/$cluster/profiles/packs/manifests" > "$dir/copies" || exit 2
  # the uid of the cluster's own copy of each manifest, by profile uid, pack name, manifest name
  jq '[(.profiles // [])[] | .metadata.uid as $p | (.spec.packs // [])[] | .spec.name as $k
       | (.spec.manifests // [])[] | {key: "\($p)/\($k)/\(.name)", value: .uid}] | from_entries' \
    "$dir/copies" > "$dir/copy-uids"
  : > "$dir/profiles"
  for uid in $(jq -r '.spec.clusterProfileTemplates[].uid' "$dir/cluster") $extra; do
    [ "$uid" = "$exclude" ] && continue
    call GET "/v1/clusterprofiles/$uid" > "$dir/profile" || exit 2
    call GET "/v1/clusterprofiles/$uid/packs/manifests" > "$dir/manifests" || exit 2
    # each pack's current values and manifest contents, as the cluster should run them
    jq -c --slurpfile m "$dir/manifests" --slurpfile copies "$dir/copy-uids" '
      ([($m[0].spec.packs // [])[] | {key: .spec.name, value: (.spec.manifests // [])}] | from_entries) as $ms
      | .metadata.uid as $p
      | {uid: $p, variables: [], packValues: [.spec.published.packs[] | .name as $k | {
          name, tag, type, values: (.values // ""),
          manifests: [($ms[$k] // [])[] | {name, content}
                      + (($copies[0]["\($p)/\($k)/\(.name)"] // null) | if . then {uid: .} else {} end)]
        }]}' "$dir/profile" >> "$dir/profiles"
  done
  jq -s '{profiles: .}' "$dir/profiles" > "$dir/body"
}

apply() {
  local cluster=${1:-} keep=${2:-} tmp query=?resolveNotification=true
  [ -n "$cluster" ] || die "usage: api.sh apply CLUSTER_UID [--keep-notification]"
  case "$keep" in
    "") ;;
    --keep-notification) query="" ;;
    *) die "usage: api.sh apply CLUSTER_UID [--keep-notification]" ;;
  esac
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  profiles_body "$cluster" "$tmp"
  call PUT "/v1/spectroclusters/$cluster/profiles$query" "$tmp/body" > /dev/null || exit 2
  echo "applied the current content of $(jq -r '[.profiles[].uid] | join(", ")' "$tmp/body") to cluster $cluster${query:+, resolving the notification}"
}

override() {
  local cluster=${1:-} from=${2:-} to=${3:-} tmp matches
  [ -n "$cluster" ] && [ -n "$from" ] || die "usage: api.sh override CLUSTER_UID FROM TO"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  profiles_body "$cluster" "$tmp"
  matches=$(jq --arg from "$from" '[.profiles[].packValues[] | .values, (.manifests[].content // "")]
      | map(select(contains($from))) | length' "$tmp/body")
  [ "$matches" = 1 ] || die "$matches values and manifests of cluster $cluster contain the text to replace; want exactly 1"
  jq --arg from "$from" --arg to "$to" '.profiles[].packValues[] |= (
        .values |= (split($from) | join($to))
        | .manifests |= map(.content = ((.content // "") | split($from) | join($to))))' \
    "$tmp/body" > "$tmp/override"
  call PUT "/v1/spectroclusters/$cluster/profiles" "$tmp/override" > /dev/null || exit 2
  echo "overrode '$from' -> '$to' on cluster $cluster, in its own copy"
}

attach() {
  local cluster=${1:-} profile=${2:-} tmp
  [ -n "$profile" ] || die "usage: api.sh attach CLUSTER_UID PROFILE_UID"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  profiles_body "$cluster" "$tmp" "" "$profile"
  call PUT "/v1/spectroclusters/$cluster/profiles?resolveNotification=true" "$tmp/body" > /dev/null || exit 2
  echo "attached profile $profile to cluster $cluster"
}

detach() {
  local cluster=${1:-} profile=${2:-} tmp
  [ -n "$profile" ] || die "usage: api.sh detach CLUSTER_UID PROFILE_UID"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  profiles_body "$cluster" "$tmp" "$profile"
  call PUT "/v1/spectroclusters/$cluster/profiles?resolveNotification=true" "$tmp/body" > /dev/null || exit 2
  echo "detached profile $profile from cluster $cluster"
}

delete() {
  local uid=${1:-} out status
  [ -n "$uid" ] || die "usage: api.sh delete PROFILE_UID"
  out=$(mktemp); trap 'rm -f "${out:-}"' RETURN EXIT
  status=$(curl -sS -o "$out" -w '%{http_code}' -X DELETE -H "ApiKey: $KEY" \
    ${PROJECT:+-H "ProjectUid: $PROJECT"} "$API/v1/clusterprofiles/$uid")
  case "$status" in
    2*) echo "deleted profile $uid" ;;
    404) echo "profile $uid: not found" ;;
    *) die "DELETE /v1/clusterprofiles/$uid answered HTTP $status: $(head -c 300 "$out")" ;;
  esac
}

probe_profile() {
  local name=${1:-} registry=${2:-} version=${3:-} row="" offset=0 page count body out status uid
  [ -n "$version" ] || die "usage: api.sh probe-profile NAME REGISTRY_UID VERSION"
  while :; do
    page=$(call GET "/v1/packs?filters=spec.name=$name&limit=100&offset=$offset") || return 2
    row=$(jq -c --arg r "$registry" --arg v "$version" \
      '[.items[] | select(.spec.registryUid == $r and .spec.version == $v)][0] // empty' <<<"$page")
    count=$(jq '.listmeta.count // 0' <<<"$page")
    [ -n "$row" ] && break
    offset=$((offset + 100))
    [ "$offset" -lt "$count" ] || die "registry $registry lists no $name@$version"
  done
  body=$(mktemp); out=$(mktemp)
  jq '{metadata: {name: "strata-probe-\(.spec.name)"},
       spec: {version: "1.0.0", template: {type: "add-on", cloudType: "all",
         packs: [{name: .spec.name, tag: .spec.version, uid: .metadata.uid,
                  registryUid: .spec.registryUid, type: .spec.type, layer: .spec.layer,
                  values: .spec.values}]}}}' <<<"$row" >"$body"
  status=$(curl -sS -o "$out" -w '%{http_code}' -X POST \
    -H "ApiKey: $KEY" -H 'Accept: application/json' -H 'Content-Type: application/json' \
    ${PROJECT:+-H "ProjectUid: $PROJECT"} --data-binary "@$body" "$API/v1/clusterprofiles")
  rm -f "$body"
  if [ "${status:0:1}" = 2 ]; then
    uid=$(jq -r '.uid' "$out"); rm -f "$out"
    delete "$uid" >/dev/null || return 2
    echo "accepted"
    return 0
  fi
  echo "refused HTTP $status $(jq -r '"\(.code): \(.message)"' "$out" 2>/dev/null || head -c 300 "$out")"
  rm -f "$out"
  return 1
}

delete_named() {
  local name uids uid
  [ $# -gt 0 ] || die "usage: api.sh delete-named NAME..."
  for name in "$@"; do
    uids=$(call GET /v1/dashboard/clusterprofiles/metadata \
      | jq -r --arg n "$name" '(.items // [])[] | select(.metadata.name == $n) | .metadata.uid') \
      || die "listing the profiles failed"
    [ -n "$uids" ] || { echo "$name: no versions"; continue; }
    for uid in $uids; do
      delete "$uid"
    done
  done
}

delete_cluster() {
  local uid=${1:-} force=${2:-} query=""
  [ -n "$uid" ] || die "usage: api.sh delete-cluster CLUSTER_UID [--force]"
  case "$force" in
    "") ;;
    --force) query="?forceDelete=true" ;;
    *) die "usage: api.sh delete-cluster CLUSTER_UID [--force]" ;;
  esac
  call DELETE "/v1/spectroclusters/$uid$query" > /dev/null || return 2
  echo "the deletion of cluster $uid is requested${query:+, forced}"
}

# edge_host_create UID PROFILE_UID...: register an edge host with no device, and bind the
# profiles to it
edge_host_create() {
  local uid=${1:-} body
  [ -n "$uid" ] && [ $# -ge 2 ] || die "usage: api.sh edge-host-create UID PROFILE_UID..."
  shift
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  jq -n --arg u "$uid" '{metadata: {uid: $u, name: $u}, spec: {archType: "amd64"}}' > "$body"
  call POST /v1/edgehosts "$body" > /dev/null || return 2
  jq -n '{profiles: [$ARGS.positional[] | {uid: .}]}' --args "$@" > "$body"
  call PUT "/v1/edgehosts/$uid/profiles" "$body" > /dev/null || return 2
  echo "edge host $uid is registered, with $# profiles"
}

# edge_host_delete UID
edge_host_delete() {
  local uid=${1:-}
  [ -n "$uid" ] || die "usage: api.sh edge-host-delete UID"
  call DELETE "/v1/edgehosts/$uid" > /dev/null || return 2
  echo "edge host $uid is deleted"
}

# filter_create NAME KEY VALUE: a saved tag filter NAME of the tenant that selects the clusters
# with the label KEY=VALUE. Prints its uid.
filter_create() {
  local name=${1:-} key=${2:-} value=${3:-} body
  [ -n "$name" ] && [ -n "$key" ] && [ -n "$value" ] || die "usage: api.sh filter-create NAME KEY VALUE"
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  jq -n --arg n "$name" --arg k "$key" --arg v "$value" '{metadata: {name: $n}, spec: {filterGroup:
      {conjunction: "and", filters: [{key: $k, values: [$v], operator: "eq", negation: false}]}}}' > "$body"
  PROJECT="" call POST /v1/filters/tag "$body" | jq -r '.uid'
}

# filter_delete UID
filter_delete() {
  local uid=${1:-}
  [ -n "$uid" ] || die "usage: api.sh filter-delete UID"
  PROJECT="" call DELETE "/v1/filters/tag/$uid" > /dev/null || return 2
  echo "saved tag filter $uid is deleted"
}

# app_profile_create NAME: an app profile NAME@1.0.0 with one manifest tier, `config`, whose
# manifest is a ConfigMap with `edited: "no"`. Prints its uid.
app_profile_create() {
  local name=${1:-} body o
  [ -n "$name" ] || die "usage: api.sh app-profile-create NAME [ORDER...]"
  shift
  for o in "$@"; do [[ "$o" =~ ^[0-9]+$ ]] || die "usage: api.sh app-profile-create NAME [ORDER...]"; done
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  # tier(NAME; CONFIGMAP): a manifest tier NAME whose ConfigMap is CONFIGMAP
  jq -n --arg n "$name" '
    def tier($t; $c): {name: $t, type: "manifest", manifests: [{name: $t,
      content: ("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: " + $c + "\ndata:\n  edited: \"no\"\n")}]};
    {metadata: {name: $n}, spec: {version: "1.0.0", template: {appTiers:
      (if $ARGS.positional == [] then [tier("config"; $n)]
       else [$ARGS.positional | to_entries[] | tier("tier\(.key + 1)"; "strata-tier\(.key + 1)")
         + {installOrder: (.value | tonumber)}] end)}}}' --args "$@" > "$body"
  call POST /v1/appProfiles "$body" | jq -r '.uid'
}

# app_profile_edit UID FROM TO: replace FROM with TO in the manifest of the first tier that has
# one, by a plain update of that manifest: an app profile has no publish step
app_profile_edit() {
  local uid=${1:-} from=${2:-} to=${3:-} tiers tier m content body
  [ -n "$uid" ] && [ -n "$from" ] || die "usage: api.sh app-profile-edit UID FROM TO"
  tiers=$(call GET "/v1/appProfiles/$uid/tiers") || return 2
  tier=$(jq -r '[.spec.appTiers[] | select((.spec.manifests // []) | length > 0)][0].metadata.uid' <<<"$tiers")
  m=$(jq -r --arg t "$tier" '.spec.appTiers[] | select(.metadata.uid == $t) | .spec.manifests[0].uid' <<<"$tiers")
  content=$(call GET "/v1/appProfiles/$uid/tiers/$tier/manifests/$m" | jq -r '.spec.published.content') || return 2
  grep -qF -- "$from" <<<"$content" || die "the manifest does not hold: $from"
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  jq -n --arg u "$m" --arg c "${content//"$from"/$to}" '{name: "config", uid: $u, content: $c}' > "$body"
  call PUT "/v1/appProfiles/$uid/tiers/$tier/manifests/$m" "$body" > /dev/null || return 2
  echo "edited the manifest of tier $tier of app profile $uid"
}

# app_profile_order UID ORDER: PUT the first tier of the app profile with installOrder ORDER
app_profile_order() {
  local uid=${1:-} order=${2:-} tiers tier body now
  [ -n "$uid" ] && [[ "$order" =~ ^[0-9]+$ ]] || die "usage: api.sh app-profile-order UID ORDER"
  tiers=$(call GET "/v1/appProfiles/$uid/tiers") || return 2
  tier=$(jq -r '.spec.appTiers[0].metadata.uid // empty' <<<"$tiers")
  [ -n "$tier" ] || die "the app profile $uid has no tier"
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  jq --argjson o "$order" '.spec.appTiers[0] | {name: .metadata.name, version: .spec.version,
      values: (.spec.values // ""), installOrder: $o, manifests: [],
      properties: [(.spec.properties // [])[] | {name, value}]}
    + (if .spec.containerRegistryUid then {containerRegistryUid: .spec.containerRegistryUid} else {} end)' \
    <<<"$tiers" > "$body"
  call PUT "/v1/appProfiles/$uid/tiers/$tier" "$body" > /dev/null || return 2
  now=$(call GET "/v1/appProfiles/$uid/tiers" | jq -r --arg t "$tier" \
    '.spec.appTiers[] | select(.metadata.uid == $t) | .spec.installOrder // 0') || return 2
  if [ "$now" != "$order" ]; then
    echo "Palette answered the update, and tier $tier of app profile $uid keeps the install order $now" >&2
    return 1
  fi
  echo "tier $tier of app profile $uid has the install order $order"
}

# app_profile_delete UID
app_profile_delete() {
  local uid=${1:-}
  [ -n "$uid" ] || die "usage: api.sh app-profile-delete UID"
  call DELETE "/v1/appProfiles/$uid" > /dev/null || return 2
  echo "app profile $uid is deleted"
}

# sync_oci UID: sync_registry of an OCI registry on ECR
sync_oci() {
  [ -n "${1:-}" ] || die "usage: api.sh sync-oci REGISTRY_UID"
  sync_registry oci-ecr "$1"
}

# sync_registry KIND UID: POST the sync of a registry of KIND (oci-ecr, oci-basic, helm, or pack),
# then wait until a run newer than the last one has ended
sync_registry() {
  local kind=${1:-} uid=${2:-} base before status after
  case "$kind" in
    oci-ecr) base=/v1/registries/oci/$uid/ecr ;;
    oci-basic) base=/v1/registries/oci/$uid/basic ;;
    helm) base=/v1/registries/helm/$uid ;;
    pack) base=/v1/registries/pack/$uid ;;
    *) die "usage: api.sh sync KIND REGISTRY_UID, where KIND is oci-ecr, oci-basic, helm, or pack" ;;
  esac
  [ -n "$uid" ] || die "usage: api.sh sync KIND REGISTRY_UID"
  before=$(call GET "$base/sync/status" | jq -r '.lastRunTime // ""') || return 2
  call POST "$base/sync" > /dev/null || return 2
  for _ in $(seq 1 60); do
    status=$(call GET "$base/sync/status") || return 2
    after=$(jq -r '.lastRunTime // ""' <<<"$status")
    if [ "$(jq -r '.status // ""' <<<"$status")" != InProgress ] && [ "$after" != "$before" ]; then
      echo "$status"
      return 0
    fi
    sleep 5
  done
  die "the sync of registry $uid did not end in five minutes"
}

# pack_registry_create NAME ENDPOINT USER: a Spectro pack registry over ENDPOINT, with basic auth;
# the password is KINDS_PASSWORD in the environment, so it is on no command line. It is not private:
# Palette synchronizes no private pack registry. Prints the uid.
pack_registry_create() {
  local name=${1:-} endpoint=${2:-} user=${3:-} body
  [ -n "$user" ] || die "usage: KINDS_PASSWORD=... api.sh pack-registry-create NAME ENDPOINT USER"
  [ -n "${KINDS_PASSWORD:-}" ] || die "pack-registry-create needs KINDS_PASSWORD"
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  jq -n --arg n "$name" --arg e "$endpoint" --arg u "$user" --arg p "$KINDS_PASSWORD" \
    '{metadata: {name: $n}, spec: {name: $n, endpoint: $e, private: false,
      auth: {type: "basic", username: $u, password: $p}}}' > "$body"
  PROJECT="" call POST /v1/registries/pack "$body" | jq -r '.uid'
}

# pack_registry_delete UID: remove a Spectro pack registry; one that is gone is not an error
pack_registry_delete() {
  local uid=${1:-}
  [ -n "$uid" ] || die "usage: api.sh pack-registry-delete REGISTRY_UID"
  if PROJECT="" call GET "/v1/registries/pack/$uid" > /dev/null 2>&1; then
    PROJECT="" call DELETE "/v1/registries/pack/$uid" > /dev/null || return 2
    echo "deleted pack registry $uid"
  else
    echo "pack registry $uid: not found"
  fi
}

# pin_profile NAME REGISTRY_UID TYPE PACK VERSION [VALUES_FILE]: create and publish an add-on
# profile NAME@1.0.0 that pins PACK at VERSION by name, with no pack uid, as the console does for a
# registry that Palette does not synchronize, and with the values of VALUES_FILE, or none. When
# the registry lists the pack, the pack uid goes too: Palette needs it for such a registry. Prints
# the uid of the profile.
pin_profile() {
  local name=${1:-} registry=${2:-} kind=${3:-} pack=${4:-} version=${5:-} values=${6:-} body uid pack_uid
  [ -n "$version" ] || die "usage: api.sh pin-profile NAME REGISTRY_UID TYPE PACK VERSION [VALUES_FILE]"
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  # the pack uid, when the registry lists the pack: a synchronized registry needs it
  pack_uid=$(call GET "/v1/packs?filters=spec.name=$pack&limit=100&offset=0" \
    | jq -r --arg r "$registry" --arg v "$version" \
      '[.items[]? | select(.spec.registryUid == $r and .spec.version == $v) | .metadata.uid][0] // ""')
  jq -n --arg n "$name" --arg r "$registry" --arg t "$kind" --arg p "$pack" --arg v "$version" \
    --arg u "$pack_uid" --rawfile values "${values:-/dev/null}" \
    '{metadata: {name: $n}, spec: {version: "1.0.0", template: {type: "add-on", cloudType: "all",
      packs: [{name: $p, tag: $v, registryUid: $r, type: $t, layer: "addon", values: $values}
        + (if $u == "" then {} else {uid: $u} end)]}}}' > "$body"
  uid=$(call POST /v1/clusterprofiles "$body" | jq -r '.uid') || return 2
  call PATCH "/v1/clusterprofiles/$uid/publish" > /dev/null || return 2
  echo "$uid"
}

# patch PATH JSON: PATCH one document of the API with the body JSON
patch() {
  local path=${1:-} json=${2:-} body
  [ -n "$json" ] || die "usage: api.sh patch PATH JSON"
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  jq -e . <<<"$json" > "$body" || die "the body is not JSON: $json"
  call PATCH "$path" "$body" > /dev/null || return 2
  echo "patched $path"
}

# put PATH JSON: PUT the body JSON to one document of the API, which it replaces
put() {
  local path=${1:-} json=${2:-} body
  [ -n "$json" ] || die "usage: api.sh put PATH JSON"
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  jq -e . <<<"$json" > "$body" || die "the body is not JSON: $json"
  call PUT "$path" "$body" > /dev/null || return 2
  echo "put $path"
}

# post PATH JSON: print the answer. For a search that Palette takes as a POST.
post() {
  local path=${1:-} json=${2:-} body
  [ -n "$json" ] || die "usage: api.sh post PATH JSON"
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  jq -e . <<<"$json" > "$body" || die "the body is not JSON: $json"
  call POST "$path" "$body" || return 2
}

# macro-set NAME VALUE: POST creates a project macro, PATCH changes one that exists
macro_set() {
  local name=${1:-} value=${2:-} body
  [ -n "$name" ] && [ $# -eq 2 ] || die "usage: api.sh macro-set NAME VALUE"
  [ -n "$PROJECT" ] || die "macro-set needs PALETTE_PROJECT_UID"
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  jq -n --arg n "$name" --arg v "$value" '{macros: [{name: $n, value: $v}]}' > "$body"
  if call GET "/v1/projects/$PROJECT/macros" 2>/dev/null | jq -e --arg n "$name" '.macros[]? | select(.name == $n)' > /dev/null; then
    call PATCH "/v1/projects/$PROJECT/macros" "$body" > /dev/null || exit 2
  else
    call POST "/v1/projects/$PROJECT/macros" "$body" > /dev/null || exit 2
  fi
  echo "macro $name = $value in project $PROJECT"
}

# get-kubeconfig CLUSTER_UID: Palette gives the kubeconfig for any Accept but JSON
get_kubeconfig() {
  local uid=${1:-} out status
  [ -n "$uid" ] || die "usage: api.sh get-kubeconfig CLUSTER_UID"
  out=$(mktemp); trap 'rm -f "${out:-}"' RETURN
  status=$(curl -sS -o "$out" -w '%{http_code}' -H "ApiKey: $KEY" -H 'Accept: */*' \
    ${PROJECT:+-H "ProjectUid: $PROJECT"} "$API/v1/spectroclusters/$uid/assets/kubeconfig")
  [ "$status" = 200 ] && grep -q '^apiVersion:' "$out" || die "no kubeconfig for $uid: HTTP $status"
  cat "$out"
}

# macro-delete NAME
macro_delete() {
  local name=${1:-} body
  [ -n "$name" ] || die "usage: api.sh macro-delete NAME"
  [ -n "$PROJECT" ] || die "macro-delete needs PALETTE_PROJECT_UID"
  if ! call GET "/v1/projects/$PROJECT/macros" 2>/dev/null | jq -e --arg n "$name" '.macros[]? | select(.name == $n)' > /dev/null; then
    echo "no macro $name in project $PROJECT"
    return 0
  fi
  body=$(mktemp); trap 'rm -f "${body:-}"' RETURN
  jq -n --arg n "$name" '{macros: [{name: $n}]}' > "$body"
  call DELETE "/v1/projects/$PROJECT/macros" "$body" > /dev/null || exit 2
  echo "deleted macro $name in project $PROJECT"
}

case "${1:-}" in
  edit) shift; edit "$@" ;;
  draft) shift; draft "$@" ;;
  publish) shift; publish "$@" ;;
  create-unpublished) shift; create_unpublished "$@" ;;
  reorder) shift; reorder "$@" ;;
  reorder-cluster) shift; reorder_cluster "$@" ;;
  retag) shift; retag "$@" ;;
  edit-manifest) shift; edit_manifest "$@" ;;
  apply) shift; apply "$@" ;;
  attach) shift; attach "$@" ;;
  detach) shift; detach "$@" ;;
  override) shift; override "$@" ;;
  delete) shift; delete "$@" ;;
  delete-named) shift; delete_named "$@" ;;
  delete-cluster) shift; delete_cluster "$@" ;;
  edge-host-create) shift; edge_host_create "$@" ;;
  app-profile-create) shift; app_profile_create "$@" ;;
  filter-create) shift; filter_create "$@" ;;
  filter-delete) shift; filter_delete "$@" ;;
  app-profile-edit) shift; app_profile_edit "$@" ;;
  app-profile-order) shift; app_profile_order "$@" ;;
  app-profile-delete) shift; app_profile_delete "$@" ;;
  edge-host-delete) shift; edge_host_delete "$@" ;;
  sync-oci) shift; sync_oci "$@" ;;
  sync) shift; sync_registry "$@" ;;
  pack-registry-create) shift; pack_registry_create "$@" ;;
  pack-registry-delete) shift; pack_registry_delete "$@" ;;
  pin-profile) shift; pin_profile "$@" ;;
  probe-profile) shift; probe_profile "$@" ;;
  macro-set) shift; macro_set "$@" ;;
  macro-delete) shift; macro_delete "$@" ;;
  get-kubeconfig) shift; get_kubeconfig "$@" ;;
  get) shift; [ -n "${1:-}" ] || die "usage: api.sh get PATH"; call GET "$1" ;;
  export) shift; [ -n "${1:-}" ] || die "usage: api.sh export PROFILE_UID [yaml|json]"
    ACCEPT=application/octet-stream call GET "/v1/clusterprofiles/$1/export?format=${2:-yaml}" ;;
  patch) shift; patch "$@" ;;
  put) shift; put "$@" ;;
  post) shift; post "$@" ;;
  *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac

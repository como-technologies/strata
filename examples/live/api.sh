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
#   api.sh detach CLUSTER_UID PROFILE_UID
#       The same PUT without PROFILE_UID, which takes that profile off the cluster.
#   api.sh delete PROFILE_UID
#       Delete a profile version; a missing one is not an error.
#   api.sh delete-named NAME...
#       Delete every version of each profile named exactly NAME, found in the profile listing:
#       the versions skip_destroy kept out of the OpenTofu state. A name with no versions is not
#       an error.
#   api.sh delete-cluster CLUSTER_UID
#       Ask Palette to delete a cluster, by the API and not the provider: DELETE the cluster. The
#       deletion runs after the request ends.
#   api.sh sync-oci REGISTRY_UID
#       Ask Palette to read an OCI pack registry in ECR again, and wait until that sync ends.
#       Prints the sync status. Exit 2 when the sync does not end in five minutes.
#   api.sh probe-profile NAME REGISTRY_UID VERSION
#       Ask Palette to create an add-on profile that pins one pack version, with the values the
#       registry lists for it. When Palette creates it, delete it at once, print `accepted`, and
#       exit 0. When Palette refuses it, print `refused`, the HTTP status, and the error code, and
#       exit 1. Exit 2 when the registry lists no such version.
#   api.sh get PATH
#       GET one path, `/v1/...`, and print the answer. A read, for a check.
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
    -H "ApiKey: $KEY" -H 'Accept: application/json' -H 'Content-Type: application/json' \
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

edit() {
  local uid=${1:-} from=${2:-} to=${3:-} tmp matches
  [ -n "$uid" ] && [ -n "$from" ] || die "usage: api.sh edit PROFILE_UID FROM TO"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  fetch_published "$uid" "$tmp"
  matches=$(jq --arg from "$from" --slurpfile c "$tmp/contents" '
      [.spec.published.packs[] | (.values // ""), ($c[0][.name] // [] | .[].content)]
      | map(select(contains($from))) | length' "$tmp/profile")
  [ "$matches" = 1 ] || die "$matches values and manifests of $uid contain the text to replace; want exactly 1"
  body "$tmp" '.values |= (split($from) | join($to)) | .manifests |= map(.content = ((.content // "") | split($from) | join($to)))' \
    --arg from "$from" --arg to "$to" > "$tmp/body"
  put_published "$uid" "$tmp"
  echo "edited $uid in place: '$from' -> '$to'"
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

# profiles_body CLUSTER DIR [EXCLUDE_UID] -> DIR/body: the PUT body for the cluster's profiles
profiles_body() {
  local cluster=$1 dir=$2 exclude=${3:-} uid
  call GET "/v1/spectroclusters/$cluster" > "$dir/cluster" || exit 2
  call GET "/v1/spectroclusters/$cluster/profiles/packs/manifests" > "$dir/copies" || exit 2
  # the uid of the cluster's own copy of each manifest, by profile uid, pack name, manifest name
  jq '[(.profiles // [])[] | .metadata.uid as $p | (.spec.packs // [])[] | .spec.name as $k
       | (.spec.manifests // [])[] | {key: "\($p)/\($k)/\(.name)", value: .uid}] | from_entries' \
    "$dir/copies" > "$dir/copy-uids"
  : > "$dir/profiles"
  for uid in $(jq -r '.spec.clusterProfileTemplates[].uid' "$dir/cluster"); do
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
  local uid=${1:-}
  [ -n "$uid" ] || die "usage: api.sh delete-cluster CLUSTER_UID"
  call DELETE "/v1/spectroclusters/$uid" > /dev/null || return 2
  echo "the deletion of cluster $uid is requested"
}

# sync_oci UID: POST the sync, then wait until a run newer than the last one has ended
sync_oci() {
  local uid=${1:-} before status after
  [ -n "$uid" ] || die "usage: api.sh sync-oci REGISTRY_UID"
  before=$(call GET "/v1/registries/oci/$uid/ecr/sync/status" | jq -r '.lastRunTime // ""') || return 2
  call POST "/v1/registries/oci/$uid/ecr/sync" > /dev/null || return 2
  for _ in $(seq 1 60); do
    status=$(call GET "/v1/registries/oci/$uid/ecr/sync/status") || return 2
    after=$(jq -r '.lastRunTime // ""' <<<"$status")
    if [ "$(jq -r '.status // ""' <<<"$status")" != InProgress ] && [ "$after" != "$before" ]; then
      echo "$status"
      return 0
    fi
    sleep 5
  done
  die "the sync of registry $uid did not end in five minutes"
}

case "${1:-}" in
  edit) shift; edit "$@" ;;
  reorder) shift; reorder "$@" ;;
  reorder-cluster) shift; reorder_cluster "$@" ;;
  retag) shift; retag "$@" ;;
  apply) shift; apply "$@" ;;
  detach) shift; detach "$@" ;;
  delete) shift; delete "$@" ;;
  delete-named) shift; delete_named "$@" ;;
  delete-cluster) shift; delete_cluster "$@" ;;
  sync-oci) shift; sync_oci "$@" ;;
  probe-profile) shift; probe_profile "$@" ;;
  get) shift; [ -n "${1:-}" ] || die "usage: api.sh get PATH"; call GET "$1" ;;
  *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac

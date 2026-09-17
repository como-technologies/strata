#!/usr/bin/env bash
# The writes to Palette that examples/live/in-place.sh makes outside Terraform.
#
#   api.sh edit PROFILE_UID FROM TO
#       Replace the text FROM with TO in the one pack values or manifest that contains it, in a
#       published profile version, in place: PUT the version, keeping each manifest's uid, then
#       publish it. This is an edit
#       through the API, which Palette accepts whatever the Terraform provider's
#       immutable-clusterprofiles flag says. Exit 2 when no place, or more than one, contains
#       FROM.
#   api.sh apply CLUSTER_UID [--keep-notification]
#       Accept the waiting update of every profile a cluster runs: PUT the cluster's profiles with
#       the uids it already has, each pack's values and manifests read from its profile, with
#       resolveNotification=true. As the Terraform provider does, each manifest keeps the uid of
#       the cluster's own copy, so the content lands in that copy. --keep-notification sends the
#       same PUT without resolveNotification; in the recorded runs Palette changed the cluster's
#       copy either way.
#   api.sh detach CLUSTER_UID PROFILE_UID
#       The same PUT without PROFILE_UID, which takes that profile off the cluster.
#   api.sh delete PROFILE_UID
#       Delete a profile version; a missing one is not an error.
#   api.sh delete-named NAME...
#       Delete every version of each profile named exactly NAME, found in the profile listing:
#       the versions skip_destroy kept out of Terraform's state. A name with no versions is not
#       an error.
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

edit() {
  local uid=${1:-} from=${2:-} to=${3:-} tmp
  [ -n "$uid" ] && [ -n "$from" ] || die "usage: api.sh edit PROFILE_UID FROM TO"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  call GET "/v1/clusterprofiles/$uid" > "$tmp/profile" || exit 2
  call GET "/v1/clusterprofiles/$uid/packs/manifests" > "$tmp/manifests" || exit 2
  # the manifests with their uid and content, by pack name. The PUT keeps each uid: a manifest
  # sent without one replaces the old manifest, and a cluster that runs a copy of the old one
  # drops that copy when its update is accepted.
  jq '[(.spec.packs // [])[] | {key: .spec.name, value: [(.spec.manifests // [])[] | {name, uid, content}]}]
      | from_entries' "$tmp/manifests" > "$tmp/contents"
  local matches
  matches=$(jq --arg from "$from" --slurpfile c "$tmp/contents" '
      [.spec.published.packs[] | (.values // ""), ($c[0][.name] // [] | .[].content)]
      | map(select(contains($from))) | length' "$tmp/profile")
  [ "$matches" = 1 ] || die "$matches values and manifests of $uid contain the text to replace; want exactly 1"
  jq --arg from "$from" --arg to "$to" --slurpfile c "$tmp/contents" '
    def swap: split($from) | join($to);
    {
      metadata: {name: .metadata.name, uid: .metadata.uid},
      spec: {
        version: .spec.version,
        template: {
          type: .spec.published.type,
          cloudType: .spec.published.cloudType,
          packs: [.spec.published.packs[] | {
            name, tag, type, layer, registryUid, uid: .packUid,
            values: ((.values // "") | swap),
            manifests: [($c[0][.name] // [])[] | {name, uid, content: (.content // "" | swap)}]
          }]
        }
      }
    }' "$tmp/profile" > "$tmp/body"
  call PUT "/v1/clusterprofiles/$uid" "$tmp/body" > /dev/null || exit 2
  call PATCH "/v1/clusterprofiles/$uid/publish" > /dev/null || exit 2
  echo "edited $uid in place: '$from' -> '$to'"
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

case "${1:-}" in
  edit) shift; edit "$@" ;;
  apply) shift; apply "$@" ;;
  detach) shift; detach "$@" ;;
  delete) shift; delete "$@" ;;
  delete-named) shift; delete_named "$@" ;;
  *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac

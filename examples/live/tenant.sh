#!/usr/bin/env bash
# Lookups for the live example. Each lookup reads. But at the repave check, Palette can save the
# address that the console of a pack of virtual machines forwards to.
#
#   tenant.sh names [REGION]   the names terraform.tfvars needs: projects, AWS cloud accounts, and
#                              for each account the availability zones and EC2 key pairs in REGION
#                              (default us-east-2), and GCP cloud accounts with the projects each
#                              can use
#   tenant.sh cluster UID      a cluster's status.state, and while it is neither Running nor
#                              Deleted, each condition not yet done and its three latest events;
#                              exit 0 once it is Deleted or not found, 1 while it exists, 2 when
#                              the answer cannot be trusted
#   tenant.sh updates UID      what Palette lists as waiting for a cluster's profiles, as JSON
#   tenant.sh diagnose UID DIR what holds a cluster that will not reach Running or will not finish
#                              deleting, saved under DIR: the cluster document, its events, and the
#                              conditions not yet done from Palette; with kubectl and a kubeconfig
#                              from Palette, the nodes, machines, pods, Kubernetes events, and the
#                              logs of the Palette and Cluster API controllers in the cluster. The
#                              kubeconfig is used from a temporary file and never saved. What could
#                              not be read is listed in DIR/errors.txt; exit 2 only when Palette
#                              cannot read the cluster
#   tenant.sh repave CLUSTER_UID CURRENT_UID TARGET_UID [PACK FROM TO]
#                              whether Palette would repave the cluster if it ran the profile
#                              version TARGET_UID in place of CURRENT_UID: Palette's own check,
#                              given TARGET_UID's pack values (not its manifests). With PACK FROM
#                              TO, the text FROM in PACK's values is replaced by TO first, as a
#                              control that changes what a node runs. REPAVE_VARIABLES, a JSON
#                              list such as '[{"name": "x", "value": "1"}]', is the variables[]
#                              of the request; the default is none. Prints each reason; exit 0
#                              for no repave, 1 for a repave, 2 when the answer cannot be read.
#                              On a cluster with a pack of virtual machines, Palette can save the
#                              address that the console of the pack forwards to
#                              (docs/src/internals/palette/validate-routes.md)
#   tenant.sh digest KIND REGISTRY_UID NAME VERSION
#                              the digest a helm or oci registry itself serves for one pack
#                              version, read from the registry rather than from Palette; exit 2
#                              when the registry cannot be read
#
# The key comes from PALETTE_API_KEY or ~/.palette.env and is never printed; the address from
# PALETTE_API (default https://api.spectrocloud.com). PALETTE_PROJECT_UID scopes the requests;
# `cluster` needs it, and `examples/live/tf.sh output -raw project_uid` prints it.
set -uo pipefail

live=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
script=tenant.sh
. "$live/common.sh"
. "$live/palette-curl.sh"
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

# get PATH [BODYFILE [ACCEPT]] -> writes the body, prints the HTTP status
get() {
  local body=${2:-/dev/stdout} accept=${3:-application/json}
  palette_curl -sS -o "$body" -w '%{http_code}' \
    -H "Accept: $accept" ${PROJECT:+-H "ProjectUid: $PROJECT"} \
    "$API$1"
}

# listing: the cluster listing itself, GET /v1/dashboard/spectroclusters/meta, as Palette answers
# it. #63 asks whether it already names the profiles each cluster applies.
listing() {
  local tmp status
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' RETURN
  status=$(get /v1/dashboard/spectroclusters/meta "$tmp/body")
  [ "$status" = 200 ] || die "the cluster listing answered HTTP $status"
  cat "$tmp/body"
}

names() {
  local region=${1:-us-east-2} tmp status
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' RETURN
  echo "projects (palette_project):"
  status=$(PROJECT="" get /v1/dashboard/projects/metadata "$tmp/projects")
  [ "$status" = 200 ] || die "listing projects answered HTTP $status"
  jq -r '(.items // [])[] | "  \(.metadata.name)  uid \(.metadata.uid)"' "$tmp/projects"
  echo "AWS cloud accounts (cloud_account_name), with zones (az) and key pairs (ssh_key_name) in $region:"
  status=$(get /v1/cloudaccounts/aws "$tmp/accounts")
  [ "$status" = 200 ] || die "listing AWS cloud accounts answered HTTP $status"
  jq -r '(.items // [])[] | "\(.metadata.uid) \(.metadata.name)"' "$tmp/accounts" | while read -r uid name; do
    echo "  $name"
    status=$(get "/v1/clouds/aws/regions/$region/availabilityzones?cloudAccountUid=$uid" "$tmp/zones")
    if [ "$status" = 200 ]; then
      echo "    zones:     $(jq -r '[(.zones // [])[].name] | join(", ")' "$tmp/zones")"
    else
      echo "    zones:     unavailable (HTTP $status)"
    fi
    status=$(get "/v1/clouds/aws/regions/$region/keypairs?cloudAccountUid=$uid" "$tmp/keys")
    if [ "$status" = 200 ]; then
      echo "    key pairs: $(jq -r '(.keyNames // []) | if length == 0 then "none" else join(", ") end' "$tmp/keys")"
    else
      echo "    key pairs: unavailable (HTTP $status)"
    fi
  done
  echo "GCP cloud accounts (gcp_cloud_account_name), with the projects each can use (gcp_project):"
  status=$(get /v1/cloudaccounts/gcp "$tmp/gcp")
  [ "$status" = 200 ] || die "listing GCP cloud accounts answered HTTP $status"
  jq -r '(.items // [])[] | "\(.metadata.uid) \(.metadata.name)"' "$tmp/gcp" | while read -r uid name; do
    echo "  $name"
    status=$(get "/v1/clouds/gcp/projects?cloudAccountUid=$uid" "$tmp/gcp-projects")
    if [ "$status" = 200 ]; then
      echo "    projects: $(jq -r '[(.projects // [])[].id] | join(", ")' "$tmp/gcp-projects")"
    else
      echo "    projects: unavailable (HTTP $status)"
    fi
  done
  echo "Helm registries (helm_registry_name):"
  status=$(get /v1/registries/metadata "$tmp/registries")
  [ "$status" = 200 ] || die "listing registries answered HTTP $status"
  jq -r '(.items // [])[] | select(.kind == "helm") | "  \(.name)"' "$tmp/registries"
  echo "OCI pack registries (oci_registry_name):"
  status=$(get /v1/registries/oci/summary "$tmp/oci")
  [ "$status" = 200 ] || die "listing OCI registries answered HTTP $status"
  jq -r '(.items // [])[] | select(.spec.providerType == "pack") | "  \(.metadata.name)  \(.spec.endpoint)"' "$tmp/oci"
}

updates() {
  local uid=${1:-} tmp status
  [ -n "$uid" ] || die "usage: tenant.sh updates CLUSTER_UID"
  tmp=$(mktemp); trap 'rm -f "${tmp:-}"' RETURN EXIT
  status=$(get "/v1/spectroclusters/$uid/profileUpdates" "$tmp")
  [ "$status" = 200 ] || die "reading the profile updates of $uid answered HTTP $status"
  jq . "$tmp"
}

repave() {
  local cluster=${1:-} current=${2:-} target=${3:-} pack=${4:-} from=${5:-} to=${6:-} tmp status what
  [ -n "$target" ] || die "usage: tenant.sh repave CLUSTER_UID CURRENT_UID TARGET_UID [PACK FROM TO]"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  status=$(get "/v1/clusterprofiles/$target" "$tmp/profile")
  [ "$status" = 200 ] || die "reading profile $target answered HTTP $status"
  what="cluster $cluster, profile $current -> $target"
  if [ -n "$pack" ]; then
    jq -e --arg p "$pack" --arg from "$from" \
      '[.spec.published.packs[] | select(.name == $p and ((.values // "") | contains($from)))] | length == 1' \
      "$tmp/profile" > /dev/null || die "pack $pack of $target has no values containing the text to replace"
    what="$what with $pack values edited"
  fi
  jq --arg current "$current" --arg p "$pack" --arg from "$from" --arg to "$to" \
    --argjson vars "${REPAVE_VARIABLES:-[]}" '
    {profiles: [{uid: $current, replaceWithProfile: .metadata.uid, variables: $vars,
      packValues: [.spec.published.packs[] | {name, tag, type, values: (.values // ""), manifests: []}
        | if $p != "" and .name == $p then .values |= (split($from) | join($to)) else . end]}]}' \
    "$tmp/profile" > "$tmp/body"
  status=$(palette_curl -sS -o "$tmp/answer" -w '%{http_code}' -X POST \
    -H 'Accept: application/json' -H 'Content-Type: application/json' \
    ${PROJECT:+-H "ProjectUid: $PROJECT"} --data-binary "@$tmp/body" \
    "$API/v1/spectroclusters/$cluster/validate/repave")
  [ "$status" = 200 ] || die "validating the repave of $cluster answered HTTP $status: $(head -c 300 "$tmp/answer")"
  jq -es '.[0] | type == "object" and has("isRepaveRequired")' "$tmp/answer" > /dev/null \
    || die "Palette's answer has no isRepaveRequired"
  if jq -e '.isRepaveRequired' "$tmp/answer" > /dev/null; then
    echo "$what: repave"
    jq -r '(.reasons // [])[] | "  \(.code): \(.message)\(if (.pack.diffConfigKeys // []) == [] then "" else " (\(.pack.diffConfigKeys | join(", ")))" end)"' "$tmp/answer"
    return 1
  fi
  echo "$what: no repave"
}

sha256_of() {
  if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1
}

digest() {
  local kind=${1:-} reg=${2:-} name=${3:-} version=${4:-} tmp status endpoint type path base ref
  [ -n "$version" ] || die "usage: tenant.sh digest helm|oci REGISTRY_UID NAME VERSION"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  case "$kind" in
    helm)
      command -v helm >/dev/null || die "digest helm needs helm"
      status=$(get "$(registry_base helm "$reg")" "$tmp/registry")
      [ "$status" = 200 ] || die "reading Helm registry $reg answered HTTP $status"
      endpoint=$(jq -r '.spec.endpoint // ""' "$tmp/registry")
      [ -n "$endpoint" ] || die "Helm registry $reg has no endpoint"
      [ "$(jq -r '.spec.auth.type // "noAuth"' "$tmp/registry")" = noAuth ] \
        || die "Helm registry $reg needs credentials; digest reads registries that need none"
      # A private Helm home, so the pull neither reads nor changes the user's repositories.
      mkdir -p "$tmp/chart"
      HELM_CACHE_HOME="$tmp/helm" HELM_CONFIG_HOME="$tmp/helm" HELM_DATA_HOME="$tmp/helm" \
        helm pull --repo "$endpoint" "$name" --version "$version" -d "$tmp/chart" >&2 \
        || die "helm pull $name $version from $endpoint failed"
      # A Helm repository index gives the sha256 of the chart archive as the digest, bare hex.
      set -- "$tmp"/chart/*.tgz
      [ -f "$1" ] || die "helm pull $name $version wrote no chart archive"
      sha256_of "$1"
      ;;
    oci)
      command -v crane >/dev/null || die "digest oci needs crane"
      status=$(get /v1/registries/oci/summary "$tmp/summary")
      [ "$status" = 200 ] || die "listing OCI registries answered HTTP $status"
      jq -e --arg r "$reg" '(.items // [])[] | select(.metadata.uid == $r)' "$tmp/summary" > "$tmp/entry" \
        || die "no OCI registry $reg"
      [ "$(jq -r '.spec.providerType' "$tmp/entry")" = pack ] \
        || die "OCI registry $reg holds $(jq -r '.spec.providerType' "$tmp/entry"), not packs; digest reads pack registries"
      endpoint=$(jq -r '.spec.endpoint // ""' "$tmp/entry")
      type=$(jq -r '.spec.registryType // ""' "$tmp/entry")
      path=$(registry_base "oci-$type" "$reg") || die "OCI registry $reg has the type '$type', not ecr or basic"
      status=$(get "$path" "$tmp/registry")
      [ "$status" = 200 ] || die "reading OCI registry $reg answered HTTP $status"
      base=$(jq -r '.spec.baseContentPath // ""' "$tmp/registry")
      endpoint=${endpoint#https://}; endpoint=${endpoint#http://}; endpoint=${endpoint%/}
      base=${base#/}; base=${base%/}
      # Palette reads a pack version from <base path>/spectro-packs/archive/<name>:<version>.
      ref="$endpoint/${base:+$base/}spectro-packs/archive/$name:$version"
      crane digest "$ref" \
        || die "crane digest $ref failed; log in to the registry first (crane auth login or docker login)"
      ;;
    *)
      die "usage: tenant.sh digest helm|oci REGISTRY_UID NAME VERSION" ;;
  esac
}

cluster() {
  local uid=${1:-} tmp status state name
  [ -n "$uid" ] || die "usage: tenant.sh cluster UID"
  [ -n "$PROJECT" ] || die "set PALETTE_PROJECT_UID to the cluster's project"
  tmp=$(mktemp); trap 'rm -f "$tmp" "$tmp.events"' RETURN
  status=$(get "/v1/spectroclusters/$uid" "$tmp")
  case "$status" in
    200)
      state=$(jq -r '.status.state // "unknown"' "$tmp" 2>/dev/null) || die "HTTP 200 without JSON; try again"
      name=$(jq -r '.metadata.name // ""' "$tmp")
      echo "$name $uid: $state"
      # What holds a cluster that is neither Running nor Deleted: each condition not yet true, and
      # the three latest events, which show whether it is still moving.
      case "$state" in
        Running|Deleted) ;;
        *)
          jq -r '(.status.conditions // [])[] | select(.status != "True")
                 | "  not done: \(.type): \(.message // .reason // "")"' "$tmp"
          if [ "$(get "/v1/events/components/spectrocluster/$uid?limit=100" "$tmp.events")" = 200 ]; then
            jq -r '[(.items // [])[]] | sort_by(.metadata.creationTimestamp) | .[-3:][]
                   | "  event \(.metadata.creationTimestamp[11:19]) \(.reason // ""): \((.message // "")[0:120])"' \
              "$tmp.events" 2>/dev/null
          fi
          ;;
      esac
      [ "$state" = Deleted ] && return 0 || return 1 ;;
    404)
      # A real not-found carries a JSON body with code ResourceNotFound. A plain-text 404 comes
      # from a gateway outage, when OpenTofu can report a destroy complete while the cluster runs.
      # An empty body is a gateway problem too: `-s` makes it `[]`, because a jq older than 1.7
      # exits with 0 for `jq -e` on an empty file.
      if jq -es '.[0].code == "ResourceNotFound"' "$tmp" >/dev/null 2>&1; then
        echo "$uid: not found"
        return 0
      fi
      die "HTTP 404 without a ResourceNotFound body: a gateway problem, not proof of deletion; try again" ;;
    *)
      die "HTTP $status; try again" ;;
  esac
}

diagnose() {
  local uid=${1:-} dir=${2:-} tmp status d
  [ -n "$dir" ] || die "usage: tenant.sh diagnose CLUSTER_UID DIR"
  [ -n "$PROJECT" ] || die "set PALETTE_PROJECT_UID to the cluster's project"
  mkdir -p "$dir/logs" || die "cannot write $dir"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  status=$(get "/v1/spectroclusters/$uid" "$dir/cluster.json")
  [ "$status" = 200 ] || die "reading cluster $uid answered HTTP $status"
  ( cluster "$uid" ) > "$dir/state.txt" 2>&1
  : > "$dir/errors.txt"
  # Events, newest first, 100 a page (Palette answers 500 for more), up to 1000.
  local page=1 cont=""
  while :; do
    status=$(get "/v1/events/components/spectrocluster/$uid?limit=100${cont:+&continue=$cont}" "$tmp/events-$page")
    [ "$status" = 200 ] || { echo "events page $page: HTTP $status" >> "$dir/errors.txt"; break; }
    cont=$(jq -r '.listmeta.continue // empty' "$tmp/events-$page")
    [ -n "$cont" ] && [ "$page" -lt 10 ] || break
    page=$((page + 1))
  done
  jq -s '{items: [.[].items // [] | .[]]}' "$tmp"/events-* > "$dir/events.json" 2>/dev/null \
    || echo '{"items":[]}' > "$dir/events.json"
  # Asked for JSON, Palette answers {"Reader":{}}; the kubeconfig comes with any other Accept.
  status=$(get "/v1/spectroclusters/$uid/assets/kubeconfig" "$tmp/kubeconfig" '*/*')
  if [ "$status" != 200 ]; then
    echo "kubeconfig: HTTP $status; nothing was read from inside the cluster" >> "$dir/errors.txt"
  elif ! grep -q '^apiVersion:' "$tmp/kubeconfig"; then
    echo "kubeconfig: Palette answered without one; nothing was read from inside the cluster" >> "$dir/errors.txt"
  elif ! command -v kubectl >/dev/null; then
    echo "kubectl: not installed; nothing was read from inside the cluster" >> "$dir/errors.txt"
  else
    # kubectl OUT ARGS...: one read from inside the cluster, into DIR/OUT
    in_cluster() {
      local out=$1
      shift
      KUBECONFIG="$tmp/kubeconfig" kubectl --request-timeout=30s "$@" > "$dir/$out" 2>&1 \
        || echo "kubectl $*: exit $?" >> "$dir/errors.txt"
    }
    in_cluster nodes.txt get nodes -o wide
    in_cluster machines.txt get machinedeployments,machinesets,machines -A -o wide
    in_cluster pods.txt get pods -A -o wide
    in_cluster kube-events.txt get events -A --sort-by=.lastTimestamp
    # A cluster Palette deploys runs its own Palette and Cluster API controllers, in cluster-<uid>.
    for d in palette-controller-manager cluster-management-agent capi-controller-manager capa-controller-manager; do
      in_cluster "logs/$d.txt" -n "cluster-$uid" logs "deploy/$d" --all-containers --tail=3000
    done
  fi
  [ -s "$dir/errors.txt" ] || rm -f "$dir/errors.txt"
  echo "diagnostics for $(head -1 "$dir/state.txt") in $dir"
}

case "${1:-}" in
  names) shift; names "$@" ;;
  listing) shift; listing "$@" ;;
  cluster) shift; cluster "$@"; exit $? ;;
  updates) shift; updates "$@" ;;
  diagnose) shift; diagnose "$@" ;;
  repave) shift; repave "$@"; exit $? ;;
  digest) shift; digest "$@" ;;
  *) usage; exit 2 ;;
esac

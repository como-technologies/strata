#!/usr/bin/env bash
# Read-only lookups for the live example.
#
#   tenant.sh names [REGION]   the names terraform.tfvars needs: projects, AWS cloud accounts, and
#                              for each account the availability zones and EC2 key pairs in REGION
#                              (default us-east-2)
#   tenant.sh cluster UID      a cluster's status.state; exit 0 once it is Deleted or not found,
#                              1 while it exists, 2 when the answer cannot be trusted
#   tenant.sh digest KIND REGISTRY_UID NAME VERSION
#                              the digest a helm or oci registry itself serves for one pack
#                              version, read from the registry rather than from Palette; exit 2
#                              when the registry cannot be read
#
# The key comes from PALETTE_API_KEY or ~/.palette.env and is never printed; the address from
# PALETTE_API (default https://api.spectrocloud.com). PALETTE_PROJECT_UID scopes the requests;
# `cluster` needs it, and `examples/live/tf.sh output -raw project_uid` prints it.
set -uo pipefail

die() { echo "tenant.sh: $*" >&2; exit 2; }
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

# get PATH [BODYFILE] -> writes the body, prints the HTTP status
get() {
  local body=${2:-/dev/stdout}
  curl -sS -o "$body" -w '%{http_code}' \
    -H "ApiKey: $KEY" -H 'Accept: application/json' ${PROJECT:+-H "ProjectUid: $PROJECT"} \
    "$API$1"
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
  echo "Helm registries (helm_registry_name):"
  status=$(get /v1/registries/metadata "$tmp/registries")
  [ "$status" = 200 ] || die "listing registries answered HTTP $status"
  jq -r '(.items // [])[] | select(.kind == "helm") | "  \(.name)"' "$tmp/registries"
  echo "OCI pack registries (oci_registry_name):"
  status=$(get /v1/registries/oci/summary "$tmp/oci")
  [ "$status" = 200 ] || die "listing OCI registries answered HTTP $status"
  jq -r '(.items // [])[] | select(.spec.providerType == "pack") | "  \(.metadata.name)  \(.spec.endpoint)"' "$tmp/oci"
}

sha256_of() {
  if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1
}

digest() {
  local kind=${1:-} reg=${2:-} name=${3:-} version=${4:-} tmp status endpoint type base ref
  [ -n "$version" ] || die "usage: tenant.sh digest helm|oci REGISTRY_UID NAME VERSION"
  tmp=$(mktemp -d); trap 'rm -rf "${tmp:-}"' RETURN EXIT
  case "$kind" in
    helm)
      command -v helm >/dev/null || die "digest helm needs helm"
      status=$(get "/v1/registries/helm/$reg" "$tmp/registry")
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
      status=$(get "/v1/registries/oci/$reg/$type" "$tmp/registry")
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
  tmp=$(mktemp); trap 'rm -f "$tmp"' RETURN
  status=$(get "/v1/spectroclusters/$uid" "$tmp")
  case "$status" in
    200)
      state=$(jq -r '.status.state // "unknown"' "$tmp" 2>/dev/null) || die "HTTP 200 without JSON; try again"
      name=$(jq -r '.metadata.name // ""' "$tmp")
      echo "$name $uid: $state"
      [ "$state" = Deleted ] && return 0 || return 1 ;;
    404)
      # A real not-found carries a JSON body with code ResourceNotFound. A plain-text 404 comes
      # from a gateway outage, when Terraform can report a destroy complete while the cluster runs.
      if jq -e '.code == "ResourceNotFound"' "$tmp" >/dev/null 2>&1; then
        echo "$uid: not found"
        return 0
      fi
      die "HTTP 404 without a ResourceNotFound body: a gateway problem, not proof of deletion; try again" ;;
    *)
      die "HTTP $status; try again" ;;
  esac
}

case "${1:-}" in
  names) shift; names "$@" ;;
  cluster) shift; cluster "$@"; exit $? ;;
  digest) shift; digest "$@" ;;
  *) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac

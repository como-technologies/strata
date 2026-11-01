#!/usr/bin/env bash
# Pull real packs from Spectro Cloud's "Public Repo" registry at several versions, save them
# under examples/packs/, synthesize raw-API-shaped cluster profiles from them, and run
# strata over those profiles to produce snapshots and reports.
#
# Read-only against the tenant: GET requests only. Needs PALETTE_API_KEY (or
# SPECTROCLOUD_APIKEY); ~/.palette.env is sourced if present. PALETTE_PROJECT_UID is optional.
#
# Env knobs:
#   PACKS="ubuntu-aws kubernetes ..."   packs to pull (default below)
#   VERSIONS_PER_PACK=3                 newest N versions per pack
#   CLOUD=aws                           keep versions whose cloudTypes include this (or "all")
#   EXAMPLES_DIR=<dir>                  output root (default: this directory)
#   OFFLINE=1                           skip the API; rebuild from packs/*/_entity.json + _summaries.json
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# The built binary (release, then debug); override with STRATA_BIN=<path>.
tool=${STRATA_BIN:-}
if [ -z "$tool" ]; then
  for c in "$here/../target/release/strata" "$here/../target/debug/strata"; do
    [ -x "$c" ] && { tool=$c; break; }
  done
fi
out=${EXAMPLES_DIR:-$here}
API="${PALETTE_API:-https://api.spectrocloud.com}"
PACKS=${PACKS:-"ubuntu-aws kubernetes cni-cilium-oss csi-aws-ebs cert-manager metrics-server"}
VERSIONS_PER_PACK=${VERSIONS_PER_PACK:-3}
CLOUD=${CLOUD:-aws}
INFRA="ubuntu-aws kubernetes cni-cilium-oss csi-aws-ebs"   # varied between profile versions

# oldest_first: the versions on the standard input, one on each line, from the lowest to the highest.
# Only a version of plain numbers counts, such as 1.30.6 or 22.04. A pre-release is below its
# version, which a text order of the shell does not know, so the examples omit it. For these
# versions the order is the order of strata, in src/reference/version.rs: by the numbers. This is
# the one order outside strata, because the script runs offline. The test
# the_examples_fetch_orders_plain_versions_as_strata_does in tests/dev_scripts.rs compares them.
oldest_first() {
  jq -R -s -r '[split("\n")[] | select(test("^[0-9]+(\\.[0-9]+){0,2}$"))] | unique
    | sort_by(split(".") | map(tonumber)) | .[]'
}

die() { echo "fetch-examples: $*" >&2; exit 2; }
log() { echo "fetch-examples: $*" >&2; }
[ -n "$tool" ] && [ -x "$tool" ] || die "no strata binary found; run cargo build --release or set STRATA_BIN=<path>"
log "using $tool"

if [ -z "${OFFLINE:-}" ]; then
  [ -f "$HOME/.palette.env" ] && . "$HOME/.palette.env"
  : "${PALETTE_API_KEY:=${SPECTROCLOUD_APIKEY:-}}"
  [ -n "$PALETTE_API_KEY" ] || die "set PALETTE_API_KEY (or SPECTROCLOUD_APIKEY), optionally in ~/.palette.env; or OFFLINE=1 to rebuild from cached responses"
  export PALETTE_API_KEY
fi

api_get() {
  local -a hdr=(-H "ApiKey: ${PALETTE_API_KEY}" -H 'Accept: application/json')
  [ -n "${PALETTE_PROJECT_UID:-}" ] && hdr+=(-H "ProjectUid: ${PALETTE_PROJECT_UID}")
  curl -sS --fail-with-body "${hdr[@]}" "${API}$1"
}

mkdir -p "$out/packs" "$out/profiles/raw" "$out/clusters/raw" "$out/reports"

# ------------------------------------------------------------------ 1. pull packs
if [ -z "${OFFLINE:-}" ]; then
  reg=${PUBLIC_REGISTRY_UID:-}
  if [ -z "$reg" ]; then
    reg=$(api_get /v1/registries/metadata | jq -r '[.items[] | select(.name == "Public Repo")] | .[0].uid // empty')
    [ -n "$reg" ] || die "no registry named 'Public Repo' visible; set PUBLIC_REGISTRY_UID. Registries: $(api_get /v1/registries/metadata | jq -r '[.items[].name] | join(", ")')"
  fi
  log "Public Repo registry uid: $reg"
  for name in $PACKS; do
    d="$out/packs/$name"; mkdir -p "$d"
    if ! api_get "/v1/packs/${name}/registries/${reg}" > "$d/_entity.raw.json" 2>/dev/null; then
      log "WARN: pack '$name' not found in Public Repo, skipping"; rm -rf "$d"; continue
    fi
    jq 'del(.packValues[]?.readme)' "$d/_entity.raw.json" > "$d/_entity.json"; rm -f "$d/_entity.raw.json"
    # The answer above does not include the disabled and the deleted versions. The states query gives
    # the retired versions, and a 404 when the registry has none.
    if api_get "/v1/packs/${name}/registries/${reg}?states=deprecated,disabled,deleted" > "$d/_entity_retired.raw.json" 2>/dev/null; then
      jq 'del(.packValues[]?.readme)' "$d/_entity_retired.raw.json" > "$d/_entity_retired.json"
    else
      rm -f "$d/_entity_retired.json"
    fi
    rm -f "$d/_entity_retired.raw.json"
    # /v1/packs caps limit at 100; page by offset until listmeta.count is reached
    : > "$d/_summaries.jsonl"; off=0
    while :; do
      page=$(api_get "/v1/packs?filters=spec.name=${name}&limit=100&offset=${off}")
      n=$(jq '.items | length' <<<"$page")
      jq -c --arg reg "$reg" '.items[] | select(.spec.registryUid == $reg)' <<<"$page" >> "$d/_summaries.jsonl"
      total=$(jq '.listmeta.count // 0' <<<"$page"); off=$((off + n))
      [ "$n" -eq 100 ] && [ "$off" -lt "$total" ] || break
    done
    jq -s . "$d/_summaries.jsonl" > "$d/_summaries.json"; rm -f "$d/_summaries.jsonl"
    log "pulled $name: $(jq length "$d/_summaries.json") versions in registry"
  done
fi

# ------------------------------------------------------------------ 2. one file per kept version
# packs/<name>/<version>.json: summary (values, digest, layer, schema, template) merged with the
# tag entity (tag, dependencies) for the same packUid.
for d in "$out"/packs/*/; do
  name=$(basename "$d")
  [ -s "$d/_summaries.json" ] && [ -s "$d/_entity.json" ] || continue
  find "$d" -maxdepth 1 -name '*.json' ! -name '_*' -delete
  jq -r --arg c "$CLOUD" '.[] | select((.spec.cloudTypes // ["all"]) | index($c) or index("all")) | .spec.version' "$d/_summaries.json" \
    | oldest_first | tail -n "$VERSIONS_PER_PACK" \
    | while IFS= read -r ver; do
        jq -n --arg v "$ver" --arg name "$name" \
           --slurpfile s "$d/_summaries.json" --slurpfile e "$d/_entity.json" '
          ($s[0] | map(select(.spec.version == $v)) | .[0]) as $sum
          | ($sum.metadata.uid) as $uid
          | ($e[0].tags // [] | map(select(.packUid == $uid))) as $tags
          | ($e[0].packValues // [] | map(select(.packUid == $uid)) | .[0]) as $pv
          | {
              name: $name,
              displayName: ($sum.spec.displayName // $e[0].displayName // $name),
              layer: ($sum.spec.layer // $e[0].layer),
              type: ($sum.spec.type // "spectro"),
              version: $v,
              tag: (($tags | map(select(.tag == $v)) | .[0].tag) // $tags[0].tag // $v),
              parentTags: ($tags[0].parentTags // []),
              packUid: $uid,
              registryUid: $sum.spec.registryUid,
              digest: ($sum.spec.digest // ""),
              cloudTypes: ($sum.spec.cloudTypes // []),
              eol: ($sum.spec.eol // ""),
              values: ($sum.spec.values // $pv.values // ""),
              schema: ($sum.spec.schema // $pv.schema // []),
              presets: ($sum.spec.presets // $pv.presets // []),
              template: ($sum.spec.template // $pv.template // null),
              dependencies: ($pv.dependencies // []),
              manifests: ($sum.spec.manifests // []),
              source: "GET /v1/packs?filters=spec.name=\($name) and GET /v1/packs/\($name)/registries/\($sum.spec.registryUid), Spectro Cloud Public Repo"
            }' > "$d/$ver.json"
      done
  log "$name -> $(ls "$d" | grep -v '^_' | sed 's/\.json$//' | tr '\n' ' ')"
  # Slim the caches: keep the full record (values, schema, template, presets) only for kept
  # versions, so the directory stays small. OFFLINE rebuilds with the same or fewer versions
  # per pack; asking for more needs a live fetch.
  kept=$(ls "$d" | grep -v '^_' | sed 's/\.json$//' | jq -R . | jq -s .)
  jq --argjson kept "$kept" 'map(if (.spec.version as $v | $kept | index($v)) then . else del(.spec.values, .spec.schema, .spec.template, .spec.presets) end)' \
    "$d/_summaries.json" > "$d/_summaries.tmp" && mv "$d/_summaries.tmp" "$d/_summaries.json"
  keptuids=$(jq -c '[.[] | select(.spec.values != null) | .metadata.uid]' "$d/_summaries.json")
  for e in "$d/_entity.json" "$d/_entity_retired.json"; do
    [ -s "$e" ] || continue
    jq --argjson u "$keptuids" '.packValues |= map(select(.packUid as $p | $u | index($p)))' \
      "$e" > "$e.tmp" && mv "$e.tmp" "$e"
  done
done

# ------------------------------------------------------------------ 3. synthesize profiles
# Real pack content (values, digests, packUids) wrapped in the API shapes strata
# reads: V1ClusterProfile, V1ClusterProfilePacksManifests, V1Variables. The pack ref digest is
# set from the registry pack digest; on a real profile Palette stamps that field itself.
versions_of() { ls "$out/packs/$1" 2>/dev/null | grep -v '^_' | sed 's/\.json$//' | oldest_first; }
pick() { # name which(newest|older) -> path or empty
  local vs; vs=$(versions_of "$1"); [ -n "$vs" ] || return 0
  case $2 in
    newest) echo "$out/packs/$1/$(echo "$vs" | tail -1).json" ;;
    older)  echo "$out/packs/$1/$(echo "$vs" | head -1).json" ;;
  esac
}

write_profile() { # name version uid packfiles... (extra manifests/variables via env EXTRA_MANIFEST_PACK, EXTRA_VARIABLES)
  local pname=$1 pver=$2 puid=$3; shift 3
  local dir="$out/profiles/raw/${pname}@${pver}"; mkdir -p "$dir"
  jq -s '.' "$@" > "$dir/.packs.json"   # real values are too big for --argjson
  jq -n --arg n "$pname" --arg v "$pver" --arg uid "$puid" --slurpfile pk "$dir/.packs.json" '
    ($pk[0]) as $packs |
    {apiVersion: "v1", kind: "ClusterProfile",
     metadata: {name: $n, uid: $uid, labels: {"source": "strata examples"}},
     spec: {version: $v,
            published: {cloudType: "aws", type: "cluster",
                        packs: [$packs[] | {name, layer, type, tag, version, packUid, registryUid,
                                             digest: (if .digest == "" then null else .digest end),
                                             values, manifests: [], params: {}, presets: []}
                                            | with_entries(select(.value != null))]},
            versions: []},
     status: {isPublished: true, inUseClusterUids: []}}' > "$dir/profile.json"
  jq -n --arg n "$pname" --arg uid "$puid" --slurpfile pk "$dir/.packs.json" --arg xm "${EXTRA_MANIFEST_PACK:-}" '
    ($pk[0]) as $packs |
    {metadata: {name: $n, uid: $uid},
     spec: {packs: [$packs[] | {metadata: {name: .name, uid: .packUid},
                                spec: {name, layer, version, digest, registryUid, values,
                                       manifests: (if .name == $xm then [{name: "namespace-labels", uid: "m-example-1", digest: "",
                                                     content: "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: platform\n  labels:\n    pod-security.kubernetes.io/enforce: restricted\n"}]
                                                   else [] end)},
                                status: {}}]}}' > "$dir/manifests.json"
  printf '%s\n' "${EXTRA_VARIABLES:-{\"variables\":[]\}}" > "$dir/variables.json"
  rm -f "$dir/.packs.json"
  echo "$dir"
}

write_cluster() { # name uid profile-raw-dir... -> a cluster that runs those profiles, in order
  local cname=$1 cuid=$2; shift 2
  local dir="$out/clusters/raw/$cname"; mkdir -p "$dir"
  local p profiles=() manifests=()
  for p in "$@"; do profiles+=("$p/profile.json"); manifests+=("$p/manifests.json"); done
  jq -s --arg n "$cname" --arg uid "$cuid" '
    {apiVersion: "v1", kind: "SpectroCluster",
     metadata: {name: $n, uid: $uid},
     spec: {cloudType: "aws",
            clusterProfileTemplates: [.[] | {uid: .metadata.uid, name: .metadata.name,
                                             profileVersion: .spec.version, type: .spec.published.type,
                                             cloudType: .spec.published.cloudType, packs: .spec.published.packs}]},
     status: {state: "Running"}}' "${profiles[@]}" > "$dir/cluster.json"
  jq -s '{profiles: [.[] | {metadata, spec}]}' "${manifests[@]}" > "$dir/manifests.json"
  # a cluster carries each profile's variable definitions with the value it was given
  for p in "$@"; do
    jq -n --slurpfile pr "$p/profile.json" --slurpfile vs "$p/variables.json" \
      '{profileUid: $pr[0].metadata.uid, variables: [$vs[0].variables[] | . + {value: .defaultValue}]}'
  done | jq -s '.' > "$dir/variables.json"
  echo "$dir"
}

collect() { # which -> prints pack file paths, infra at $which, addons at newest
  local f
  for p in $INFRA; do f=$(pick "$p" "$1"); [ -n "$f" ] && echo "$f"; done
  for p in $PACKS; do
    case " $INFRA " in *" $p "*) continue ;; esac
    f=$(pick "$p" newest); [ -n "$f" ] && echo "$f"
  done
}

mapfile -t OLDER < <(collect older)
mapfile -t NEWER < <(collect newest)
[ ${#NEWER[@]} -gt 0 ] || die "no packs available under $out/packs; nothing to build"

rm -rf "$out/profiles/raw"/* "$out/profiles"/*.json "$out/clusters/raw"/* "$out/clusters"/*.json "$out/reports"/*
p1=$(write_profile aws-stack 1.0.0 64e0000000000000000000a1 "${OLDER[@]}")
p2=$(write_profile aws-stack 1.1.0 64e0000000000000000000b2 "${NEWER[@]}")
first_addon=""
for p in $PACKS; do case " $INFRA " in *" $p "*) ;; *) [ -d "$out/packs/$p" ] && { first_addon=$p; break; } ;; esac; done
p3=$(EXTRA_MANIFEST_PACK="${first_addon:-kubernetes}" \
     EXTRA_VARIABLES='{"variables":[{"name":"platform_owner","displayName":"Platform owner","defaultValue":"","format":"string","inputType":"text","required":true,"hidden":false,"immutable":false,"isSensitive":false,"options":[]}]}' \
     write_profile aws-stack 1.1.1 64e0000000000000000000c3 "${NEWER[@]}")
CLUSTER_UID=64e0000000000000000000d1
c1=$(write_cluster workload-a "$CLUSTER_UID" "$p2")   # still on 1.1.0 while 1.1.1 is proposed

# ------------------------------------------------------------------ 4. snapshots and reports
export STRATA_QUIET=1
for d in "$p1" "$p2" "$p3"; do
  # the ref is recorded in the snapshot; keep it relative so the files do not carry a machine path
  "$tool" snapshot --from-raw "$d" "examples/profiles/raw/$(basename "$d")" -o "$out/profiles/$(basename "$d").json"
done
report() { # a b suffix extra-args...
  local a=$1 b=$2 suffix=$3; shift 3
  "$tool" compare "$@" "$out/profiles/$a.json" "$out/profiles/$b.json" > "$out/reports/${a}__vs__${b}${suffix}" || true
}
report aws-stack@1.0.0 aws-stack@1.1.0 .txt
report aws-stack@1.0.0 aws-stack@1.1.0 .json --json
report aws-stack@1.1.0 aws-stack@1.1.1 .txt --full
report aws-stack@1.0.0 aws-stack@1.0.0 .txt
"$tool" snapshot --from-raw "$c1" "cluster:$CLUSTER_UID" -o "$out/clusters/workload-a.json"
cluster_report() { # profile-record cluster
  "$tool" compare "$out/profiles/$1.json" "$out/clusters/$2.json" > "$out/reports/${1}__vs__${2}.txt" || true
}
cluster_report aws-stack@1.1.0 workload-a
cluster_report aws-stack@1.1.1 workload-a

# ------------------------------------------------------------------ 5. index
{
  echo "# Examples index"
  echo
  echo "Generated by fetch-examples.sh from the Spectro Cloud Public Repo registry."
  echo
  echo "## Packs pulled"
  echo
  echo "| pack | layer | version | tag | packUid | digest |"
  echo "|------|-------|---------|-----|---------|--------|"
  for d in "$out"/packs/*/; do
    for v in $(ls "$d" | grep -v '^_' | sed 's/\.json$//' | oldest_first); do
      jq -r '"| \(.name) | \(.layer) | \(.version) | \(.tag) | `\(.packUid)` | `\(.digest | if . == "" then "-" else .[0:19] end)` |"' "$d/$v.json"
    done
  done
  echo
  echo "## Profiles synthesized from those packs"
  echo
  for s in "$out"/profiles/*.json; do
    jq -r '"- `\(.source.name)@\(.source.version)` root `\(.root[0:12])`: " + ([.packs[] | "\(.name) \(.version)"] | join(", "))' "$s"
  done
  echo
  echo "## Clusters synthesized from those profiles"
  echo
  for s in "$out"/clusters/*.json; do
    jq -r '"- `\(.source.cluster.name)` running " + ([.source.cluster.profiles[] | "`\(.name)@\(.version)`"] | join(", ")) + ", root `\(.root[0:12])`"' "$s"
  done
  echo
  echo "## Reports"
  echo
  for r in "$out"/reports/*; do echo "- \`reports/$(basename "$r")\`"; done
} > "$out/INDEX.md"

log "done: $(ls "$out/packs" | wc -l) packs, $(ls "$out"/profiles/*.json | wc -l) profiles, $(ls "$out"/clusters/*.json | wc -l) clusters, $(ls "$out/reports" | wc -l) reports under $out"

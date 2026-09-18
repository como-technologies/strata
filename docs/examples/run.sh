#!/usr/bin/env bash
# Run each strata command that the book shows, and write its output into docs/src/output/. The
# book includes those files, so each output in the book is the output of this build of strata.
#
# The commands talk to docs/examples/palette.py, a local stand-in for the Palette API. Its tenant
# holds the profiles and clusters that `just examples` builds from real registry data. Each
# chapter below builds the tenant that one page needs. Between two commands, a chapter changes
# the tenant in the same way that Palette changes: an edit in place, a version bump, a cluster
# update. The book links each of these changes to the live run that shows Palette doing it.
#
# Each step gives the exit code that the book states. A different exit code stops the script.
# Each time stamp in the output becomes 2026-09-17T09:30:00Z, so an output file changes only when
# the output of strata changes.
#
# Needs: target/release/strata (or STRATA_BIN), the examples that `just examples` builds,
# python3, and jq.
#
#   docs/examples/run.sh           write docs/src/output/
#   docs/examples/run.sh --check   exit 1 when docs/src/output/ is not current
set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
tool=${STRATA_BIN:-$repo/target/release/strata}
dest=$repo/docs/src/output

die() { echo "docs-examples: $*" >&2; exit 2; }

check=""
case "${1:-}" in
  --check) check=1 ;;
  "") ;;
  *) die "usage: run.sh [--check]" ;;
esac

[ -x "$tool" ] || die "no strata binary at $tool. Run: just build"
[ -f "$repo/examples/profiles/raw/aws-stack@1.1.1/profile.json" ] || die "the examples are not built. Run: just examples"
command -v python3 >/dev/null || die "python3 is not installed"
command -v jq >/dev/null || die "jq is not installed"

work=$(mktemp -d)
server=""
cleanup() {
  [ -z "$server" ] || kill "$server" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

tenant=$work/tenant
out=$work/output
mkdir -p "$tenant" "$out" "$work/bin" "$work/home"
ln -s "$tool" "$work/bin/strata"

python3 "$repo/docs/examples/palette.py" "$tenant" "$work/port" &
server=$!
for _ in $(seq 100); do
  [ -s "$work/port" ] && break
  sleep 0.1
done
[ -s "$work/port" ] || die "the stand-in for Palette did not start"

# The commands get this environment and no other: no key, no project, and no ~/.palette.env of
# the person who runs the script.
base_env=(env -i "PATH=$work/bin:$PATH" "HOME=$work/home" "LANG=C.UTF-8"
  "PALETTE_API=http://127.0.0.1:$(cat "$work/port")" "PALETTE_API_KEY=docs-example-key"
  "STRATA_BIN=$work/bin/strata")

# ---- steps ------------------------------------------------------------------------------------

dir=""
chapter_env=()

# chapter NAME [VAR=VALUE]...: an empty tenant, a new working directory, and the environment of
# the commands that follow. The working directory links examples/ and scripts/ of the repository.
chapter() {
  dir=$work/chapters/$1
  shift
  mkdir -p "$dir"
  ln -s "$repo/examples" "$dir/examples"
  ln -s "$repo/scripts" "$dir/scripts"
  chapter_env=("$@")
  # Empty the directory, but keep it: the server holds its path.
  find "$tenant" -mindepth 1 -delete
}

# step NAME EXIT KEEP COMMAND: run COMMAND with bash in the chapter directory, and add "$ COMMAND"
# and the output to NAME.txt. KEEP is out for standard output, or both to keep standard error too.
step() {
  local name=$1 want=$2 keep=$3 cmd=$4 got=0 raw=$work/step.txt
  printf '$ %s\n' "$cmd" > "$raw"
  if [ "$keep" = both ]; then
    (cd "$dir" && "${base_env[@]}" "${chapter_env[@]}" bash -o pipefail -c "$cmd") >> "$raw" 2>&1 || got=$?
  else
    (cd "$dir" && "${base_env[@]}" "${chapter_env[@]}" bash -o pipefail -c "$cmd") >> "$raw" 2> "$work/stderr" || got=$?
  fi
  if [ "$got" != "$want" ]; then
    cat "$raw" "$work/stderr" >&2 2>/dev/null || true
    die "$name: '$cmd' exited with $got, and the book says $want"
  fi
  sed -E -e 's/[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z/2026-09-17T09:30:00Z/g' \
    -e 's/ in [0-9a-f]{7,40}$/ in 8c1d4f2/' "$raw" >> "$out/$name.txt"
}

show() { step "$1" "$2" out "$3"; }
show2() { step "$1" "$2" both "$3"; }

# quiet COMMAND: a step that the book does not show
quiet() {
  (cd "$dir" && "${base_env[@]}" "${chapter_env[@]}" bash -o pipefail -c "$1") > "$work/quiet" 2>&1 ||
    { cat "$work/quiet" >&2; die "the command '$1' failed"; }
}

# ---- the tenant -------------------------------------------------------------------------------

DEV=64f20000000000000000a0d1
PROD=64f20000000000000000a0f1
PUBLIC_REPO=5eecc89d0b150045ae661cef
PALETTE_REGISTRY=64ec000000000000000000a1

# doc PROJECT PATH: the file that answers GET PATH in PROJECT, or at tenant scope for "tenant"
doc() {
  local f=$tenant/$1/${2#/}.json
  mkdir -p "$(dirname "$f")"
  printf '%s\n' "$f"
}

# rewrite FILE FILTER: apply a jq program to FILE in place
rewrite() {
  jq "$2" "$1" > "$1.new"
  mv "$1.new" "$1"
}

# projects NAME=UID...: the projects the key can see
projects() {
  local pair items=()
  for pair in "$@"; do
    items+=("$(jq -cn --arg n "${pair%%=*}" --arg u "${pair#*=}" '{metadata: {name: $n, uid: $u}}')")
    printf '{"items": [%s]}\n' "${items[-1]}" > "$(doc "${pair#*=}" v1/dashboard/projects/metadata)"
  done
  jq -s '{items: .}' <<<"${items[*]}" > "$(doc tenant v1/dashboard/projects/metadata)"
  listings
}

# listings: the profile and cluster listings of each scope, from the documents in the tenant. At
# tenant scope Palette lists the tenant's own profiles, and the clusters of every project.
listings() {
  local scope project profiles clusters all=()
  shopt -s nullglob
  for scope in "$tenant"/*/; do
    project=$(basename "$scope")
    [ "$project" != tenant ] || continue
    profiles=("$scope"v1/clusterprofiles/*.json)
    clusters=("$scope"v1/spectroclusters/*.json)
    all+=("${clusters[@]}")
    jq -s '{items: [.[] | {metadata: {name: .metadata.name, uid: .metadata.uid},
                           spec: {version: .spec.version, cloudType: .spec.published.cloudType}}]}' \
      "${profiles[@]}" < /dev/null > "$(doc "$project" v1/dashboard/clusterprofiles/metadata)"
    jq -s '[.[] | {metadata: {name: .metadata.name, uid: .metadata.uid, annotations: .metadata.annotations},
                   spec: {cloudType: .spec.cloudType}, status: {state: .status.state}}]' \
      "${clusters[@]}" < /dev/null > "$(doc "$project" v1/dashboard/spectroclusters/meta)"
  done
  echo '{"items": []}' > "$(doc tenant v1/dashboard/clusterprofiles/metadata)"
  jq -s '[.[] | {metadata: {name: .metadata.name, uid: .metadata.uid, annotations: .metadata.annotations},
                 spec: {cloudType: .spec.cloudType}, status: {state: .status.state}}]' \
    "${all[@]}" < /dev/null > "$(doc tenant v1/dashboard/spectroclusters/meta)"
  shopt -u nullglob
}

# profile PROJECT UID VERSION EXAMPLE [FILTER]: publish a copy of the example profile EXAMPLE, such
# as aws-stack@1.1.1, in PROJECT under UID and VERSION. FILTER is a jq program for the profile
# document: a changed value or tag.
profile() {
  local project=$1 uid=$2 version=$3 from=$repo/examples/profiles/raw/$4 filter=${5:-.}
  jq --arg uid "$uid" --arg v "$version" \
    ".metadata.uid = \$uid | .spec.version = \$v
     | .status.inUseClusterUids = [] | .status.inUseClusters = [] | $filter" \
    "$from/profile.json" > "$(doc "$project" "v1/clusterprofiles/$uid")"
  jq --arg uid "$uid" '.metadata.uid = $uid' "$from/manifests.json" \
    > "$(doc "$project" "v1/clusterprofiles/$uid/packs/manifests")"
  cp "$from/variables.json" "$(doc "$project" "v1/clusterprofiles/$uid/variables")"
  listings
}

# addons PROJECT UID PACK_UID LABEL: publish platform-addons@1.0.0, an add-on profile with one
# manifest pack. Palette gives the manifest pack of each profile a new pack uid.
addons() {
  local project=$1 uid=$2 pack=$3 label=$4
  local content="apiVersion: v1
kind: Namespace
metadata:
  name: payments
  labels:
    pod-security.kubernetes.io/enforce: $label"
  jq -n --arg uid "$uid" --arg pack "$pack" '
    {apiVersion: "v1", kind: "ClusterProfile",
     metadata: {name: "platform-addons", uid: $uid},
     spec: {version: "1.0.0",
            published: {cloudType: "all", type: "add-on",
                        packs: [{name: "namespace-policy", layer: "addon", type: "manifest",
                                 tag: "1.0.0", version: "1.0.0", packUid: $pack,
                                 manifests: [{kind: "manifest", name: "namespace-policy"}],
                                 presets: [], schema: []}]}},
     status: {isPublished: true, inUseClusterUids: [], inUseClusters: []}}' \
    > "$(doc "$project" "v1/clusterprofiles/$uid")"
  jq -n --arg uid "$uid" --arg pack "$pack" --arg content "$content" '
    {metadata: {name: "platform-addons", uid: $uid},
     spec: {packs: [{metadata: {name: "namespace-policy", uid: $pack},
                     spec: {name: "namespace-policy", layer: "addon", version: "1.0.0",
                            manifests: [{name: "namespace-policy", content: $content}]},
                     status: {}}]}}' \
    > "$(doc "$project" "v1/clusterprofiles/$uid/packs/manifests")"
  echo '{"variables": []}' > "$(doc "$project" "v1/clusterprofiles/$uid/variables")"
  listings
}

# edit PROJECT UID FILTER: change a published profile in place, as an edit through the API does
edit() {
  rewrite "$(doc "$1" "v1/clusterprofiles/$2")" "$3"
  listings
}

# cluster PROJECT UID NAME PROFILE_UID... [-- VARIABLE=VALUE...]: create a cluster that runs the
# profiles in order. The cluster gets a copy of what each profile holds now, so a later edit of a
# profile does not reach the cluster. Run it again for the same cluster to apply the update that
# waits for the cluster.
cluster() {
  local project=$1 uid=$2 name=$3 p kv
  shift 3
  local profiles=() values='{}'
  while [ $# -gt 0 ]; do
    if [ "$1" = -- ]; then
      shift
      for kv in "$@"; do
        values=$(jq -c --arg k "${kv%%=*}" --arg v "${kv#*=}" '. + {($k): $v}' <<<"$values")
      done
      break
    fi
    profiles+=("$1")
    shift
  done
  local docs=() mans=()
  for p in "${profiles[@]}"; do
    docs+=("$(doc "$project" "v1/clusterprofiles/$p")")
    mans+=("$(doc "$project" "v1/clusterprofiles/$p/packs/manifests")")
  done
  jq -s --arg n "$name" --arg uid "$uid" --arg project "$project" '
    {apiVersion: "v1", kind: "SpectroCluster",
     metadata: {name: $n, uid: $uid, annotations: {projectUid: $project, scope: "project"}},
     spec: {cloudType: "aws",
            clusterProfileTemplates: [.[] | {uid: .metadata.uid, name: .metadata.name,
                                             profileVersion: .spec.version,
                                             type: .spec.published.type,
                                             cloudType: .spec.published.cloudType,
                                             packs: .spec.published.packs}]},
     status: {state: "Running"}}' "${docs[@]}" > "$(doc "$project" "v1/spectroclusters/$uid")"
  jq -s '{profiles: [.[] | {metadata, spec}]}' "${mans[@]}" \
    > "$(doc "$project" "v1/spectroclusters/$uid/profiles/packs/manifests")"
  for p in "${profiles[@]}"; do
    jq --arg p "$p" --argjson values "$values" \
      '{profileUid: $p, variables: [.variables[] | . + {value: ($values[.name] // .defaultValue)}]}' \
      "$(doc "$project" "v1/clusterprofiles/$p/variables")"
  done | jq -s . > "$(doc "$project" "v1/spectroclusters/$uid/variables")"
  for p in "${profiles[@]}"; do
    rewrite "$(doc "$project" "v1/clusterprofiles/$p")" "
      .status.inUseClusterUids = ((.status.inUseClusterUids // []) - [\"$uid\"] + [\"$uid\"])
      | .status.inUseClusters = ((.status.inUseClusters // []) | map(select(.uid != \"$uid\")))
          + [{kind: \"spectrocluster\", name: \"$name\", projectUid: \"$project\", uid: \"$uid\"}]"
  done
  listings
}

# on_cluster PROJECT UID FILTER: change a cluster document in place, as an edit of the pack values
# on the cluster does
on_cluster() {
  rewrite "$(doc "$1" "v1/spectroclusters/$2")" "$3"
}

# set_value PROJECT UID VARIABLE VALUE: give a cluster variable a new value
set_value() {
  rewrite "$(doc "$1" "v1/spectroclusters/$2/variables")" \
    "map(.variables |= map(if .name == \"$3\" then .value = \"$4\" else . end))"
}

# registries PROJECT: the Public Repo registry and the versions of each pack in examples/packs/,
# as the registry lookups of a candidate read them
registries() {
  local project=$1 d name count listing
  jq -n --arg uid "$PUBLIC_REPO" --arg oci "$PALETTE_REGISTRY" \
    '{items: [{name: "Public Repo", uid: $uid, kind: "pack"},
              {name: "Palette Registry", uid: $oci, kind: "oci"}]}' \
    > "$(doc "$project" v1/registries/metadata)"
  for d in "$repo"/examples/packs/*/; do
    name=$(basename "$d")
    count=$(jq length "$d/_summaries.json")
    listing=$(doc "$project" "v1/packs@filters=spec.name=$name&limit=100&offset=0")
    jq --argjson n "$count" '{items: ., listmeta: {count: $n}}' "$d/_summaries.json" > "$listing"
    cp "$d/_entity.json" "$(doc "$project" "v1/packs/$name/registries/$PUBLIC_REPO")"
    # The CSI pack is in the OCI registry too, as the live run of 20260917T150025Z found. A pack
    # ref that names no registry then has to say so.
    if [ "$name" = csi-aws-ebs ]; then
      rewrite "$listing" "$(csi_in_oci)"
      jq "$(csi_entity_in_oci)" "$d/_entity.json" \
        > "$(doc "$project" "v1/packs/$name/registries/$PALETTE_REGISTRY")"
    fi
  done
}

# the newest csi-aws-ebs version, copied into the OCI registry with its own uid and digest
csi_in_oci() {
  cat <<'JQ'
    ([.items[] | select(.spec.values != null) | .spec.version]
       | sort_by(split(".") | map(tonumber? // 0)) | last) as $newest
    | .items += [.items[] | select(.spec.version == $newest and .spec.values != null)
        | .metadata.uid = "64ec000000000000000000b1"
        | .spec.registryUid = "64ec000000000000000000a1"
        | .spec.digest = "sha256:a746e14689085b14531566f9c9cfb2bc7716e96a394547ef50a1c52eab510c9e"]
JQ
}

csi_entity_in_oci() {
  printf '%s' '.tags = [{tag: ([.tags[].tag] | map(select(test("^[0-9]+\\.[0-9]+\\.[0-9]+$"))) | sort | last), packUid: "64ec000000000000000000b1"}]'
}

# jq programs for a profile document
pack_values() { printf '(.spec.published.packs[] | select(.name == "%s") | .values)' "$1"; }
RETAIN="$(pack_values csi-aws-ebs) |= sub(\"reclaimPolicy: Delete\"; \"reclaimPolicy: Retain\")"
TAG_2404='(.spec.published.packs[] | select(.name == "ubuntu-aws") | .tag) = "24.04"'
TAG_LTS2404='(.spec.published.packs[] | select(.name == "ubuntu-aws") | .tag) = "LTS__24.04"'
# The CSI pack from the OCI registry "Palette Registry", as run 20260917T150025Z recorded it
OCI_CSI='(.spec.published.packs[] | select(.name == "csi-aws-ebs")) |= (.type = "oci"
  | .registryUid = "64ec000000000000000000a1" | .packUid = "64ec000000000000000000b1"
  | .digest = "sha256:a746e14689085b14531566f9c9cfb2bc7716e96a394547ef50a1c52eab510c9e")'

# the profile uids of the examples, in dev
A100=64e0000000000000000000a1
A110=64e0000000000000000000b2
A111=64e0000000000000000000c3
# other profile and cluster uids
A111_PROD=64e1000000000000000000c3
A111_PROD2=64e1000000000000000000c4
A112=64e0000000000000000000d4
A120=64e0000000000000000000e5
ADDONS_DEV=64e2000000000000000000a1
ADDONS_PROD=64e3000000000000000000a1
PAYMENTS=64f1000000000000000000e1
SEARCH=64f1000000000000000000e2
LEDGER=64f1000000000000000000e3
PAYMENTS_PROD=64f1000000000000000000f1

# ---- chapters: getting started ----------------------------------------------------------------

chapter help
show help 0 'strata --help'
for c in list snapshot candidate compare same-release verify consumers schema; do
  show "help-$c" 0 "strata $c --help"
done

chapter quickstart PALETTE_PROJECT=dev
projects dev=$DEV prod=$PROD
profile $DEV $A100 1.0.0 aws-stack@1.0.0
profile $DEV $A110 1.1.0 aws-stack@1.1.0
profile $DEV $A111 1.1.1 aws-stack@1.1.1
cluster $DEV $PAYMENTS payments $A110
show quickstart-offline 1 'strata compare examples/profiles/aws-stack@1.1.0.json examples/profiles/aws-stack@1.1.1.json'
show quickstart-list 0 'strata list'
show2 quickstart-snapshot 0 'strata snapshot aws-stack@1.1.0 -o aws-stack-1.1.0.json'
show quickstart-verify 0 'strata verify aws-stack-1.1.0.json'
show quickstart-cluster 0 'strata compare aws-stack-1.1.0.json cluster:payments'
show quickstart-versions 1 'strata compare aws-stack@1.1.0 aws-stack@1.1.1'

# ---- chapters: commands -----------------------------------------------------------------------

chapter list PALETTE_PROJECT=dev
projects dev=$DEV prod=$PROD
profile $DEV $A100 1.0.0 aws-stack@1.0.0
profile $DEV $A110 1.1.0 aws-stack@1.1.0
profile $DEV $A111 1.1.1 aws-stack@1.1.1
profile $PROD $A111_PROD 1.1.1 aws-stack@1.1.1
cluster $DEV $PAYMENTS payments $A110
cluster $DEV $SEARCH search $A111
cluster $PROD $PAYMENTS_PROD payments $A111_PROD
show list 0 'strata list'
show list-projects 0 'strata list projects'
show list-prod 0 'strata list --project-name prod'
show list-tenant 0 "strata list clusters --project ''"
show list-pack 0 'strata list --pack cilium'
show list-pack-all 0 'for p in dev prod; do strata list --pack cilium --project-name "$p"; done'
show list-pack-consumers 1 'for v in 1.0.0 1.1.0 1.1.1; do strata consumers aws-stack@$v; done'
show list-pack-consumers 1 'PALETTE_PROJECT=prod strata consumers aws-stack@1.1.1'
show list-pack-json 0 "strata list --pack cilium --json | jq -r '.profiles[] | .ref as \$ref | .packs[] | select(.name | contains(\"cilium\")) | \"\(\$ref) \(.name) \(.version)\"'"

chapter snapshot PALETTE_PROJECT=dev
projects dev=$DEV
profile $DEV $A110 1.1.0 aws-stack@1.1.0
profile $DEV $A111 1.1.1 aws-stack@1.1.1
cluster $DEV $PAYMENTS payments $A111
quiet 'mkdir -p releases/1.1.1 runs'
show2 snapshot-profile 0 'strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json'
show snapshot-record 0 "jq '{root, source: (.source | {ref, uid, projectUid, name, version, template, fetchedAt}), packs: [.packs[] | {key, version, node: .hash.node}]}' releases/1.1.1/dev.json"
show2 snapshot-latest 0 'strata snapshot aws-stack@latest -o latest.json'
show snapshot-latest-version 0 "jq -r '.source | \"\(.name)@\(.version)\"' latest.json"
show2 snapshot-cluster 0 'strata snapshot cluster:payments -o runs/payments.json'
show snapshot-cluster-source 0 'jq .source.cluster runs/payments.json'
show2 snapshot-raw 0 'strata snapshot aws-stack@1.1.1 --raw-dir raw/aws-stack-1.1.1 -o online.json'
show snapshot-raw 0 'ls raw/aws-stack-1.1.1'
show2 snapshot-raw 0 'strata snapshot --from-raw raw/aws-stack-1.1.1 -o offline.json'
show snapshot-raw 0 'jq -r .root online.json offline.json'

chapter candidate PALETTE_PROJECT=dev
projects dev=$DEV
registries $DEV
profile $DEV $A111 1.1.1 aws-stack@1.1.1
show2 candidate-write 0 'strata candidate aws-stack@1.1.1 -o aws-stack.yaml --files-dir aws-stack'
show candidate-yaml 0 'cat aws-stack.yaml'
show candidate-files 0 'find aws-stack -type f | sort'
show2 candidate-snapshot 0 'strata snapshot --from-candidate aws-stack.yaml -o candidate.json'
show2 candidate-published 0 'strata snapshot aws-stack@1.1.1 -o published.json'
show candidate-same 0 'strata same-release candidate.json published.json'

chapter compare PALETTE_PROJECT=dev
projects dev=$DEV
profile $DEV $A100 1.0.0 aws-stack@1.0.0
profile $DEV $A110 1.1.0 aws-stack@1.1.0
profile $DEV $A111 1.1.1 aws-stack@1.1.1
profile $DEV $A112 1.1.2 aws-stack@1.1.1 "$RETAIN"
show compare-identical 0 'strata compare aws-stack@1.1.1 aws-stack@1.1.1'
show compare-repave 1 'strata compare aws-stack@1.0.0 aws-stack@1.1.0'
show compare-values 1 'strata compare aws-stack@1.1.1 aws-stack@1.1.2'
show compare-full 1 'strata compare aws-stack@1.1.1 aws-stack@1.1.2 --full'
show compare-markdown 1 'strata compare aws-stack@1.0.0 aws-stack@1.1.0 --format markdown'
show compare-json 1 'strata compare aws-stack@1.0.0 aws-stack@1.1.0 --json | jq .summary'

chapter schema
show schema-diff 0 "strata schema diff | jq -r '.required | join(\", \")'"

# ---- chapters: use cases ----------------------------------------------------------------------

chapter review PALETTE_PROJECT=dev
projects dev=$DEV
registries $DEV
profile $DEV $A100 1.0.0 aws-stack@1.0.0
quiet 'mkdir -p releases/1.0.0 releases/1.1.0 && strata snapshot aws-stack@1.0.0 -o releases/1.0.0/dev.json'
cat > "$dir/aws-stack.yaml" <<'YAML'
name: aws-stack
version: 1.1.0
cloudType: aws
registry: Public Repo
layers:
  - name: ubuntu-aws
    version: "24.04"
  - name: kubernetes
    version: 1.36.2
  - name: cni-cilium-oss
    version: 1.20.1
  - name: csi-aws-ebs
    version: 1.65.0
  - name: cert-manager
    version: 1.21.1
YAML
show review-candidate 0 'cat aws-stack.yaml'
show2 review-snapshot 0 'strata snapshot --from-candidate aws-stack.yaml -o candidate.json'
show review-markdown 1 'strata compare releases/1.0.0/dev.json candidate.json --format markdown'
show review-repave 1 "strata compare releases/1.0.0/dev.json candidate.json --json | jq -r '.packs[] | select(.impact.level == \"repave\") | \"\(.name): \(.impact.text)\"'"
show2 review-pr-comment 0 'scripts/ci/pr-comment.sh releases/1.0.0/dev.json candidate.json --dry-run > comment.md'
show review-pr-comment 0 'head -3 comment.md'
profile $DEV $A110 1.1.0 aws-stack@1.1.0
show2 review-published 0 'strata snapshot aws-stack@1.1.0 -o releases/1.1.0/dev.json'
show review-same 0 'strata same-release candidate.json releases/1.1.0/dev.json'

chapter merge-gate PALETTE_PROJECT=dev GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null \
  GIT_AUTHOR_NAME=dev GIT_AUTHOR_EMAIL=dev@example.com \
  GIT_COMMITTER_NAME=dev GIT_COMMITTER_EMAIL=dev@example.com \
  GIT_AUTHOR_DATE=2026-09-10T10:00:00+00:00 GIT_COMMITTER_DATE=2026-09-10T10:00:00+00:00
projects dev=$DEV
registries $DEV
profile $DEV $A100 1.0.0 aws-stack@1.0.0
# main pins aws-stack@1.0.0, and a branch changes the record: the profile was edited in place
quiet 'mkdir -p releases && git init -q -b main . && git remote add origin .'
quiet 'strata snapshot aws-stack@1.0.0 -o releases/dev.json &&
  git add releases && git commit -qm "dev pins aws-stack@1.0.0"'
quiet 'git checkout -q -b keep-the-volumes'
edit $DEV $A100 "$RETAIN"
quiet 'strata snapshot aws-stack@1.0.0 -o releases/dev.json &&
  git commit -qam "keep the volumes on delete"'
# while that branch waited, another merge request published 1.1.0 and pinned it on main
quiet 'git checkout -q main'
profile $DEV $A110 1.1.0 aws-stack@1.1.0
quiet 'strata snapshot aws-stack@1.1.0 -o releases/dev.json &&
  git commit -qam "dev pins aws-stack@1.1.0"'
# the branch settled the conflict by keeping its own record, which is what a hand merge does
quiet 'git checkout -q keep-the-volumes && git fetch -q origin main &&
  { git merge -q --no-edit --no-commit origin/main || true; } &&
  git checkout HEAD -- releases/dev.json &&
  git commit -qm "merge main, keep our record"'
show2 merge-gate-behind 1 'scripts/ci/merge-gate.sh "releases/*.json" --base main'
# the fix: pin the version that main has, with the change on top
quiet 'strata snapshot aws-stack@1.1.0 -o releases/dev.json && git commit -qam "pin aws-stack@1.1.0"'
show2 merge-gate-clear 0 'scripts/ci/merge-gate.sh "releases/*.json" --base main'
# and the check that git cannot make: the pinned profile is edited in Palette after the record
edit $DEV $A110 "$RETAIN"
show2 merge-gate-drifted 1 'scripts/ci/merge-gate.sh "releases/*.json" --base main'

chapter merge-gate-rollback PALETTE_PROJECT=dev GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null \
  GIT_AUTHOR_NAME=dev GIT_AUTHOR_EMAIL=dev@example.com \
  GIT_COMMITTER_NAME=dev GIT_COMMITTER_EMAIL=dev@example.com \
  GIT_AUTHOR_DATE=2026-09-10T10:00:00+00:00 GIT_COMMITTER_DATE=2026-09-10T10:00:00+00:00
projects dev=$DEV
registries $DEV
profile $DEV $A100 1.0.0 aws-stack@1.0.0
profile $DEV $A110 1.1.0 aws-stack@1.1.0
# dev ran 1.0.0, and then 1.1.0
quiet 'mkdir -p releases && git init -q -b main . && git remote add origin .'
quiet 'strata snapshot aws-stack@1.0.0 -o releases/dev.json &&
  git add releases && git commit -qm "dev pins aws-stack@1.0.0"'
quiet 'strata snapshot aws-stack@1.1.0 -o releases/dev.json &&
  git commit -qam "dev pins aws-stack@1.1.0"'
# 1.1.0 repaved the nodes badly, so a branch off main puts back what dev ran before
quiet 'git checkout -q -b roll-dev-back &&
  git show main~1:releases/dev.json > releases/dev.json &&
  git commit -qam "pin what dev ran before 1.1.0"'
show2 merge-gate-rollback-red 1 'scripts/ci/merge-gate.sh "releases/*.json" --base main'
# the same change, with the rollback declared in the commit that makes it
quiet 'git commit -q --amend -m "roll dev back to aws-stack@1.0.0

Rollback: releases/dev.json to aws-stack@1.0.0
1.1.0 repaved the nodes with a CSI driver that lost the volumes"'
show2 merge-gate-rollback 0 'scripts/ci/merge-gate.sh "releases/*.json" --base main'

chapter promote
projects dev=$DEV prod=$PROD
profile $DEV $A111 1.1.1 aws-stack@1.1.1
profile $PROD $A111_PROD 1.1.1 aws-stack@1.1.1 "$RETAIN"
quiet 'mkdir -p releases/1.1.1'
show2 promote-record 0 'PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json'
show2 promote-record 0 'PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.1 -o releases/1.1.1/prod.json'
show promote-different 1 'strata same-release releases/1.1.1/dev.json releases/1.1.1/prod.json'
show promote-compare 1 'strata compare releases/1.1.1/dev.json releases/1.1.1/prod.json'
show promote-consumers 0 'PALETTE_PROJECT=prod strata consumers aws-stack@1.1.1'
find "$tenant/$PROD/v1/clusterprofiles" -mindepth 1 -delete
profile $PROD $A111_PROD2 1.1.1 aws-stack@1.1.1
show2 promote-again 0 'PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.1 -o releases/1.1.1/prod.json'
show promote-same 0 'strata same-release releases/1.1.1/dev.json releases/1.1.1/prod.json'

chapter registry-move
projects dev=$DEV prod=$PROD
profile $DEV $A111 1.1.1 aws-stack@1.1.1
profile $PROD $A111_PROD 1.1.1 aws-stack@1.1.1 "$OCI_CSI"
quiet 'mkdir -p releases/1.1.1'
quiet 'PALETTE_PROJECT=dev strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json'
quiet 'PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.1 -o releases/1.1.1/prod.json'
show move-exact 1 'strata same-release releases/1.1.1/dev.json releases/1.1.1/prod.json'
show move-compare 1 'strata compare releases/1.1.1/dev.json releases/1.1.1/prod.json'
show move-fields 1 "strata compare releases/1.1.1/dev.json releases/1.1.1/prod.json --json | jq '.packs[] | select(.identity) | {name, a: .a | {packUid, registryUid, upstream}, b: .b | {packUid, registryUid, upstream}}'"
show move-allow 0 'strata same-release --allow registry-move releases/1.1.1/dev.json releases/1.1.1/prod.json'
find "$tenant/$PROD/v1/clusterprofiles" -mindepth 1 -delete
profile $PROD $A111_PROD 1.1.1 aws-stack@1.1.1 "$OCI_CSI | $RETAIN"
quiet 'PALETTE_PROJECT=prod strata snapshot aws-stack@1.1.1 -o releases/1.1.1/prod.json'
show move-allow-edited 1 'strata same-release --allow registry-move releases/1.1.1/dev.json releases/1.1.1/prod.json'

chapter tag-name PALETTE_PROJECT=dev
projects dev=$DEV
profile $DEV $A111 1.1.1 aws-stack@1.1.1
profile $DEV $A112 1.1.2 aws-stack@1.1.1 "$TAG_2404"
show tag-compare 1 'strata compare aws-stack@1.1.1 aws-stack@1.1.2'
show tag-same 0 'strata same-release aws-stack@1.1.1 aws-stack@1.1.2'
show tag-json 1 "strata compare aws-stack@1.1.1 aws-stack@1.1.2 --json | jq '{same, sameContent, maxImpact: .summary.maxImpact, renamed: [.packs[] | select(.tagRenamed) | .name]}'"
quiet 'mkdir -p releases/1.1.2 && strata snapshot aws-stack@1.1.2 -o releases/1.1.2/dev.json'
edit $DEV $A112 "$TAG_LTS2404"
show tag-verify 1 'strata verify releases/1.1.2/dev.json'

chapter manifest-pack
projects dev=$DEV prod=$PROD
addons $DEV $ADDONS_DEV 64e2000000000000000000d1 restricted
addons $PROD $ADDONS_PROD 64e3000000000000000000d1 restricted
quiet 'mkdir -p addons/1.0.0'
show2 manifest-record 0 'PALETTE_PROJECT=dev strata snapshot platform-addons@1.0.0 -o addons/1.0.0/dev.json'
show2 manifest-record 0 'PALETTE_PROJECT=prod strata snapshot platform-addons@1.0.0 -o addons/1.0.0/prod.json'
show manifest-compare 1 'strata compare addons/1.0.0/dev.json addons/1.0.0/prod.json'
show manifest-same 0 'strata same-release addons/1.0.0/dev.json addons/1.0.0/prod.json'
find "$tenant/$PROD/v1/clusterprofiles" -mindepth 1 -delete
addons $PROD $ADDONS_PROD 64e3000000000000000000d1 baseline
quiet 'PALETTE_PROJECT=prod strata snapshot platform-addons@1.0.0 -o addons/1.0.0/prod.json'
show manifest-different 1 'strata same-release addons/1.0.0/dev.json addons/1.0.0/prod.json'

chapter edited-in-place PALETTE_PROJECT=dev
projects dev=$DEV
profile $DEV $A111 1.1.1 aws-stack@1.1.1
cluster $DEV $PAYMENTS payments $A111
quiet 'mkdir -p releases/1.1.1 runs'
show2 edited-record 0 'strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json'
show2 edited-record 0 'strata snapshot cluster:payments -o runs/payments.json'
show edited-verify-ok 0 'strata verify releases/1.1.1/dev.json'
edit $DEV $A111 "$RETAIN"
show edited-verify 1 'strata verify releases/1.1.1/dev.json'
show edited-compare 1 'strata compare releases/1.1.1/dev.json aws-stack@1.1.1'
show edited-cluster-kept 0 'strata verify runs/payments.json'
cluster $DEV $SEARCH search $A111
show edited-two-clusters 1 'strata compare cluster:payments cluster:search'
show edited-bulk 1 "strata verify 'releases/**/*.json' 'runs/*.json'"
show edited-bulk-json 1 "strata verify 'releases/**/*.json' 'runs/*.json' --json | jq '.summary'"
show2 edited-report 1 "scripts/ci/verify-report.sh 'releases/**/*.json' 'runs/*.json' --dry-run"
cluster $DEV $PAYMENTS payments $A111
show edited-cluster-updated 1 'strata verify runs/payments.json'

chapter overwritten PALETTE_PROJECT=dev
projects dev=$DEV
profile $DEV $A111 1.1.1 aws-stack@1.1.1
cluster $DEV $PAYMENTS payments $A111
quiet 'mkdir -p releases/1.1.1 runs'
quiet 'strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json'
quiet 'strata snapshot cluster:payments -o runs/payments.json'
edit $DEV $A111 ".spec.version = \"1.2.0\" | $RETAIN"
show overwritten-list 0 'strata list profiles'
show overwritten-verify 1 'strata verify releases/1.1.1/dev.json'
show overwritten-cluster 1 'strata verify runs/payments.json'
show overwritten-consumers 1 'strata consumers aws-stack@1.2.0'
show overwritten-compare 1 'strata compare cluster:payments aws-stack@1.2.0'
show2 overwritten-candidate 0 'strata candidate releases/1.1.1/dev.json -o aws-stack-1.1.1.yaml --files-dir aws-stack-1.1.1'
edit $DEV $A111 '.spec.version = "1.1.1"'
rewrite "$(doc $DEV "v1/clusterprofiles/$A111")" "$(pack_values csi-aws-ebs) |= sub(\"reclaimPolicy: Retain\"; \"reclaimPolicy: Delete\")"
profile $DEV $A120 1.2.0 aws-stack@1.1.1 "$RETAIN"
show overwritten-flag-list 0 'strata list profiles'
show overwritten-flag-verify 0 'strata verify releases/1.1.1/dev.json runs/payments.json'

chapter cluster-release PALETTE_PROJECT=dev
projects dev=$DEV
profile $DEV $A110 1.1.0 aws-stack@1.1.0
profile $DEV $A111 1.1.1 aws-stack@1.1.1
addons $DEV $ADDONS_DEV 64e2000000000000000000d1 restricted
cluster $DEV $PAYMENTS payments $A110 $ADDONS_DEV
cluster $DEV $SEARCH search $A111
cluster $DEV $LEDGER ledger $A110
on_cluster $DEV $LEDGER "(.spec.clusterProfileTemplates[].packs[] | select(.name == \"csi-aws-ebs\") | .values) |= sub(\"reclaimPolicy: Delete\"; \"reclaimPolicy: Retain\")"
quiet 'mkdir -p releases/1.1.0 runs'
quiet 'strata snapshot aws-stack@1.1.0 -o releases/1.1.0/dev.json'
show cluster-identical 0 'strata compare releases/1.1.0/dev.json cluster:payments/aws-stack'
show cluster-all-profiles 1 'strata compare releases/1.1.0/dev.json cluster:payments'
show cluster-moved 1 'strata compare releases/1.1.0/dev.json cluster:search'
show cluster-edited 1 'strata compare releases/1.1.0/dev.json cluster:ledger'
show2 cluster-run-record 0 'strata snapshot cluster:payments/aws-stack -o runs/payments.json'
show cluster-run-record 0 'strata verify runs/payments.json'

chapter cluster-values PALETTE_PROJECT=dev
projects dev=$DEV
profile $DEV $A111 1.1.1 aws-stack@1.1.1
cluster $DEV $PAYMENTS payments $A111 -- platform_owner=team-payments
cluster $DEV $SEARCH search $A111 -- platform_owner=team-search
show values-compare 0 'strata compare cluster:payments cluster:search'
quiet 'mkdir -p runs && strata snapshot cluster:payments -o runs/payments.json'
show values-record 0 'jq .source.cluster.variables runs/payments.json'
set_value $DEV $PAYMENTS platform_owner team-platform
show values-verify 0 'strata verify runs/payments.json'

chapter same-names
projects dev=$DEV prod=$PROD
profile $DEV $A111 1.1.1 aws-stack@1.1.1
profile $PROD $A111_PROD 1.1.1 aws-stack@1.1.1 "$RETAIN"
cluster $DEV $PAYMENTS payments $A111
cluster $PROD $PAYMENTS_PROD payments $A111_PROD
show names-tenant 0 "strata list clusters --project ''"
show2 names-ambiguous 2 'strata snapshot cluster:payments -o payments.json'
show2 names-profile 2 'strata snapshot aws-stack@1.1.1 -o aws-stack.json'
show2 names-by-project 0 'PALETTE_PROJECT=dev strata snapshot cluster:payments -o dev-payments.json'
show2 names-by-project 0 'PALETTE_PROJECT=prod strata snapshot cluster:payments -o prod-payments.json'
show names-compare 1 'strata compare dev-payments.json prod-payments.json'

chapter packs PALETTE_PROJECT=dev
projects dev=$DEV prod=$PROD
registries $DEV
show pack-compare 1 "strata compare 'pack:Public Repo/kubernetes@1.35.3' 'pack:Public Repo/kubernetes@1.36.2'"
show pack-os 1 "strata compare 'pack:Public Repo/ubuntu-aws@20.04' 'pack:Public Repo/ubuntu-aws@24.04'"
show2 pack-ambiguous 2 'strata compare pack:csi-aws-ebs@1.65.0 pack:csi-aws-ebs@1.65.0'
show pack-registries 1 "strata compare 'pack:Public Repo/csi-aws-ebs@1.65.0' 'pack:Palette Registry/csi-aws-ebs@1.65.0'"
show2 pack-record 0 "strata snapshot 'pack:Public Repo/kubernetes@latest' -o kubernetes.json"
show pack-verify 0 'strata verify kubernetes.json'

chapter retire PALETTE_PROJECT=dev
projects dev=$DEV
profile $DEV $A100 1.0.0 aws-stack@1.0.0
profile $DEV $A110 1.1.0 aws-stack@1.1.0
cluster $DEV $PAYMENTS payments $A110
cluster $DEV $SEARCH search $A110
show retire-in-use 1 'strata consumers aws-stack@1.1.0'
show retire-free 0 'strata consumers aws-stack@1.0.0'
show retire-gate 0 'strata consumers aws-stack@1.0.0 && echo "no cluster uses aws-stack@1.0.0"'
show retire-json 1 "strata consumers aws-stack@1.1.0 --json | jq -r '.consumers[] | \"\(.name) \(.state) \(.version)\"'"

# ---- chapters: automation ---------------------------------------------------------------------

chapter automation PALETTE_PROJECT=dev
projects dev=$DEV
profile $DEV $A100 1.0.0 aws-stack@1.0.0
profile $DEV $A110 1.1.0 aws-stack@1.1.0
profile $DEV $A111 1.1.1 aws-stack@1.1.1
show auto-max-impact 1 'strata compare aws-stack@1.0.0 aws-stack@1.1.0 --json | jq -r .summary.maxImpact'
show auto-gate 1 'strata compare aws-stack@1.1.0 aws-stack@1.1.1 --json > report.json'
show auto-gate 0 "jq -e '.summary.maxImpact != \"repave\"' report.json"
show2 auto-error 2 'strata compare aws-stack@1.1.1 aws-stack@9.9.9 --json --quiet'
show2 auto-error-text 2 'strata compare aws-stack@1.1.1 aws-stack@9.9.9 --quiet'

# ---- the result -------------------------------------------------------------------------------

if [ -n "$check" ]; then
  if ! diff -ru "$dest" "$out"; then
    echo "docs-examples: docs/src/output/ is not current. Run: just docs-examples" >&2
    exit 1
  fi
  echo "docs-examples: docs/src/output/ is current ($(ls "$out" | wc -l) files)"
else
  rm -rf "$dest"
  mkdir -p "$dest"
  cp "$out"/*.txt "$dest/"
  echo "docs-examples: wrote $(ls "$dest" | wc -l) files to docs/src/output/"
fi

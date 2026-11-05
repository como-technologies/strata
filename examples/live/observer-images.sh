#!/usr/bin/env bash
# The digest that each pod runs (#646). The observer reads the pods of cluster A with the component
# deploy/observer/pods. A throwaway tag on ttl.sh moves from one image to another while pods run it.
# The page of the observer shows each step. It shows the first sight of a tag, a split, and a move.
# It shows a pod that never started, a pod in a crash loop, and the images that match no pack, such as
# the images of the agents of Palette.
#
#   examples/live/observer-images.sh [FROM_STAGE]
#
#   1  the setup: the pull Secret of the image of the observer, the tag on ttl.sh holds its first
#      image, and the values of the add-on list the tag. A record of each profile applies. The
#      observer installs with the component, from the image of OBSERVER_IMAGE. It has the role of the
#      component, and it reads the pods. When the token file fails, the run stops at the pull Secret,
#      before it changes the cluster
#   2  the first sight: a pod runs the tag. The observer shows the first digest and no finding
#   3  the tag moves to a second image, and a new pod runs it. The observer shows a split. Then the
#      first pod is replaced, so each pod runs the second image. The observer shows a move
#   4  a pod that never starts, and a pod in a crash loop
#   5  the images that match no pack, such as the images of the agents of Palette, make no finding. A
#      pod made with IfNotPresent shows the pull policy that the cluster sets
#   6  the end: the pods, the observer, the role, and the values of the add-on go back
#
# Every pod of the run pulls with Always, so a new pod pulls the tag again. A cluster can turn on the
# admission plugin AlwaysPullImages, which sets Always for each pod. The check pull-policy shows what
# cluster A does. An image on ttl.sh lives for two hours from its push. The names of the images hold a
# random suffix.
#
# The image of the observer is on a private package of GitHub, and the script never pushes it to
# another registry. The nodes pull it with the Secret ghcr-pull, which comes from a token file. The
# images on ttl.sh are copies of public images only.
#
# The only write to the tenant is an edit in place of the values of the add-on. It lists the images
# of the run, so that a pack names them. The undo puts the values back. The script applies no update
# to cluster A.
#
# The namespaces, the role, and the binding go at the end, also after a failed check or an error. The
# script deletes no cluster. KEEP=1 skips the undo, so that FROM_STAGE can resume the run.
#
# Needs the live example at its baseline with cluster A, the strata release binary, crane, kubectl,
# curl, jq, gh, git, the key of verify.sh, and the token file. Run just live-up first. It is billable
# until live-down. OBSERVER_IMAGE names the image of the observer, which must hold the code of this
# commit (default: the image of the version in Cargo.toml). GHCR_PULL_TOKEN_FILE names the token file
# (default: ~/.config/strata/ghcr-pull-token, mode 0600). It holds a token with only the scope
# read:packages. PORT is the local port of the port-forward (default 18080). CYCLE_TRIES is the number
# of looks, 5 seconds apart, in a wait for the observer (default 60).
#
# Results go under runs/<UTC time>/observer-images/, or OUT. FROM_STAGE resumes a run in OUT. Exit 0
# only when no check fails. A check with ? records a result without judging it.
set -uo pipefail
script=observer-images
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4|5|6) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"
for tool in crane kubectl curl jq gh git timeout; do
  command -v "$tool" > /dev/null || die "$tool is needed"
done
root=$(cd "$live/../.." && pwd)
observer_image
port=${PORT:-18080}
tries=${CYCLE_TRIES:-60}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/observer-images}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
C=$(out cluster_uid)
AD=$(out addon_profile_uid)
REL=$(out release_profile_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
export STRATA_BIN=$strata
records_script=$root/scripts/ci/observer-records.sh
revision=$(git -C "$root" rev-parse HEAD)
palette_settings

ns=strata-observer
tns=strata-live-images
role=strata-observer-pods
pack=strata-live-config

# What the run makes. ids.env keeps it, so that a resumed run knows it.
edited= d1= d2=
save() { echo "$1=$2" >> ids.env; }
if [ ! -f ids.env ]; then
  suffix=$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')
  save suffix "$suffix"
fi
. ./ids.env
# The tag that moves. The second name has no image: a pull of it fails. The third is a copy of an image
# with a shell, for the pod in a crash loop.
moving=ttl.sh/strata-live-$suffix:2h
absent=ttl.sh/strata-live-$suffix-absent:2h
crash=ttl.sh/strata-live-$suffix-crash:2h
# The tag holds source_one first and source_two later. source_crash has a shell, for the crash loop.
# source_policy is the image of the pod that asks for IfNotPresent. A copy keeps the digest of the index.
source_one=registry.k8s.io/pause:3.9
source_two=registry.k8s.io/pause:3.10
source_crash=docker.io/library/busybox:1.37
source_policy=registry.k8s.io/pause:3.9
undone=

# ---- the page ------------------------------------------------------------------------------------------

# row_check REF FILTER [JQ OPTION...]: the row of the image REF holds what FILTER says. image.json keeps
# the row.
row_check() {
  local ref=$1 filter=$2
  shift 2
  page "/v1/images/$(uri "$ref")" > image.json 2> /dev/null && jq -e "$@" "$filter" image.json > /dev/null
}
# has_finding KIND REF: /v1/findings holds a finding of KIND that names the image REF
has_finding() {
  page /v1/findings > findings.json 2> /dev/null \
    && jq -e --arg k "$1" --arg r "$2" '[.findings[] | select(.kind == $k and ((.images // []) | index($r) != null))] | length > 0' findings.json > /dev/null
}
# no_finding_for REF: no finding of any kind names the image REF
no_finding_for() {
  page /v1/findings > findings.json 2> /dev/null \
    && jq -e --arg r "$1" '[.findings[] | select((.images // []) | index($r) != null)] | length == 0' findings.json > /dev/null
}
# show_row: the row that row_check read
show_row() { jq '.image | {ref, status, match, digests}' image.json; }

# ---- the setup -----------------------------------------------------------------------------------------

# tag_first: the tag holds the first image. A copy under the name of the crash loop is the image with a
# shell. The absent name has no image.
tag_first() {
  crane copy "$source_one" "$moving" && d1=$(crane digest "$moving") && save d1 "$d1" || return 1
  crane copy "$source_crash" "$crash" || return 1
  if crane digest "$absent" > /dev/null 2>&1; then
    echo "$absent has an image"
    return 1
  fi
  echo "$moving holds $d1"
}
# images_values: the values of the pack, with the three images in pack.content.images
images_values() {
  printf 'pack:\n  content:\n    images:\n'
  printf '      - image: %s\n' "$moving" "$absent" "$crash"
}
# values_edit: the pack names the three images, in place. The flag comes first, so that the undo knows
# of the edit also when the call fails.
values_edit() {
  edited=1 && save edited 1
  "$live/api.sh" set-values "$AD" "$pack" "$(images_values)"
}
# namespaces_make: the namespace of the pods. The pull Secret made the namespace of the observer, with the
# restricted standard.
namespaces_make() { kc create namespace "$tns"; }
observer_settings() { observer_config && observer_key "$palette_key"; }
# records: a record of the release profile and of the add-on profile, after the edit. The add-on record
# lists the tag in a pack. The records apply with --server-side.
records_make() {
  "$strata" snapshot "$REL" -o release.json -q && "$strata" snapshot "$AD" -o addon.json -q \
    && jq -e --arg p "$pack" --arg i "$moving" '[.packs[] | select(.name == $p) | .images[]?] | index($i) != null' addon.json > /dev/null \
    && "$records_script" --revision "$revision" release.json addon.json | kc apply --server-side -f -
}
# overlay: the overlay of lib.sh, with the component. The content cycle gets a short period.
overlay() {
  observer_overlay --content-every=1m && observer_overlay_pods
}
# observer_apply: render the overlay, make sure that the component is in it, and apply it
observer_apply() {
  overlay && observer_render && grep -q 'name: kube-credentials' observer.yaml \
    && kc apply -f observer.yaml
}

# ---- the read ------------------------------------------------------------------------------------------

# pods_role: the service account of the observer gets and lists pods in each namespace, and it does
# nothing else
pods_role() {
  local as=--as=system:serviceaccount:$ns:strata-observer verb
  for verb in get list; do
    [ "$(can_i "$verb" pods --all-namespaces "$as")" = yes ] || { echo "no $verb on pods"; return 1; }
  done
  for verb in watch create update patch delete; do
    [ "$(can_i "$verb" pods --all-namespaces "$as")" = no ] || { echo "$verb on pods"; return 1; }
  done
  [ "$(can_i list nodes "$as")" = no ] || { echo "list nodes"; return 1; }
  for verb in get list; do
    [ "$(can_i "$verb" secrets --all-namespaces "$as")" = no ] || { echo "$verb secrets"; return 1; }
  done
  echo "get and list on pods, and nothing else"
}

# ---- the pods ------------------------------------------------------------------------------------------

# make_pod NAME IMAGE POLICY [COMMAND]: a pod in the namespace of the run. COMMAND is a flow sequence,
# such as '["sh","-c","exit 1"]'.
make_pod() {
  kc apply -f - << EOF
apiVersion: v1
kind: Pod
metadata:
  name: $1
  namespace: $tns
spec:
  containers:
    - name: c
      image: $2
      imagePullPolicy: $3
${4:+      command: $4}
EOF
}
wait_ready() { KUBE_TIMEOUT=200 kc -n "$tns" wait --for=condition=Ready "pod/$1" --timeout=180s; }
# pod_json NAME: the pod as the cluster holds it. pod-NAME.json keeps it.
pod_json() { kj -n "$tns" get pod "$1" -o json > "pod-$1.json"; }

# first_row: one pod runs the first image. The row holds one digest, the first one, and no finding names
# the tag.
first_row() {
  row_check "$moving" '.image | .status == "steady" and .match == "exact" and .byDigest == false
    and (.digests | length == 1) and .digests[0].digest == $d and .digests[0].status == "first-digest" and .digests[0].pods == 1' \
    --arg d "$d1" && no_finding_for "$moving"
}
first_sight() {
  make_pod p1 "$moving" Always && wait_ready p1 || return 1
  poll "$tries" first_row || { show_row; return 1; }
  show_row
}
# move_tag: the tag holds the second image. A copy moves the whole index.
move_tag() {
  crane copy "$source_two" "$moving" && d2=$(crane digest "$moving") && save d2 "$d2" || return 1
  [ "$d2" != "$d1" ] || { echo "the tag holds $d1 still"; return 1; }
  echo "$moving holds $d2, and held $d1"
}
# split_row: two pods run two digests. The first one is the first digest, and the other is another one.
split_row() {
  row_check "$moving" '.image | .status == "split" and (.digests | length == 2)
    and ([.digests[] | select(.digest == $a and .status == "first-digest" and .pods == 1)] | length == 1)
    and ([.digests[] | select(.digest == $b and .status == "another-digest" and .pods == 1)] | length == 1)' \
    --arg a "$d1" --arg b "$d2" && has_finding image-split "$moving"
}
image_split() {
  move_tag && make_pod p2 "$moving" Always && wait_ready p2 || return 1
  poll "$tries" split_row || { show_row; return 1; }
  show_row
}
# moved_row: each pod runs the second digest. It is not the first digest, so the image moved and it is
# no split.
moved_row() {
  row_check "$moving" '.image | .status == "moved" and (.digests | length == 1)
    and .digests[0].digest == $b and .digests[0].status == "another-digest" and .digests[0].pods == 2' \
    --arg b "$d2" && has_finding image-moved "$moving" && ! has_finding image-split "$moving"
}
image_moved() {
  KUBE_TIMEOUT=150 kc -n "$tns" delete pod p1 --wait=true && make_pod p1 "$moving" Always && wait_ready p1 || return 1
  poll "$tries" moved_row || { show_row; return 1; }
  show_row
}

# pull_fails NAME: the pod waits for an image that it cannot pull
pull_fails() {
  pod_json "$1" && jq -e '.status.containerStatuses[0] | (.state.waiting.reason // "") | test("ErrImagePull|ImagePullBackOff")' "pod-$1.json" > /dev/null
}
# never_row: the container never started, so the row holds one digest with no digest in it, and the
# status is Not checked. A finding is possible, because the tag is in a pack, and none exists.
never_row() {
  row_check "$absent" '.image | .status == "not-checked" and .match == "exact"
    and (.digests | length == 1) and .digests[0].status == "not-started" and (.digests[0] | has("digest") | not) and .digests[0].pods == 1' \
    && no_finding_for "$absent"
}
never_started() {
  make_pod absent "$absent" Always || return 1
  poll 24 pull_fails absent || { jq '.status.containerStatuses[0]' pod-absent.json; return 1; }
  jq -e '(.status.containerStatuses[0].imageID // "") == ""' pod-absent.json > /dev/null || { echo "the pod has an image id"; return 1; }
  poll "$tries" never_row || { show_row; return 1; }
  show_row
}
# crash_looping: the container restarted twice at least, and its status holds an image id
crash_looping() {
  pod_json crash && jq -e '.status.containerStatuses[0] | .restartCount >= 2 and (.imageID // "") != ""' pod-crash.json > /dev/null
}
# crash_row: the container keeps its digest in a crash loop, so the row holds it as the first digest
crash_row() {
  row_check "$crash" '.image | .status == "steady" and (.digests | length == 1)
    and .digests[0].digest == $d and .digests[0].status == "first-digest"' --arg d "$(crane digest "$crash")" \
    && no_finding_for "$crash"
}
crash_loop() {
  make_pod crash "$crash" Always '["sh","-c","exit 1"]' || return 1
  poll 36 crash_looping || { jq '.status.containerStatuses[0]' pod-crash.json; return 1; }
  jq -c '.status.containerStatuses[0] | {restartCount, imageID}' pod-crash.json
  poll "$tries" crash_row || { show_row; return 1; }
  show_row
}
# agents_unmatched: the answer of the page names the images that match no pack. These are the images of
# the agents of Palette and of the system, which no pack names. The cluster runs some, and no finding
# names one. agent-images.txt keeps the images.
agents_unmatched() {
  page /v1/images > images.json && page /v1/findings > findings.json || return 1
  jq -r '.images[] | select(.match == "none") | .ref' images.json | sort -u > agent-images.txt
  [ -s agent-images.txt ] || { echo "no image matches no pack: the answer of the page holds no agent"; return 1; }
  jq -e --rawfile list agent-images.txt '($list | split("\n") | map(select(. != ""))) as $agents
    | [.findings[] | (.images // [])[] | select(. as $i | $agents | index($i) != null)] | length == 0' findings.json > /dev/null \
    || { echo "a finding names an image that matches no pack"; return 1; }
  echo "$(wc -l < agent-images.txt | tr -d ' ') images match no pack, and no finding names one"
}
# pull_policy: a pod made with IfNotPresent. The output says what the cluster did to the policy. It
# succeeds when the cluster set Always.
pull_policy() {
  make_pod policy "$source_policy" IfNotPresent || return 1
  pod_json policy || return 1
  jq -r '"asked IfNotPresent, the cluster set " + .spec.containers[0].imagePullPolicy' pod-policy.json
  jq -e '.spec.containers[0].imagePullPolicy == "Always"' pod-policy.json > /dev/null
}

# ---- the end -------------------------------------------------------------------------------------------

namespace_gone() { not_found get namespace "$1"; }
# namespaces_gone: both namespaces are gone. A namespace stays in Terminating for a while.
namespaces_gone() { poll 36 namespace_gone "$ns" && poll 36 namespace_gone "$tns"; }
role_gone() { not_found get clusterrole "$role" && not_found get clusterrolebinding "$role"; }
# values_back: the pack of the add-on names none of the images of the run
values_back() {
  "$strata" snapshot "$AD" -o addon-end.json -q \
    && jq -e --arg p "$pack" '[.packs[] | select(.name == $p) | .images[]?] | length == 0' addon-end.json > /dev/null
}
# undo: return cluster A to its baseline. It is safe to run again. The values of the add-on, the pods,
# the observer, the role, and the binding go. No cluster goes.
undo() {
  pf_stop
  if [ -n "$edited" ]; then
    "$live/api.sh" set-values "$AD" "$pack" "" >> steps.log 2>&1 && edited= && save edited ""
  fi
  KUBE_TIMEOUT=300 kc delete namespace "$tns" --ignore-not-found >> steps.log 2>&1
  [ ! -f observer.yaml ] || KUBE_TIMEOUT=300 kc delete -f observer.yaml --ignore-not-found >> steps.log 2>&1
  KUBE_TIMEOUT=300 kc delete namespace "$ns" --ignore-not-found >> steps.log 2>&1
  kc delete clusterrolebinding "$role" --ignore-not-found >> steps.log 2>&1
  kc delete clusterrole "$role" --ignore-not-found >> steps.log 2>&1
  undone=1
}

summary() {
  write_summary "the digest that each pod runs" ", cluster \`$C\`, tag \`$moving\`, image \`$image\`, revision \`$revision\`."
}
# the port-forward, the kubeconfig, and the changes of the run go at the end, also when a check stops
# the script
finish() {
  summary
  pf_stop
  [ -n "$undone" ] || [ "${KEEP:-}" = 1 ] || undo
  [ -z "$kubeconfig" ] || rm -f "$kubeconfig"
}
trap finish EXIT

if stage 1 "the setup"; then
  require_baseline
  check pull-secret 0 '#646: the Secret ghcr-pull holds the token of the file, which has only read:packages and comes through standard input' -- pull_secret
  [ "$failures" = 0 ] || pull_secret_stop
  check tag-first 0 '#646: the tag on ttl.sh holds its first image, and an image with a shell is there for the crash loop' -- tag_first
  check values-edit 0 "#646: the pack $pack of the add-on lists the three images of the run, in place" -- values_edit
  check namespaces 0 '#646: the namespace of the pods' -- namespaces_make
  check settings 0 '#646: the ConfigMap holds the cluster, the address of Palette, and the project. The Secret holds the key' -- observer_settings
  check records 0 '#646: a record of each profile, with the tag in the add-on record, applies with --server-side' -- records_make
  check observer-apply 0 '#646: the overlay lists the component, the image of OBSERVER_IMAGE, and the pull Secret, and it applies' -- observer_apply
  check observer-ready 0 '#646: the Deployment becomes available' -- observer_ready
fi

if stage 2 "the read and the first sight"; then
  check pods-role 0 '#646: the account of the observer gets and lists pods in each namespace, and it does nothing else' -- pods_role
  check pods-read 0 '#646: /v1/images says read, with a time and a count of images: the token, the role, and the policy work' -- pods_read
  check image-first 0 '#646: a pod runs the tag. The row holds the first digest, and no finding names the tag' -- first_sight
fi

if stage 3 "a split and a move"; then
  check image-split 0 '#646: the tag moves, and a new pod runs the second digest. The row says split, and the finding image-split names the tag' -- image_split
  check image-moved 0 '#646: the first pod is replaced, so each pod runs the second digest. The row says moved, and the finding image-moved replaces the split' -- image_moved
fi

if stage 4 "a pod that never starts, and a pod in a crash loop"; then
  check image-never-started 0 '#646: a pull that fails leaves the image id empty. The row reads Not checked, and it makes no finding' -- never_started
  check image-crash-loop 0 '#646: a container in a crash loop keeps its digest. The row shows it, and it makes no finding' -- crash_loop
fi

if stage 5 "the images that match no pack, and the pull policy"; then
  check image-agents-no-finding 0 '#646: the images that match no pack, such as those of the agents of Palette, make no finding: the answer of the page names them' -- agents_unmatched
  check pull-policy ? '#646: a pod made with IfNotPresent: 0 says that the cluster set Always, and 1 says that the policy stayed' -- pull_policy
fi

if stage 6 "the end" && [ "${KEEP:-}" != 1 ]; then
  undo
  check undo-values 0 "the pack $pack names none of the images of the run" -- values_back
  check undo-namespaces 0 '#646: the namespace of the observer and the namespace of the pods are gone' -- namespaces_gone
  check undo-role 0 '#646: the role of the component and its binding are gone' -- role_gone
  check undo-baseline 0 'the example is at its baseline' -- "$live/baseline.sh"
fi

[ "${KEEP:-}" != 1 ] || echo "KEEP=1: the observer, the pods, and the edit stay; FROM_STAGE resumes the run" | tee -a steps.log

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

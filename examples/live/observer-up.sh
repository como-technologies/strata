#!/usr/bin/env bash
# The observer on a test cluster, with one command (#681). It is for a test. A team installs the
# observer with the steps of "The observer" in docs/src/automation/observer.md, and not with this script.
#
#   examples/live/observer-up.sh [up] [--cluster NAME] [--tag TAG] [--read-pods]
#   examples/live/observer-up.sh open [--cluster NAME]
#   examples/live/observer-up.sh down [--cluster NAME]
#
#   up    installs the observer, or repairs an install, and waits for the pod. A second run works: each
#         object is made with kubectl create --dry-run and sent with kubectl apply, so it repairs an
#         object that a person made by hand. The records are the first-seen records of the cluster: the
#         content that each profile holds now. No person approved that content, and the page says so.
#   open  opens a port-forward to the page, and holds it until Ctrl-C. The address is
#         http://127.0.0.1:18080, because the port 8080 is often in use. PORT gives another port.
#   down  deletes the namespace, and the cluster role with its binding. A cluster with no observer is
#         no error.
#
#   --cluster NAME  the cluster, by its name. With no name, the one cluster of the project. With
#                   several clusters, the script lists their names and stops. A name that is not in the
#                   project stops it too.
#   --tag TAG       the tag of the image (default: the newest release on GitHub). Any tag works. A
#                   flag or a read that the image does not hold is a fault of the tag: --read-pods
#                   needs an image with that code.
#   --read-pods     adds the component deploy/observer/pods, so that the observer reads the pods. It
#                   adds a cluster role that gives get and list on the pods of each namespace.
#
# The script asks Palette for the kubeconfig of the cluster and writes it to ~/.kube/strata-<cluster>.yaml
# with the mode 0600. Each kubectl call uses that file, so no call reaches another cluster, whatever the
# context of the shell is. The key of Palette and the pull token go to kubectl through standard input.
# They reach no argument and no file. The overlay lives in a temporary folder that the script deletes.
#
# Needs a built strata (just build), kubectl, curl, jq, gzip, base64, the key of Palette in
# PALETTE_API_KEY or ~/.palette.env, and, for up, a token file with only the scope read:packages
# (GHCR_PULL_TOKEN_FILE, default ~/.config/strata/ghcr-pull-token, mode 0600) and gh. PALETTE_PROJECT_UID
# selects the project. OBSERVER_IMAGE names the whole image, and then --tag is not allowed. The
# observer reads Palette from a node of the cluster, so the node must reach the API.
set -uo pipefail
script=observer-up
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"

mode=up
case "${1:-}" in up|open|down) mode=$1; shift ;; esac
name= tag= read_pods=
while [ $# -gt 0 ]; do
  case "$1" in
    --cluster) [ $# -ge 2 ] || { usage; exit 2; }; name=$2; shift 2 ;;
    --tag) [ $# -ge 2 ] || { usage; exit 2; }; tag=$2; shift 2 ;;
    --read-pods) read_pods=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done
if [ "$mode" != up ] && { [ -n "$tag" ] || [ -n "$read_pods" ]; }; then
  die "--tag and --read-pods are for up"
fi
[ -z "$tag" ] || [ -z "${OBSERVER_IMAGE:-}" ] || die "give --tag or OBSERVER_IMAGE, and not both"
case "$tag" in
  "") ;;
  *[!A-Za-z0-9._-]*|[.-]*) die "$tag is not the tag of an image" ;;
esac
[ -x "$strata" ] || die "no strata binary at $strata; run just build"
for tool in kubectl curl jq timeout; do
  command -v "$tool" > /dev/null || die "$tool is needed"
done

root=$(cd "$live/../.." && pwd)
ns=strata-observer
selector=app.kubernetes.io/name=strata-observer
port=${PORT:-18080}
records_script=$root/scripts/ci/observer-records.sh
export STRATA_QUIET=1 STRATA_BIN=$strata
palette_settings

work=$(mktemp -d) || exit 2
# The port-forward ends with the script. The kubeconfig in ~/.kube stays: it is the kubeconfig of the person.
trap 'pf_stop; rm -rf "$work"' EXIT
trap 'exit 130' INT TERM HUP
cd "$work" || exit 2

# step WHAT COMMAND...: print what the script does, run the command, and stop when it fails
step() {
  local what=$1
  shift
  echo "-- $what"
  "$@" || die "$what failed"
}

# ---- the cluster and its kubeconfig ----------------------------------------------------------------

# choose_cluster: set $C, $cluster, and $PALETTE_PROJECT_UID from the clusters that strata lists. The
# project is that of the cluster, and then that of the listing. The script stops unless exactly one
# cluster matches. It also stops when the uid of the cluster or of its project is empty, so that no
# ConfigMap holds an empty value.
choose_cluster() {
  local list found count
  list=$("$strata" list clusters --json -q 2> list.err) || die "strata list clusters failed: $(cat list.err)"
  found=$(jq -c --arg n "$name" '. as $d | [(.clusters // [])[] | select($n == "" or .name == $n)
    | {uid: (.uid // ""), name: (.name // ""), state: (.state // ""),
       project: ([.projectUid, $d.projectUid] | map(select(. != null and . != "")) | first // "")}]' <<< "$list") \
    || die "strata list clusters gave no document"
  count=$(jq length <<< "$found")
  case "$count" in
    0)
      if [ -n "$name" ]; then
        die "no cluster named $name in the project; its clusters: $(jq -r '[(.clusters // [])[].name] | join(", ")' <<< "$list")"
      fi
      die "the project has no cluster" ;;
    1) ;;
    *)
      if [ -n "$name" ]; then
        die "$count clusters are named $name; set PALETTE_PROJECT_UID to one project. They are: $(jq -r 'map("\(.uid) in project \(.project)") | join(", ")' <<< "$found")"
      fi
      echo "$count clusters are in the project. Name one with --cluster NAME:" >&2
      jq -r '.[] | "  \(.name)  \(.state)"' <<< "$found" >&2
      exit 2 ;;
  esac
  C=$(jq -r '.[0].uid' <<< "$found")
  cluster=$(jq -r '.[0].name' <<< "$found")
  PALETTE_PROJECT_UID=$(jq -r '.[0].project' <<< "$found")
  [ -n "$C" ] || die "the listing gives no uid for the cluster $cluster"
  [ -n "$PALETTE_PROJECT_UID" ] || die "the listing gives no project for the cluster $cluster; set PALETTE_PROJECT_UID"
  case "$cluster" in
    ""|*[!A-Za-z0-9._-]*|[.-]*) die "the name of the cluster, $cluster, cannot be a part of a file name" ;;
  esac
  export PALETTE_PROJECT_UID
}

# write_kubeconfig: ask Palette for the kubeconfig of the cluster, and write ~/.kube/strata-<cluster>.yaml
# with the mode 0600. It writes a file of its own first, so that a failed read leaves the old file. Then
# every kubectl call of lib.sh reads that file.
write_kubeconfig() {
  local tmp
  mkdir -p "$HOME/.kube" || die "cannot make $HOME/.kube"
  tmp=$(mktemp "$HOME/.kube/.strata-kubeconfig.XXXXXX") || die "cannot write to $HOME/.kube"
  if ! "$live/api.sh" get-kubeconfig "$C" > "$tmp" 2> kubeconfig.err || [ ! -s "$tmp" ]; then
    rm -f "$tmp"
    die "no kubeconfig for $cluster: $(cat kubeconfig.err)"
  fi
  chmod 600 "$tmp" && mv -f "$tmp" "$HOME/.kube/strata-$cluster.yaml" || die "cannot write the kubeconfig"
  KUBE_FILE=$HOME/.kube/strata-$cluster.yaml
  kubeconfig=$KUBE_FILE
  export KUBE_FILE
}

# ---- up ---------------------------------------------------------------------------------------------

# newest_release: the tag of the newest release of this repository, as GitHub gives it
newest_release() { (cd "$root" && gh release view --json tagName --jq .tagName); }
# records_first_seen: the first-seen records of the cluster. A server-side apply takes the Secrets over
# from an earlier apply, and a client-side apply fails above 256 KiB.
records_first_seen() { "$records_script" --first-seen "$C" | kc apply --server-side --force-conflicts -f -; }
# render: observer.yaml, from the overlay with the tag, the pull Secret, and two short periods. --read-pods
# adds the component.
render() {
  observer_overlay --content-every=1m --versions-every=5m \
    && { [ -z "$read_pods" ] || observer_overlay_pods; } \
    && observer_render
}

up() {
  if [ -z "$tag" ] && [ -z "${OBSERVER_IMAGE:-}" ]; then
    tag=$(newest_release) && [ -n "$tag" ] || die "cannot read the newest release with gh; give --tag TAG"
  fi
  observer_image "$tag"
  step "the pull Secret of $image" pull_secret
  step "the ConfigMap" observer_config
  step "the Secret of the key" observer_key "$palette_key"
  step "the first-seen records" records_first_seen
  step "the manifests" render
  step "apply the manifests" kc apply -f observer.yaml
  step "restart the pod" kc -n "$ns" rollout restart deploy/strata-observer
  step "wait for the pod" observer_ready
  cat << EOF

The observer runs in the cluster $cluster, with the image $image.
To open the page, run one of these commands, and then open http://127.0.0.1:$port:
  just observer-open $cluster
  kubectl --kubeconfig $KUBE_FILE -n $ns port-forward deploy/strata-observer $port:8080
To remove the observer: just observer-down $cluster
EOF
}

# ---- open and down ----------------------------------------------------------------------------------

open_page() {
  kc -n "$ns" get deploy/strata-observer -o name > /dev/null 2>&1 \
    || die "no observer in the cluster $cluster; run just observer-up $cluster"
  pf_start || { cat port-forward.log; die "no port-forward to the observer"; }
  echo "The page of the observer is at http://127.0.0.1:$port. Press Ctrl-C to close it."
  wait "$pf_pid"
}

# down: the namespace holds the pod, the Secrets, and the ConfigMap. The cluster role and its binding are
# not in a namespace, so a delete of the namespace keeps them. They carry the label of the observer.
down() {
  KUBE_TIMEOUT=200 step "delete the namespace $ns" kc delete namespace "$ns" --ignore-not-found --timeout=180s
  step "delete the cluster role and its binding" kc delete clusterrole,clusterrolebinding -l "$selector" --ignore-not-found
}

choose_cluster
write_kubeconfig
echo "cluster: $cluster ($C), project $PALETTE_PROJECT_UID; kubeconfig: $KUBE_FILE"
case "$mode" in
  up) up ;;
  open) open_page ;;
  down) down ;;
esac

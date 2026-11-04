#!/usr/bin/env bash
# The observer on the real cluster fred (#656). Stage 2 installs the manifests of deploy/observer
# with a plain kubectl apply. Stage 6 installs them from a profile in Palette.
#
#   examples/live/observer.sh [FROM_STAGE]
#
#   1  the records and the settings: the pull Secret of the image, a record of cluster A and of each
#      profile that it runs, the namespace, the ConfigMap, the Secret of the key, and the records of
#      the Git revision that is checked out. When the token file fails, the run stops at the pull
#      Secret, before it changes the cluster
#   2  way 1, a plain kubectl apply: the manifests, with the image, the pull Secret, and two short
#      periods set in an overlay. The pod is admitted, Ready, and hardened, and its first cycle
#      reaches Palette. At the baseline the page holds no finding. Its status of each profile is
#      the status of strata verify and strata compare at a terminal
#   3  who can open the page: a user with only the reader role, a user with no role, and a pod of
#      another namespace
#   4  a real change: the requests of the cycles at the baseline, an edit in place of the add-on
#      version, the finding with its actor, the copy of the cluster that stays the same, the value
#      of a variable that no route and no log shows, the requests with the finding, and the undo of
#      the edit
#   5  failure and restart: a wrong key, the key back, a restart of the pod, and the removal of the
#      manifests
#   6  way 2, a profile in Palette: an add-on profile with the rendered manifests as one manifest
#      layer, added to cluster A, and then taken off
#   7  the end: the profile, the namespace, and the probe pod go, and no key or token is in a file
#      of the run
#
# The first cycle of the observer reads Palette from a node of cluster A. So stage 2 shows whether a
# workload cluster reaches the API. The key is never in a profile and never in a file of the run. It
# goes to its Secret through standard input. The pull Secret comes from a token file, and it goes the
# same way. The file holds a token with only the scope read:packages. Whoever reads the Secret in the
# cluster has each scope of that token. So a token of a person, with more scopes, must not go there. A
# person makes the file. The script never prints it, and it has no fallback to `gh auth token`. It
# refuses a file that is absent, empty, or open to a group or to others. The kubeconfig lives in a
# temporary file that the script deletes at its end. A check with `?` records a result without judging
# it.
#
# A failed reaches-palette ends the run before stage 4, because the later stages need the page. The
# undo runs at the end, also after a failed check or an error. The edit in place, the profile of way
# 2, the namespace, and the probe pod go. The script deletes no cluster. KEEP=1 skips the undo, so
# that FROM_STAGE can resume the run.
#
# Needs the live example at its baseline with cluster A Running (just live-up) and the strata release
# binary. It also needs kubectl, curl, jq, gh, git, go, the key of verify.sh, and the token file.
# GHCR_PULL_TOKEN_FILE names the token file (default: ~/.config/strata/ghcr-pull-token, mode 0600).
# OBSERVER_IMAGE names the image (default: the image of the version in Cargo.toml). PORT is the local
# port of the port-forward (default 18080). CYCLE_TRIES is the number of looks, 5 seconds apart, in a wait for the page
# (default 60). Results go under runs/<UTC time>/observer/, or OUT; FROM_STAGE resumes a run in OUT.
# Exit 0 only when no check fails.
set -uo pipefail
script=observer
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4|5|6|7) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"
for tool in kubectl curl jq gh git go timeout; do
  command -v "$tool" > /dev/null || die "$tool is needed"
done

root=$(cd "$live/../.." && pwd)
version=$(grep -m1 '^version' "$root/Cargo.toml" | cut -d'"' -f2)
image=${OBSERVER_IMAGE:-ghcr.io/como-technologies/strata-observer:v$version}
case "$image" in
  *@sha256:*) image_name=${image%%@*}; image_pin="digest: ${image#*@}" ;;
  *:*) image_name=${image%:*}; image_pin="newTag: ${image##*:}" ;;
  *) die "OBSERVER_IMAGE needs a tag or a digest" ;;
esac
case "${image##*:}" in */*) die "OBSERVER_IMAGE needs a tag or a digest" ;; esac
port=${PORT:-18080}
tries=${CYCLE_TRIES:-60}
pull_file=${GHCR_PULL_TOKEN_FILE:-$HOME/.config/strata/ghcr-pull-token}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/observer}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
C=$(out cluster_uid)
AD=$(out addon_profile_uid)
addon=$(out addon_profile_name)
fred=$(out cluster_names | jq -r '.a // empty')
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
export STRATA_BIN=$strata strata C
records_script=$root/scripts/ci/observer-records.sh
revision=$(git -C "$root" rev-parse HEAD)
palette_settings

ns=strata-observer
selector=app.kubernetes.io/name=strata-observer
profile_name=strata-observer-live
reader_user=strata-observer-reader
nobody_user=strata-observer-nobody
wrong_key=strata-observer-wrong-key
target=$API
case "$target" in http://*|https://*) ;; *) target="https://$target" ;; esac
target=${target%/}

# What the run changes. ids.env keeps it, so that a resumed run knows it.
OP= edited= attached=
save() { echo "$1=$2" >> ids.env; }
[ -f ids.env ] && . ./ids.env
kubeconfig= pf_pid= pf_user= no_palette= undone=

# kc ARGS...: kubectl in cluster A. kj does the same with the errors on standard error, for JSON.
kc() { kube "$C" "$@"; }
kj() { KUBE_STDERR=1 kube "$C" "$@"; }

# poll COUNT COMMAND...: run COMMAND until it succeeds, at most COUNT times, 5 seconds apart
poll() {
  local count=$1 _
  shift
  for _ in $(seq "$count"); do
    "$@" && return 0
    sleep 5
  done
  return 1
}

# ---- the page ------------------------------------------------------------------------------------------

# pf_stop: end the port-forward
pf_stop() {
  [ -z "$pf_pid" ] || { kill "$pf_pid" 2> /dev/null; wait "$pf_pid" 2> /dev/null; }
  pf_pid= pf_user=
}
# pf_start [USER]: open kubectl port-forward deploy/strata-observer, as USER when it is given. The
# kubeconfig is a credential: it lives in a temporary file until the end of the run.
pf_start() {
  local _
  pf_stop
  if [ ! -s "$kubeconfig" ]; then
    kubeconfig=$(mktemp) || return 1
    if ! "$live/api.sh" get-kubeconfig "$C" > "$kubeconfig" 2> /dev/null || [ ! -s "$kubeconfig" ]; then
      rm -f "$kubeconfig"
      kubeconfig=
      return 1
    fi
  fi
  kubectl --kubeconfig "$kubeconfig" ${1:+--as="$1"} -n "$ns" port-forward deploy/strata-observer "$port:8080" > port-forward.log 2>&1 < /dev/null &
  pf_pid=$! pf_user=${1:-}
  for _ in $(seq 30); do
    grep -q 'Forwarding from' port-forward.log && return 0
    # the process can end after it prints its line, so the log gets a last look
    kill -0 "$pf_pid" 2> /dev/null || { grep -q 'Forwarding from' port-forward.log; return; }
    sleep 1
  done
  return 1
}
# pf_ensure: a port-forward as the admin of the cluster, to the pod that runs now
pf_ensure() {
  [ -n "$pf_pid" ] && [ -z "$pf_user" ] && kill -0 "$pf_pid" 2> /dev/null && return 0
  pf_start
}
# page PATH: the answer of the observer through the port-forward
page() { curl -fsS --max-time 20 "http://127.0.0.1:$port$1"; }
# page_is STATE: the page has the state STATE. page.json keeps /v1/cluster.
page_is() { page /v1/cluster > page.json 2> /dev/null && jq -e --arg s "$1" '.page == $s' page.json > /dev/null; }
# page_ok: the page is ok, and a content cycle ended well
page_ok() { page_is ok && jq -e '.content.checkedAt != null' page.json > /dev/null; }
# no_findings: /v1/findings is empty. findings.json keeps it.
no_findings() { page /v1/findings > findings.json 2> /dev/null && jq -e '.findings | length == 0' findings.json > /dev/null; }

# ---- the install -------------------------------------------------------------------------------------

# snapshot_profiles: a record of each profile that cluster A runs, in record-<n>.json
snapshot_profiles() {
  local n=0 uid
  jq -r '.source.cluster.profiles[].uid' cluster.json > uids.txt && [ -s uids.txt ] || return 1
  while read -r uid; do
    n=$((n + 1))
    "$strata" snapshot "$uid" -o "record-$n.json" -q || return 1
  done < uids.txt
}
# pull_token: the pull token on standard output, from the token file. The file holds a token with only
# the scope read:packages. When the file is absent, empty, or open to a group or to others, the
# function prints one line on standard error and fails.
pull_token() {
  local token mode
  token=$(tr -d '[:space:]' 2> /dev/null < "$pull_file")
  if [ -z "$token" ]; then
    echo "no token in $pull_file: it must hold a token with only the scope read:packages" >&2
    return 1
  fi
  mode=$(stat -c %a "$pull_file" 2> /dev/null || stat -f %Lp "$pull_file") \
    || { echo "cannot read the mode of $pull_file" >&2; return 1; }
  if [ $((8#$mode & 077)) != 0 ]; then
    echo "$pull_file has the mode $mode: run chmod 600 on it. It must hold a token with only the scope read:packages" >&2
    return 1
  fi
  printf %s "$token"
}
# pull_secret: the Secret ghcr-pull from the token file. The token goes through standard input. The
# function reads the file first, so a file that fails changes nothing in the cluster. The Secret
# needs the namespace, so the function applies it.
pull_secret() {
  local token user
  token=$(pull_token) || return 1
  kc apply -f "$root/deploy/observer/namespace.yaml" || return 1
  user=$(gh api user --jq .login 2> /dev/null)
  printf %s "$token" \
    | jq -Rs --arg u "${user:-strata-observer}" \
      '. as $t | {auths: {"ghcr.io": {username: $u, password: $t, auth: ("\($u):\($t)" | @base64)}}}' \
    | kc -n "$ns" create secret generic ghcr-pull --type=kubernetes.io/dockerconfigjson --from-file=.dockerconfigjson=/dev/stdin
}
# records_apply: the records of this revision. A client-side apply fails above 256 KiB, so it is --server-side.
records_apply() { "$records_script" --revision "$revision" record-*.json | kc apply --server-side -f -; }
# prepare_settings: the pull Secret, the namespace, and what the pod reads. Stage 1 runs each step as
# a check. The pull Secret comes first, because it reads the token file before any change.
prepare_settings() {
  pull_secret && observer_config && observer_key "$palette_key" && records_apply
}
# overlay: base/ is a copy of deploy/observer. kustomization.yaml sets the image and the pull Secret,
# and gives the two timers short periods, so that the run takes minutes.
overlay() {
  mkdir -p base && cp "$root"/deploy/observer/*.yaml base/ || return 1
  cat > kustomization.yaml << EOF
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
resources:
  - base
images:
  - name: ghcr.io/como-technologies/strata-observer
    newName: $image_name
    $image_pin
patches:
  - target:
      kind: Deployment
      name: strata-observer
    patch: |-
      - op: add
        path: /spec/template/spec/imagePullSecrets
        value:
          - name: ghcr-pull
      - op: add
        path: /spec/template/spec/containers/0/args/-
        value: --content-every=1m
      - op: add
        path: /spec/template/spec/containers/0/args/-
        value: --versions-every=5m
EOF
}
# render: observer.yaml, the plainest install that a person can run
render() { overlay && kubectl kustomize . > observer.yaml && [ -s observer.yaml ]; }

# ---- the checks of way 1 -----------------------------------------------------------------------------

pod_exists() { kj -n "$ns" get pod -l "$selector" -o json | jq -e '.items | length > 0' > /dev/null; }
# apply_admitted: the namespace enforces the restricted standard, and the standard admits the pod
apply_admitted() {
  kj get namespace "$ns" -o json | jq -e '.metadata.labels["pod-security.kubernetes.io/enforce"] == "restricted"' > /dev/null || return 1
  poll 24 pod_exists && return 0
  # the ReplicaSet says why no pod exists, for example a refusal of the standard
  kj -n "$ns" get replicaset -o json | jq -r '.items[].status.conditions[]? | select(.type == "ReplicaFailure") | .message'
  return 1
}
# pod_ready: the Deployment is Available in 3 minutes, and the probe of the image exits 0 in the pod
pod_ready() {
  KUBE_TIMEOUT=200 kc -n "$ns" wait --for=condition=Available deploy/strata-observer --timeout=180s || return 1
  kc -n "$ns" exec deploy/strata-observer -- /usr/local/bin/strata-observer health
}
# pod_hardened: the spec of the pod, as the cluster holds it. pod.json keeps it.
pod_hardened() {
  kj -n "$ns" get pod -l "$selector" -o json > pod.json || return 1
  jq -e '.items[0].spec | .securityContext.runAsNonRoot == true and .automountServiceAccountToken == false
    and .containers[0].securityContext.readOnlyRootFilesystem == true
    and .containers[0].securityContext.allowPrivilegeEscalation == false
    and .containers[0].securityContext.capabilities.drop == ["ALL"]
    and ([.volumes[]?.name | select(startswith("kube-api-access"))] | length == 0)' pod.json
}
# reaches_palette: the first content cycle ends well, so the pod reaches Palette. When it does not,
# the output names lastError, the kind and the status of the failed cycle.
reaches_palette() {
  pf_start || { echo "no port-forward: $(cat port-forward.log)"; return 1; }
  if poll "$tries" page_ok; then
    jq '{page, checkedAt: .content.checkedAt}' page.json
    return 0
  fi
  jq '{page, lastError: .content.lastError, failedRuns: .content.failedRuns}' page.json 2> /dev/null \
    || echo '{"lastError": "the page gave no answer"}'
  return 1
}
# summary_matches_strata: the status of each record on the page is the status of strata at a terminal.
# The published side is the verdict of verify, and the copy of the cluster is the answer of compare.
summary_matches_strata() {
  local rec name version shown_pub shown_copy want_pub want_copy bad=0
  page /v1/profiles > profiles.json || return 1
  for rec in record-*.json; do
    name=$(jq -r .source.name "$rec") version=$(jq -r .source.version "$rec")
    shown_pub=$(jq -r --arg n "$name" --arg v "$version" '.profiles[] | select(.name == $n and .version == $v) | .published.status' profiles.json)
    shown_copy=$(jq -r --arg n "$name" --arg v "$version" '.profiles[] | select(.name == $n and .version == $v) | .clusterCopy.status' profiles.json)
    want_pub=$("$strata" verify "$rec" --summary --json --compact -q | jq -r '.records[0].status')
    want_copy=$("$strata" compare "$rec" "cluster:$C/$name" --json --compact -q | jq -r 'if .same then "same" elif .sameContent then "same-content" else "differs" end')
    printf '%s@%s: page %s and %s, strata %s and %s\n' "$name" "$version" "$shown_pub" "$shown_copy" "$want_pub" "$want_copy"
    [ "$shown_pub" = "$want_pub" ] && [ "$shown_copy" = "$want_copy" ] || bad=1
  done
  return "$bad"
}

# ---- the checks of who can open the page -------------------------------------------------------------

# reader_role: the reader role, bound to a user
reader_role() {
  kc apply -f "$root/deploy/observer/reader-role.yaml" \
    && kc -n "$ns" create rolebinding reader --role=strata-observer-reader --user="$reader_user"
}
# reader_can_open: the user opens the page with kubectl port-forward, and the page names the cluster
reader_can_open() {
  pf_start "$reader_user" || { cat port-forward.log; return 1; }
  [ "$(page /healthz)" = ok ] && page /v1/cluster | jq -e --arg c "$C" '.cluster.uid == $c' > /dev/null
}
# can_i ARGS...: yes or no, as kubectl auth can-i gives it
can_i() { kc -n "$ns" auth can-i "$@" | tail -n 1; }
# reader_cannot_read: the user reads no Secret and runs no exec
reader_cannot_read() {
  [ "$(can_i get secrets --as="$reader_user")" = no ] \
    && [ "$(can_i create pods --subresource=exec --as="$reader_user")" = no ]
}
# nobody_cannot_open: a user with no role opens no port-forward
nobody_cannot_open() { [ "$(can_i create pods --subresource=portforward --as="$nobody_user")" = no ]; }
# probe_refused: the check of lib.sh, with the time that a pull of the test image needs
probe_refused() { KUBE_TIMEOUT=180 other_pod_refused; }

# ---- the checks of a real change ---------------------------------------------------------------------

# content_cycle: the strata commands of one content cycle of the observer. The counter runs it in a
# directory of its own, which holds the raw answers with the values of the variables, and removes it.
content_cycle() {
  local dir rec n=0
  dir=$(mktemp -d) || return 1
  trap 'rm -rf "$dir"' RETURN
  cp record-*.json "$dir" && cd "$dir" || return 1
  "$strata" verify record-*.json --summary --who --json --compact -q > /dev/null
  [ $? -le 2 ] || return 1
  "$strata" snapshot "cluster:$C" --raw-dir raw -o cluster.json -q || return 1
  for rec in record-*.json; do
    n=$((n + 1))
    "$strata" snapshot "cluster:$C/$(jq -r .source.name "$rec")" --from-raw raw -o "side-$n.json" -q || return 1
    "$strata" compare "$rec" "side-$n.json" --json --compact -q > /dev/null
    [ $? -le 1 ] || return 1
  done
}
# versions_behind: the first command of a versions cycle. behind.json keeps its answer.
versions_behind() {
  "$strata" behind record-*.json --no-impact --json --compact -q > behind.json
  [ $? -le 2 ]
}
# versions_moves: the second command of a versions cycle, for each pin that is behind. The first cycle
# of a pod runs it for each such pin, and a later cycle runs it for a new move only.
versions_moves() {
  local a b
  while read -r a b; do
    "$strata" compare "$a" "$b" --json --compact -q > /dev/null
    [ $? -le 1 ] || return 1
  done < <(jq -r '.pins[] | select(.behind and .error == null and (.newer | length) > 0)
    | "pack:\(.registryUid)/\(.name)@\(.version) pack:\(.registryUid)/\(.name)@\(.newer[0].version)"' behind.json)
}
export -f content_cycle versions_behind versions_moves
# requests_of NAME FUNCTION: run FUNCTION behind the counter. NAME.log holds one line for each request
# that Palette got, with the method and the path, and no query and no key.
requests_of() { ./count-requests -target "$target" -log "$1.log" -- bash -c "$2" > "$1.run" 2>&1; }
# count_requests: the requests of a content cycle, of the first command of a versions cycle, and of
# its moves. The routes that took most follow the count of each.
count_requests() {
  local name
  [ -x ./count-requests ] || go -C "$live/count-requests" build -o "$PWD/count-requests" . || return 1
  requests_of content content_cycle && requests_of behind versions_behind && requests_of moves versions_moves \
    || return 1
  for name in content behind moves; do
    printf '%s: %s requests\n' "$name" "$(wc -l < "$name.log" | tr -d ' ')"
  done
  for name in content behind moves; do
    echo "-- the routes of $name"
    sed -E 's#/[0-9a-f]{24}#/{uid}#g' "$name.log" | sort | uniq -c | sort -rn | head -n 12
  done
}
# drift_edit: an edit in place of the published add-on version. The flag comes first, so that the undo
# knows of the edit also when the call fails.
drift_edit() {
  edited=1 && save edited 1
  "$live/api.sh" edit "$AD" "$addon_from" "$addon_to"
}
# drift_undo: restore the add-on version
drift_undo() { "$live/api.sh" edit "$AD" "$addon_to" "$addon_from" && edited= && save edited ""; }
# drift_seen: the finding published-changed names the add-on profile and its pack. findings.json keeps it.
drift_seen() {
  page /v1/findings > findings.json 2> /dev/null \
    && jq -e --arg p "$addon@1.0.0" '.findings[] | select(.kind == "published-changed" and .profile == $p and (.packs | length) > 0)' findings.json > /dev/null
}
# who_seen: the finding names the actor of the write
who_seen() {
  page /v1/findings > findings.json 2> /dev/null \
    && jq -e --arg p "$addon@1.0.0" '.findings[] | select(.kind == "published-changed" and .profile == $p) | .who | length > 0 and all(.actor | length > 0)' findings.json > /dev/null
}
# drift_who: the finding names the actor. The output shows who.
drift_who() {
  poll "$tries" who_seen || return 1
  jq -c '.findings[] | select(.kind == "published-changed") | .who[] | {actor, actorType, action}' findings.json
}
# cluster_copy_same: the copy of the cluster says same, because the cluster did not take the update
cluster_copy_same() { page "/v1/profiles/$addon@1.0.0" | jq -e '.clusterCopy.status == "same"'; }
# variable_value: the first value of a variable in the record of the cluster
variable_value() {
  jq -r '[.source.cluster.variables[]? | .value | select(. != null and . != "") | tostring] | first // empty' cluster.json
}
# variable_in_raw: the raw output of strata compare, between the record of the cluster and the cluster,
# holds the value. So the grep of no_value_leaves proves something.
variable_in_raw() {
  local value
  value=$(variable_value)
  [ -n "$value" ] || { echo "the record of the cluster holds no value of a variable"; return 1; }
  "$strata" compare cluster.json "cluster:$C" --json --compact -q > raw-compare.json
  [ $? -le 1 ] || return 1
  grep -qF -- "$value" raw-compare.json
}
# leak_paths: each route of the page, and the detail route of each pack that a finding names
leak_paths() {
  printf '%s\n' / /healthz /v1/cluster /v1/findings /v1/profiles /v1/newer /v1/notchecked /v1/events
  jq -r '.findings[] | .profile as $p | .packs[] | "/v1/packs/\($p)/\(.)/diff?side=published", "/v1/packs/\($p)/\(.)/diff?side=cluster"' findings.json
}
# no_value_leaves: no route and no line of the log holds the value of the variable
no_value_leaves() {
  local value path body routes=0
  value=$(variable_value)
  [ -n "$value" ] || return 1
  while IFS= read -r path; do
    routes=$((routes + 1))
    body=$(curl -sS --max-time 60 "http://127.0.0.1:$port$path" 2> /dev/null)
    case "$body" in
      *"$value"*) echo "the route $path shows the value"; return 1 ;;
    esac
  done < <(leak_paths)
  body=$(kc -n "$ns" logs deploy/strata-observer --tail=5000)
  case "$body" in
    *"$value"*) echo "the log shows the value"; return 1 ;;
  esac
  echo "$routes routes and the log hold no value of a variable"
}
# drift_clears: after the undo, the finding goes in the next cycle
drift_clears() { poll "$tries" no_findings; }

# ---- the checks of failure and restart ---------------------------------------------------------------

# restart_pod: restart the Deployment and wait for the new pod. The port-forward ends with the old pod.
restart_pod() {
  pf_stop
  kc -n "$ns" rollout restart deploy/strata-observer \
    && KUBE_TIMEOUT=200 kc -n "$ns" rollout status deploy/strata-observer --timeout=180s
}
# replace_key KEY: put KEY in the Secret of the key, restart the pod, and open the port-forward. The
# Secret goes through pipes, never through a file or an argument.
replace_key() {
  printf %s "$1" \
    | kj -n "$ns" create secret generic strata-observer-key --from-file=api-key=/dev/stdin --dry-run=client -o yaml \
    | kc apply -f - \
    && restart_pod && pf_start
}
# wrong_key_set and right_key_set: the two puts of the key
wrong_key_set() { replace_key "$wrong_key"; }
right_key_set() { replace_key "$palette_key"; }
page_lost() { page_is lost; }
# lost_after_wrong_key: after two failed cycles the page is lost
lost_after_wrong_key() { poll "$tries" page_lost; }
# ok_after_key_back: with the key back, the page is ok again
ok_after_key_back() { poll "$tries" page_ok; }
# later A B: the time B is after the time A. Both are RFC 3339 times, with or without a fraction.
later() {
  jq -en --arg a "$1" --arg b "$2" '($b | sub("\\.[0-9]+"; "") | fromdate) > ($a | sub("\\.[0-9]+"; "") | fromdate)' > /dev/null
}
# restart_keeps_working: after a restart, the page goes from first-load to ok. The start of its state
# is later, because the state goes with the pod. pages-after-restart.txt lists the states.
restart_keeps_working() {
  local before after state _
  page /v1/cluster | jq -r '.records.startedAt' > started-before.txt || return 1
  restart_pod && pf_start || return 1
  : > pages-after-restart.txt
  for _ in $(seq "$tries"); do
    state=$(page /v1/cluster 2> /dev/null | tee page.json | jq -r '.page // empty')
    if [ -n "$state" ] && [ "$state" != "$(tail -n 1 pages-after-restart.txt)" ]; then
      echo "$state" >> pages-after-restart.txt
    fi
    [ "$state" = ok ] && break
    sleep 2
  done
  [ "$(tail -n 1 pages-after-restart.txt)" = ok ] || return 1
  later "$(cat started-before.txt)" "$(jq -r '.records.startedAt' page.json)"
}
# first_seen: the first sight of the finding published-changed, in findings.json
first_seen() { jq -r '[.findings[] | select(.kind == "published-changed") | .firstSeen] | first // empty' findings.json; }
# first_seen_starts_again: the pod keeps the first sight of a finding in its state, and the state goes
# with the pod. So a finding that lives through a restart has a later first sight. The edit goes back at the end.
first_seen_starts_again() {
  local before after bad=0
  drift_edit && poll "$tries" drift_seen || bad=1
  if [ "$bad" = 0 ]; then
    before=$(first_seen)
    restart_pod && pf_start && poll "$tries" drift_seen || bad=1
    after=$(first_seen)
  fi
  drift_undo || bad=1
  [ "$bad" = 0 ] || return 1
  echo "before the restart: $before"
  echo "after the restart:  $after"
  later "$before" "$after"
}
# restart_first_load: the page showed first-load after the restart
restart_first_load() { grep -qx first-load pages-after-restart.txt; }
# manifests_delete: kubectl delete -f observer.yaml, which holds the Namespace
manifests_delete() { KUBE_TIMEOUT=300 kc delete -f observer.yaml; }
# not_found ARGS...: kubectl in cluster A says that ARGS name nothing
not_found() {
  local answer
  answer=$(kc "$@")
  case "$answer" in
    *"not found"*) return 0 ;;
  esac
  return 1
}
namespace_gone() { not_found get namespace "$ns"; }
# removal_is_clean: the Namespace is gone, and no object with the label of the observer stays
removal_is_clean() {
  local left
  pf_stop
  poll 36 namespace_gone || return 1
  left=$(kj get namespaces,deployments,pods,secrets,configmaps,serviceaccounts,networkpolicies,roles,rolebindings -A -l "$selector" -o name) \
    || return 1
  [ -z "$left" ] || { echo "$left"; return 1; }
}

# ---- the checks of way 2, a profile in Palette -------------------------------------------------------

# profile_create: an add-on profile with the rendered manifests as one manifest layer. The key is not
# in it: the Secret of the key is made with kubectl. An old profile of this name goes first.
profile_create() {
  [ -s observer.yaml ] || render || return 1
  "$live/api.sh" delete-named "$profile_name" > /dev/null || return 1
  OP=$("$live/api.sh" manifest-profile "$profile_name" strata-observer observer.yaml) && [ -n "$OP" ] || return 1
  save OP "$OP"
  echo "$OP"
}
# profile_attach: add the profile to cluster A, and wait for the cluster
profile_attach() {
  attached=1 && save attached 1
  "$live/api.sh" attach "$C" "$OP" && wait_running "$C"
}
# deployment_available: the Deployment has its replica
deployment_available() { kj -n "$ns" get deployment strata-observer -o json | jq -e '.status.availableReplicas == 1' > /dev/null; }
# profile_applied: Palette applies the layer within 20 minutes, and the pod is Ready
profile_applied() {
  poll 240 deployment_available || return 1
  KUBE_TIMEOUT=140 kc -n "$ns" wait --for=condition=Ready pod -l "$selector" --timeout=120s
}
# other_profiles_same: each other profile of cluster A still agrees with its record
other_profiles_same() {
  local rec name bad=0
  for rec in record-*.json; do
    name=$(jq -r .source.name "$rec")
    if "$strata" compare "$rec" "cluster:$C/$name" --json --compact -q | jq -e '.same or .sameContent' > /dev/null; then
      echo "$name: same"
    else
      echo "$name: differs"
      bad=1
    fi
  done
  return "$bad"
}
# own_profile_not_checked: the page shows the profile of the observer as no-record under "Not checked",
# and shows no finding for it
own_profile_not_checked() {
  local subject=$profile_name@1.0.0
  pf_start && poll "$tries" page_ok || return 1
  page /v1/profiles | jq -e --arg n "$profile_name" '.profiles[] | select(.name == $n and .status == "no-record")' > /dev/null || return 1
  page /v1/notchecked | jq -e --arg s "$subject" '.notChecked[] | select(.cause == "no-record" and (.profiles | index($s) != null))' > /dev/null || return 1
  page /v1/findings | jq -e --arg s "$subject" '[.findings[] | select(.profile == $s)] | length == 0' > /dev/null
}
# record_has_key FILE: FILE holds the key. The key goes to grep through a pipe, never through an argument.
record_has_key() { grep -qF -f <(printf %s "$palette_key") "$1"; }
# record_nodes: the machines of cluster A, in nodes-before.txt
record_nodes() { instances > nodes-before.txt && [ -s nodes-before.txt ]; }
# same_nodes: cluster A has the machines that it had before the layer
same_nodes() {
  local now
  now=$(instances)
  echo "before: $(cat nodes-before.txt)"
  echo "after:  $now"
  [ "$now" = "$(cat nodes-before.txt)" ]
}
# profile_detach: take the profile off cluster A, and wait for the cluster
profile_detach() {
  "$live/api.sh" detach "$C" "$OP" && attached= && save attached "" && wait_running "$C"
}
deployment_gone() { not_found -n "$ns" get deployment strata-observer; }
# profile_removed: the Deployment goes within 20 minutes
profile_removed() { poll 240 deployment_gone; }

# ---- the end -----------------------------------------------------------------------------------------------

# undo: return cluster A to its baseline. It is safe to run again. The edit in place, the profile of way
# 2, the namespace, and the probe pod go. No cluster goes.
undo() {
  pf_stop
  if [ -n "$edited" ]; then
    "$live/api.sh" edit "$AD" "$addon_to" "$addon_from" >> steps.log 2>&1 && edited= && save edited ""
  fi
  if [ -n "$attached" ]; then
    "$live/api.sh" detach "$C" "$OP" >> steps.log 2>&1 && attached= && save attached ""
    # a subshell, because wait_running stops the script when the cluster does not get there
    (wait_running "$C") >> steps.log 2>&1
  fi
  if [ -n "$OP" ] && [ -z "$attached" ]; then
    "$live/api.sh" delete "$OP" >> steps.log 2>&1 && OP= && save OP ""
  fi
  KUBE_TIMEOUT=300 kc delete namespace "$ns" --ignore-not-found >> steps.log 2>&1
  kc -n default delete pod observer-probe --ignore-not-found >> steps.log 2>&1
  undone=1
}
# profile_gone: no profile of way 2 stays in Palette
profile_gone() {
  [ -z "$OP" ] && [ "$("$strata" list profiles --json | jq --arg n "$profile_name" '[.profiles[] | select(.name == $n)] | length')" = 0 ]
}
# secret_in_run: a file of the run holds the key, the pull token, or the token of gh. The token of gh
# never goes to the cluster, but no file of the run must hold it either. Each secret goes to grep
# through a pipe. The function skips an empty one, because an empty pattern matches each line.
secret_in_run() {
  local secret found=1
  for secret in "$palette_key" "$(pull_token 2> /dev/null)" "$(gh auth token 2> /dev/null)"; do
    [ -n "$secret" ] || continue
    grep -rIlF -f <(printf %s "$secret") .
    case $? in 0) found=0 ;; 1) ;; *) return 2 ;; esac
  done
  return $found
}

summary() {
  write_summary "the observer on cluster A, ${fred:-fred}" ", cluster \`$C\`, image \`$image\`, revision \`$revision\`."
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

# skip_without_page: say why a stage waits when the page did not answer
skip_without_page() {
  [ -z "$no_palette" ] && return 0
  echo "reaches-palette failed: this stage needs the page, so it does not run" | tee -a steps.log
  return 1
}

if stage 1 "the records and the settings"; then
  require_baseline
  check pull-secret 0 '#656: the Secret ghcr-pull holds the token of the file, which has only read:packages and comes through standard input' -- \
    pull_secret
  if [ "$failures" != 0 ]; then
    # pull_secret reads the file before any change, so a file that fails leaves nothing to undo
    pull_token > /dev/null 2>&1 || undone=1
    { cat pull-secret.err; echo "the run stops: the pull Secret needs the token file"; } | tee -a steps.log
    exit 1
  fi
  check cluster-record 0 '#656: a record of cluster A. Only the check of the variables and the last verify read it' -- \
    "$strata" snapshot "cluster:$C" -o cluster.json
  check profile-records 0 '#656: a record of each profile that cluster A runs, in record-<n>.json' -- snapshot_profiles
  check namespace 0 '#656: the namespace with the restricted standard' -- \
    kc apply -f "$root/deploy/observer/namespace.yaml"
  check config 0 '#656: the ConfigMap holds the cluster, the address of Palette, and the project' -- observer_config
  check key 0 '#656: the Secret holds the key, which comes through standard input' -- observer_key "$palette_key"
  check records-apply 0 '#656: the records of this revision apply with --server-side' -- records_apply
fi

if stage 2 "way 1, a plain kubectl apply"; then
  check manifests-render 0 '#656: kubectl kustomize renders the overlay: the image, the pull Secret, and the two periods' -- render
  check manifests-apply 0 '#656: kubectl apply -f observer.yaml, the plainest install' -- kc apply -f observer.yaml
  check apply-admitted 0 '#656: the namespace enforces restricted, and the pod is admitted' -- apply_admitted
  check pod-ready 0 '#656: the Deployment is Available in 3 minutes, and strata-observer health exits 0 through kubectl exec' -- pod_ready
  check pod-hardened 0 '#656: runAsNonRoot, a read-only root, no capability, and no volume of a token' -- pod_hardened
  failed_before=$failures
  check reaches-palette 0 '#656: through port-forward, /v1/cluster gives page ok and a checkedAt: a workload cluster reaches the API' -- reaches_palette
  [ "$failures" = "$failed_before" ] || no_palette=1
  if skip_without_page; then
    check no-finding-at-baseline 0 '#656: /v1/findings is empty for the records that the run made' -- no_findings
    check summary-matches-strata 0 '#656: the status of each profile agrees with strata verify and strata compare at a terminal' -- \
      summary_matches_strata
  fi
fi

if stage 3 "who can open the page"; then
  check reader-role 0 '#656: the reader role, bound to a user' -- reader_role
  check reader-can-open 0 '#656: a user with only the reader role opens the page with kubectl port-forward' -- reader_can_open
  check reader-cannot-read 0 '#656: the same user gets no for get secrets and for create pods --subresource=exec' -- reader_cannot_read
  check nobody-cannot-open 0 '#656: a user with no role gets no for create pods --subresource=portforward' -- nobody_cannot_open
  check other-pod-refused 0 '#656: a pod of another namespace cannot connect to the pod address on the port' -- probe_refused
fi

if stage 4 "a real change" && skip_without_page; then
  check page-admin 0 '#656: the port-forward is back, as the admin of the cluster' -- pf_ensure
  check requests-counted ? '#656: the requests of a content cycle and of a versions cycle at the baseline (requests-counted.out)' -- count_requests
  check drift-edit 0 "#656: edit $addon in place through the API" -- drift_edit
  check drift-found 0 '#656: within two content cycles, /v1/findings holds published-changed with the pack' -- poll "$tries" drift_seen
  check drift-who 0 '#656: the finding names the actor of the write' -- drift_who
  check cluster-copy-unchanged 0 '#656: the copy of the cluster still says same, because the cluster did not take the update' -- cluster_copy_same
  check variable-in-raw 0 '#656: the raw output of strata compare holds the value of a variable' -- variable_in_raw
  check no-value-leaves 0 '#656: no route and no line of the log holds that value' -- no_value_leaves
  check requests-counted-drift ? '#656: the same count with the drift, where verify --who reads the audit log (requests-counted-drift.out)' -- \
    count_requests
  check drift-undo 0 "#656: restore $addon" -- drift_undo
  check drift-clears 0 '#656: the finding goes in the next cycle' -- drift_clears
fi

if stage 5 "failure and restart" && skip_without_page; then
  check page-admin-again 0 '#656: the port-forward as the admin of the cluster' -- pf_ensure
  check wrong-key 0 '#656: a wrong key in the Secret, and a restart of the pod' -- wrong_key_set
  check lost-after-wrong-key 0 '#656: the page goes to lost after two cycles' -- lost_after_wrong_key
  check key-back 0 '#656: the key back, and a restart of the pod' -- right_key_set
  check ok-after-key-back 0 '#656: the page returns to ok' -- ok_after_key_back
  check restart-keeps-working 0 '#656: after kubectl rollout restart the page goes to ok, and the start of its state is later' -- restart_keeps_working
  check restart-first-load ? '#656: the page showed first-load after the restart (pages-after-restart.txt)' -- restart_first_load
  check first-seen-starts-again 0 '#656: a finding that lives through a restart has a later firstSeen, because the state goes with the pod' -- first_seen_starts_again
  check manifests-delete 0 '#656: kubectl delete -f observer.yaml' -- manifests_delete
  check removal-is-clean 0 '#656: the namespace is gone, and no object of the observer stays' -- removal_is_clean
  check fred-unchanged 0 '#656: strata verify says that cluster A did not change' -- "$strata" verify cluster.json
fi

if stage 6 "way 2, a profile in Palette" && skip_without_page; then
  check profile-prepare 0 '#656: the namespace and the Secrets, made with kubectl before the profile' -- prepare_settings
  check nodes-before 0 '#656: the machines of cluster A before the layer' -- record_nodes
  check profile-create 0 '#656: an add-on profile with the rendered manifests as one manifest layer' -- profile_create
  check profile-attach 0 '#656: the profile goes to cluster A, and the cluster is Running' -- profile_attach
  check profile-applied 0 '#656: Palette applies the layer, and the pod is Ready' -- profile_applied
  check profile-does-not-disturb 0 '#656: each other profile of cluster A still agrees with its record' -- other_profiles_same
  check own-profile-not-checked 0 '#656: the profile of the observer shows as no-record under Not checked, and as no finding' -- own_profile_not_checked
  check profile-record 0 '#656: a record of the profile of the observer' -- "$strata" snapshot "$OP" -o observer-profile.json -q
  check no-key-in-profile 1 '#656: the record of the profile does not hold the key' -- record_has_key observer-profile.json
  check no-repave ? '#656: cluster A keeps its machines when the layer goes on: 0 says it does, 1 says Palette replaced them' -- same_nodes
  check profile-detach 0 '#656: the profile comes off cluster A, and the cluster is Running' -- profile_detach
  check profile-removal 0 '#656: the Deployment goes' -- profile_removed
fi

if stage 7 "the end" && [ "${KEEP:-}" != 1 ]; then
  undo
  check undo-profile 0 '#656: no profile of way 2 stays in Palette' -- profile_gone
  check undo-namespace 0 '#656: the namespace is gone' -- namespace_gone
  check no-secret-in-run 1 '#656: no file of the run holds the key, the pull token, or the token of gh' -- secret_in_run
  check undo-baseline 0 'the example is at its baseline' -- "$live/baseline.sh"
fi

[ "${KEEP:-}" != 1 ] || echo "KEEP=1: the install, the profile, and the edit stay; FROM_STAGE resumes the run" | tee -a steps.log

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

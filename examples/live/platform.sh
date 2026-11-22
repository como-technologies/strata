#!/usr/bin/env bash
# The platform cluster wilma (#700). It runs the cluster profile of the lab and seven add-on
# profiles with 14 community packs, one profile for each team of a platform group. The cases ask
# how drift and diff behave where the profiles cross. terraform-platform/ holds wilma in a state
# apart from the live example, so a normal live window costs the same as before.
#
#   examples/live/platform.sh pins        write terraform-platform/terraform.tfvars from the tenant
#   examples/live/platform.sh up          build wilma, and wait until each pack is ready
#   examples/live/platform.sh [N]         run case N, or each case from 1 to 8
#   examples/live/platform.sh down        remove wilma, its profiles, its zone, and its policy
#   examples/live/platform.sh inventory   what wilma still has in AWS: 0 on each line after down
#
#   1  an edit in one profile: verify gives a finding for that record only
#   2  one pack in two profiles: what Palette answers, and what strata says
#   3  a pack that moves to another profile: the roots, same-release, and compare
#   4  a variable of four profiles that the cluster changes: snapshot, compare, and the observer
#   5  behind at scale: the requests, the time, and the pins behind
#   6  the glue breaks: the CRD of a Cluster goes with the profile of its operator
#   7  the observer at scale: the requests of a cycle, the memory, the text limit, and the images
#   8  the Bitnami images: the image that each Bitnami chart names, and what the nodes run
#
# pins reads the registry of each pack, and the mark `# behind`, from terraform.tfvars.example. It
# pins the newest version in the order of strata, or the version before it for a pack with the mark.
# up and down show their plans and ask before they change anything. Each case gives wilma its
# baseline again at its end, also after a failed check: the defaults of terraform-platform/.
# KEEP=1 skips that undo, for a person who reads the state of wilma after a failed check.
#
# Needs `just live-profiles` or `just live-up` first, for the release profile of the lab. Needs
# tofu, jq, curl, kubectl, go, the AWS CLI, and the key of Palette in PALETTE_API_KEY or
# ~/.palette.env. Case 4 and case 7 install the observer with observer-up.sh, which needs gh or
# OBSERVER_IMAGE. EDIT_FROM and EDIT_TO give the edit of case 1, and PROMETHEUS_FROM and
# PROMETHEUS_TO the edit of case 7. Results go under runs/<UTC time>/platform/, or OUT. Exit 0 only
# when no check fails.
set -uo pipefail
script=platform
flag=false
mode=cases from=1 to=8
case "${1:-}" in
  pins | up | down | inventory) mode=$1 ;;
  "") ;;
  [1-8]) from=$1 to=$1 ;;
  -h | --help) mode=help ;;
  *) mode=refused ;;
esac
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$mode" in
  help) usage; exit 0 ;;
  refused) usage >&2; exit 2 ;;
esac
for tool in jq curl; do
  command -v "$tool" > /dev/null || die "$tool is needed"
done

platform=$live/terraform-platform
pins_file=$platform/terraform.tfvars
# The state of wilma is apart from the live example. Each plan, apply, destroy, and console of it
# reads the names of the tenant from the file of the lab.
export TF_DIR=$platform
for command in plan apply destroy console; do
  export "TF_CLI_ARGS_$command=-var-file=$live/terraform/terraform.tfvars"
done
export STRATA_QUIET=1

# ---- the pins -------------------------------------------------------------------------------------

# The version to pin in the registry $r, from the listing of strata packs. The listing gives the
# versions newest first, in the order of strata. The pin is the version that strata marks latest. With
# the mark behind, it is the first version after it that Spectro Cloud did not deprecate or disable.
pick_jq='.registries[] | select(.registry == $r) | .versions as $v | ($v | map(.latest) | index(true)) as $i
  | select($i != null)
  | (if $m == "behind" then [$v[($i + 1):][] | select(.state == null)][0] else $v[$i] end)
  | select(. != null) | "\(.version) \(.type)"'

# pins: write the file of the pins from the example and the tenant. It keeps a file that exists.
pins() {
  local line name registry mark listing version type
  [ ! -e "$pins_file" ] || die "$pins_file exists: remove it, and pins writes it again"
  {
    echo "# The pins of wilma. examples/live/platform.sh pins wrote them from the tenant."
    echo "packs = {"
  } > "$pins_file.new" || exit 2
  while IFS= read -r line; do
    name=$(sed -n 's/^ *\([a-z0-9-]*\) *= *{ *registry *= *"[^"]*".*/\1/p' <<<"$line")
    [ -n "$name" ] || continue
    registry=$(sed -n 's/^ *[a-z0-9-]* *= *{ *registry *= *"\([^"]*\)".*/\1/p' <<<"$line")
    case "$line" in *"# behind"*) mark=behind ;; *) mark= ;; esac
    version= type=
    listing=$("$strata" packs "$name" --cloud aws --json -q) || { rm -f "$pins_file.new"; die "strata packs $name failed"; }
    read -r version type < <(jq -r --arg r "$registry" --arg m "$mark" "$pick_jq" <<<"$listing")
    if [ -z "${version:-}" ]; then
      rm -f "$pins_file.new"
      die "the registry $registry gives no version of $name to pin${mark:+ one behind}; the registries of $name: $(jq -r '[.registries[].registry] | join(", ")' <<<"$listing")"
    fi
    printf '  %s = { registry = "%s", version = "%s", type = "%s" }%s\n' "$name" "$registry" "$version" "$type" \
      "${mark:+ # behind}" >> "$pins_file.new"
    echo "$name: $version from $registry${mark:+, the version before the newest}"
  done < "$platform/terraform.tfvars.example"
  echo "}" >> "$pins_file.new" && mv "$pins_file.new" "$pins_file"
}

# behind_pins: the name of each pack that pins one version behind
behind_pins() { sed -n 's/^ *\([a-z0-9-]*\) *= *{.*# behind *$/\1/p' "$pins_file"; }

# ---- the outputs and the records ------------------------------------------------------------------

# platform_outputs: read the outputs of wilma, and set W, R, name, and the project
platform_outputs() {
  load_outputs
  W=$(out cluster_uid)
  R=$(out release_profile_uid)
  name=$(out cluster_name)
  PALETTE_PROJECT_UID=$(out project_uid)
  for v in W R name PALETTE_PROJECT_UID; do
    [ -n "${!v}" ] || die "the tofu output behind $v is empty; run examples/live/platform.sh up"
  done
  export PALETTE_PROJECT_UID
  C=$W
  export STRATA_BIN=$strata strata C
}
# teams: the teams of the seven profiles, one on each line
teams() { out profile_uids | jq -r 'keys[]'; }
# team_uid TEAM: the uid of version 1.0.0 of the profile of TEAM
team_uid() { out profile_uids | jq -r --arg t "$1" '.[$t] // empty'; }
# records_made: a record of the release profile, of each profile of a team, and of wilma
records_made() {
  local team
  mkdir -p records || return 1
  "$strata" snapshot "$R" -o records/release.json -q || return 1
  for team in $(teams); do
    "$strata" snapshot "$(team_uid "$team")" -o "records/$team.json" -q || return 1
  done
  "$strata" snapshot "cluster:$W" -o records/cluster.json -q
}
# records_ready: the records of the baseline, made once in a run. A run that resumes keeps them.
records_ready() {
  [ -f records/cluster.json ] && return 0
  check records 0 '#700: a record of the release, of each profile of a team, and of wilma, at the baseline' -- records_made
}
# verify_teams TEAM...: verify the record of each TEAM, one at a time. The output names the exit of each.
verify_teams() {
  local team code bad=0
  for team in "$@"; do
    "$strata" verify "records/$team.json" > "verify-$team.txt" 2>&1
    code=$?
    echo "$team: verify exits $code"
    [ "$code" = 0 ] || bad=1
  done
  return "$bad"
}
# other_teams TEAM: each team other than TEAM, and the release
other_teams() { { teams | grep -vx "$1"; echo release; } | paste -sd ' ' -; }

# ---- the readiness of the packs -------------------------------------------------------------------

# One line for each pack of the profiles in $want, from the cluster document: ready, waiting, or
# missing, with the reason of Palette. A pack is ready when its condition has the status True.
ready_jq='(.status.packs // []) as $have | $want[] | . as $w
  | ([$have[] | select(.name == $w.name and .profileUid == $w.uid)] | first) as $p
  | if $p == null then "missing \($w.name): Palette reports no status for it"
    elif ($p.condition.status // "") == "True" then "ready \($w.name): \($p.condition.reason // "")"
    else "waiting \($w.name): \($p.condition.reason // $p.condition.status // "no condition") \($p.condition.message // "" | .[:160])"
    end'
# packs_ready: each pack of each add-on profile that wilma runs is ready. ready.txt keeps the lines.
packs_ready() {
  local uid
  "$live/api.sh" get "/v1/spectroclusters/$W" > ready-cluster.json || return 1
  for uid in $(out on_cluster | jq -r '.[]'); do
    "$live/api.sh" get "/v1/clusterprofiles/$uid" | jq -c --arg u "$uid" '.spec.published.packs[] | {uid: $u, name}'
  done > ready-expected.jsonl || return 1
  jq -r --slurpfile want ready-expected.jsonl "$ready_jq" ready-cluster.json > ready.txt || return 1
  [ -s ready.txt ] && ! grep -qv '^ready ' ready.txt
}
# wait_ready: wait up to 45 minutes until each pack is ready. The output is the last reading.
wait_ready() {
  local _
  for _ in $(seq 1 "${READY_TRIES:-135}"); do
    packs_ready && { cat ready.txt; return 0; }
    sleep 20
  done
  cat ready.txt 2> /dev/null
  return 1
}

# ---- the applies of the cases ---------------------------------------------------------------------

# dirty: a case changed a variable of wilma. ids.env keeps it, so that a run that resumes knows it.
dirty= edited_security= edited_prometheus= observer=
save() { echo "$1=$2" >> ids.env; }
# applied ARG...: an apply with the variables ARG. The output is the lines of the plan, of the end, and of
# each error. It returns the exit of tofu. The apply runs in this shell, so that it reads the outputs again.
applied() {
  local rc
  try_apply "$@" > case-apply.txt
  rc=$?
  grep -E '^(Plan|Apply complete|Error)|Code|code' case-apply.txt | head -n 20
  return "$rc"
}
# apply_case ARG...: an apply with the variables of one case
apply_case() {
  dirty=1 && save dirty 1
  applied "$@"
}
# refused_apply CODE ARG...: an apply that Palette must refuse with the error code CODE
refused_apply() {
  local code=$1 rc
  shift
  dirty=1 && save dirty 1
  try_apply "$@" > refused-apply.txt
  rc=$?
  grep -E 'Error|Code|code' refused-apply.txt | head -n 20
  [ "$rc" != 0 ] && grep -q "$code" refused-apply.txt
}
# apply_defaults: the defaults of terraform-platform/, which are the baseline of wilma
apply_defaults() { applied && dirty= && save dirty ""; }
# baseline_ready: the defaults, and each pack is ready
baseline_ready() { apply_defaults && wait_ready; }

# ---- the cases ------------------------------------------------------------------------------------

edit_from=${EDIT_FROM:-releaseNameOverride: \"cert-manager\"}
edit_to=${EDIT_TO:-releaseNameOverride: \"cert-manager-edited\"}
prometheus_from=${PROMETHEUS_FROM:-retention: 10d}
prometheus_to=${PROMETHEUS_TO:-retention: 9d}

# edit_security: an edit in place of a value of cert-manager in the profile of the security team
edit_security() {
  edited_security=1 && save edited_security 1
  "$live/api.sh" edit "$(team_uid security)" "$edit_from" "$edit_to"
}
undo_security() { "$live/api.sh" edit "$(team_uid security)" "$edit_to" "$edit_from" && edited_security= && save edited_security ""; }

# other_cert_manager: a version of cert-manager in the registry of its pin, other than the pin. It is
# the first such version in the order of strata, so the newest one.
other_cert_manager() {
  "$strata" packs cert-manager --cloud aws --json -q | jq -r --argjson p "$(out packs)" \
    '[.registries[] | select(.registry == $p["cert-manager"].registry) | .versions[]
      | select(.state == null and .version != $p["cert-manager"].version)][0].version // empty'
}
# move_refused: Palette refuses the move of wilma from wilma-network 1.0.0 to 1.1.0 with HTTP 500 and
# DuplicateClusterPacksForbidden. move.txt keeps the status and the answer.
move_refused() {
  "$live/api.sh" replace-status "$W" "$(team_uid network)" "$NEXT" > move.txt || return 1
  cat move.txt
  refused 500 DuplicateClusterPacksForbidden move.txt
}
# names_duplicate FILE...: a file holds the code DuplicateClusterPacksForbidden and the pack cert-manager
names_duplicate() { cat "$@" | grep -q DuplicateClusterPacksForbidden && cat "$@" | grep -q cert-manager; }

# record_moved: the records of the two profiles after the move of fluentbit, and of wilma
record_moved() {
  "$strata" snapshot "$(out moved_uids | jq -r .observability)" -o records/observability-moved.json -q \
    && "$strata" snapshot "$(out moved_uids | jq -r .logging)" -o records/logging.json -q \
    && "$strata" snapshot "cluster:$W" -o records/cluster-moved.json -q
}
# move_roots: the root of each record of the move, and each node of fluentbit in it
move_roots() {
  local f
  for f in records/observability.json records/observability-moved.json records/logging.json \
    records/cluster.json records/cluster-moved.json; do
    jq -r --arg f "$f" '"\($f): root \(.root)",
      ([.. | objects | select((.key? // "") | endswith("/fluentbit"))][] | "  \(.key) \(.version) node \(.hash.node)")' "$f" \
      || return 1
  done
}

# domain_variables VALUE: the variables route of wilma gives VALUE to the variable domain of each of
# the four profiles. The output is each variable domain with its value.
domain_variables() {
  "$live/api.sh" get "/v1/spectroclusters/$W/variables" > domain-variables.json || return 1
  jq -c '[.. | objects | select(.name? == "domain") | {name, value}]' domain-variables.json
  jq -e --arg v "$1" --argjson n "$(out domain_teams | jq length)" \
    '[.. | objects | select(.name? == "domain" and .value? == $v)] | length == $n' domain-variables.json > /dev/null
}

# observer_on [--read-pods]: install the observer on wilma, or repair it. The trap removes it.
observer_on() {
  observer=1 && save observer 1
  "$live/observer-up.sh" up --cluster "$name" "$@"
}
observer_off() { "$live/observer-up.sh" down --cluster "$name" && observer= && save observer ""; }
ns=strata-observer
port=${PORT:-18080}
# observer_after_cycles: the page of the four profiles after two content cycles of one minute
observer_after_cycles() {
  sleep 150
  pf_start || return 1
  page /v1/profiles > profiles.json || return 1
  jq -c --argjson t "$(out domain_teams)" --arg n "$name" \
    '.profiles[] | select(.name as $p | $t | map("\($n)-\(.)") | index($p | split("@")[0])) | {name, status, clusterCopy: .clusterCopy.status}' profiles.json
  page /v1/findings | jq -c '.findings[] | {kind, profile, packs}'
}

# behind_scale: behind on the records of the seven profiles, behind the counter. The output gives
# the requests, the seconds, the pins and the pins behind, and the routes that took most.
behind_scale() {
  local start code records
  counter_built || return 1
  records=$(for team in $(teams); do printf 'records/%s.json ' "$team"; done)
  start=$SECONDS
  # shellcheck disable=SC2086
  ./count-requests -target "$target" -log behind-scale.log -- "$strata" behind $records --json -q > behind-scale.json
  code=$?
  echo "behind exits $code after $((SECONDS - start)) seconds, with $(wc -l < behind-scale.log | tr -d ' ') requests"
  jq -r '"\(.pins | length) pins, \([.pins[] | select(.behind)] | length) behind"' behind-scale.json
  sed -E 's#/[0-9a-f]{24}#/{uid}#g' behind-scale.log | sort | uniq -c | sort -rn | head -n 12
  [ "$code" -le 1 ]
}
# behind_rows: behind names a newer version for each pack that pins one version behind
behind_rows() {
  local pack bad=0
  for pack in $(behind_pins); do
    if jq -e --arg n "$pack" '[.pins[] | select(.name == $n and .behind and (.newer | length) > 0)] | length > 0' \
      behind-scale.json > /dev/null; then
      echo "$pack: behind"
    else
      echo "$pack: no newer version"
      bad=1
    fi
  done
  return "$bad"
}

# identity_status: what Palette reports for the packs of wilma-identity, 10 minutes after the change
identity_status() {
  sleep 600
  "$live/api.sh" get "/v1/spectroclusters/$W" | jq -c --arg u "$(team_uid identity)" \
    '.status.packs[]? | select(.profileUid == $u) | {name, condition}'
}

# cycles_counted: the requests of the cycles of the observer on wilma, with the records of the
# baseline, in the folder cycles/
cycles_counted() {
  local n=0 f
  mkdir -p cycles || return 1
  for f in records/release.json $(for team in $(teams); do printf 'records/%s.json ' "$team"; done); do
    n=$((n + 1))
    cp "$f" "cycles/record-$n.json" || return 1
  done
  (cd cycles && count_requests)
}
# The host of the registry of an image reference. The first part of a reference names a host when a
# slash follows it and it holds a dot or a colon, or is localhost. Each other reference is of docker.io.
host_jq='def host: split("/") as $p | if ($p | length) > 1 and ($p[0] | test("[.:]") or . == "localhost") then $p[0] else "docker.io" end;'
# images_by_registry: the images that the pods of wilma run, by the host of the registry, and each
# image that matches no pack
images_by_registry() {
  pf_start && poll 60 images_read || return 1
  jq -r "$host_jq"' .images | group_by(.ref | host)[]
    | "\(.[0].ref | host): \(length) images, \(map(select(.match == "none")) | length) with no pack"' images.json
  jq -r '.images[] | select(.match == "none") | "no pack: \(.ref)"' images.json
}
# prometheus_edit: an edit in place of a value of prometheus-operator in the profile of observability
prometheus_edit() {
  edited_prometheus=1 && save edited_prometheus 1
  "$live/api.sh" edit "$(team_uid observability)" "$prometheus_from" "$prometheus_to"
}
prometheus_undo() {
  "$live/api.sh" edit "$(team_uid observability)" "$prometheus_to" "$prometheus_from" && edited_prometheus= \
    && save edited_prometheus ""
}
# text_limit: the detail route of the diff of prometheus-operator, after the finding of the edit. The
# output gives textTooLarge and the bytes of the text of each side.
text_limit() {
  local profile=$name-observability@1.0.0
  pf_start || return 1
  poll 60 finding_of "$profile" || return 1
  detail_page "/v1/packs/$profile/addon/prometheus-operator/diff?side=published" > prometheus-diff.json || return 1
  jq -c '{textTooLarge, values: (.values.unified // "" | length), manifests: [.manifests[]? | (.unified // "" | length)]}' \
    prometheus-diff.json
}
# finding_of PROFILE: /v1/findings holds a finding published-changed of PROFILE
finding_of() {
  page /v1/findings > findings.json 2> /dev/null \
    && jq -e --arg p "$1" '.findings[] | select(.kind == "published-changed" and .profile == $p)' findings.json > /dev/null
}
# observer_memory: the working set of the container of the observer, from the summary of the kubelet
# of its node, beside the limit of the pod
observer_memory() {
  local pod node
  observer_pod > observer-pod.json || return 1
  pod=$(jq -r '.items[0].metadata.name' observer-pod.json)
  node=$(jq -r '.items[0].spec.nodeName' observer-pod.json)
  kj get --raw "/api/v1/nodes/$node/proxy/stats/summary" > node-summary.json || return 1
  jq -r --arg p "$pod" '.pods[] | select(.podRef.name == $p) | .containers[]
    | "\(.name): working set \(.memory.workingSetBytes) bytes"' node-summary.json
  jq -r '.items[0].spec.containers[] | "\(.name): limit \(.resources.limits.memory // "none")"' observer-pod.json
}
# observer_no_oom: no container of the observer restarted, and none ended with OOMKilled
observer_no_oom() {
  observer_pod > observer-pod.json || return 1
  jq -c '.items[0].status.containerStatuses[] | {name, restartCount, last: .lastState.terminated.reason}' observer-pod.json
  jq -e '[.items[0].status.containerStatuses[] | select(.restartCount > 0 or .lastState.terminated.reason == "OOMKilled")]
    | length == 0' observer-pod.json > /dev/null
}

# bitnami_images: for each pack of a Bitnami registry, the lines of its values that name an image. Then
# the image and the state of each container of its pods on wilma.
bitnami_images() {
  local pack team
  kj get pods -A -o json > pods.json || return 1
  for pack in $(out packs | jq -r 'to_entries[] | select(.value.registry | test("bitnami"; "i")) | .key'); do
    team=$(out teams | jq -r --arg p "$pack" 'to_entries[] | select(.value | index($p)) | .key')
    echo "== $pack, in $name-$team"
    "$live/api.sh" get "/v1/clusterprofiles/$(team_uid "$team")" \
      | jq -r --arg p "$pack" '.spec.published.packs[] | select(.name == $p) | .values' \
      | grep -n -E '^ *(registry|repository|tag): ' | head -n 30
    jq -r --arg p "$pack" '.items[] | select(.metadata.labels["app.kubernetes.io/instance"] == $p
        or .metadata.labels["app.kubernetes.io/name"] == $p)
      | .metadata.namespace as $ns | (.status.containerStatuses // [])[]
      | "  \($ns) \(.image) ready=\(.ready) \(.state.waiting.reason // "")"' pods.json
  done
}

# ---- up, down, and the inventory ------------------------------------------------------------------

# up: build wilma, and wait until each pack is ready. The apply asks first, as live-up does.
up() {
  [ -f "$pins_file" ] || die "no pins: run examples/live/platform.sh pins first"
  "$live/tf.sh" init -input=false || exit 2
  "$live/tf.sh" apply || { "$live/diagnose.sh" "$PWD/diagnostics" >&2; exit 1; }
  platform_outputs
  wait_running "$W"
  check packs-ready 0 '#700: each pack of each add-on profile of wilma is ready' -- wait_ready
}

# no_addon_left: the cluster document lists no pack of an add-on profile. The output counts them.
no_addon_left() {
  local n
  n=$("$live/api.sh" get "/v1/spectroclusters/$W" | jq --arg r "$R" '[.status.packs[]? | select(.profileUid != $r)] | length') \
    || return 1
  echo "$n packs of an add-on profile"
  [ "$n" = 0 ]
}
# no_volume_left: no persistent volume is left in wilma
no_volume_left() { [ "$(kj get pv -o json | jq '.items | length')" = 0 ]; }

# down: remove the add-on profiles from wilma, so that their workloads free the claims of their
# volumes. Delete each claim while the CSI driver runs, so that it deletes each volume. Then destroy,
# delete each version of each profile name, and read the inventory.
down() {
  local names
  platform_outputs
  names=$(out profile_names | jq -r '.[]')
  echo "== the add-on profiles leave $name" | tee -a steps.log
  tf_apply false -var addons=false
  poll 120 no_addon_left || echo "packs of an add-on profile are still on $name; the destroy continues" | tee -a steps.log
  echo "== each claim of a volume goes, while the CSI driver runs" | tee -a steps.log
  KUBE_TIMEOUT=600 kc delete pvc --all -A --wait=true | tee -a steps.log
  poll 60 no_volume_left || echo "persistent volumes are still in $name: the inventory counts their volumes" | tee -a steps.log
  echo "== the destroy" | tee -a steps.log
  "$live/tf.sh" destroy || "$live/tf.sh" destroy || { "$live/diagnose.sh" "$PWD/diagnostics" >&2; exit 1; }
  # shellcheck disable=SC2086
  "$live/api.sh" delete-named $names
  inventory
}

# inventory: what wilma still has in AWS. inventory.sh reads the variables and not the outputs, so it
# works after down.
inventory() { "$live/inventory.sh" --platform; }

case "$mode" in
  pins) pins; exit ;;
  inventory) inventory; exit ;;
esac

case "$mode" in cases) folder=platform ;; *) folder=platform-$mode ;; esac
run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/$folder}
mkdir -p "$run" && cd "$run" || exit 2
[ -f ids.env ] && . ./ids.env

case "$mode" in
  up)
    up
    write_summary "the platform cluster up" ", cluster \`$name\` \`$W\`."
    [ "$failures" = 0 ]
    exit
    ;;
  down) down; exit ;;
esac

palette_settings
palette_target
platform_outputs

# undo: give wilma its baseline again, and remove what a case left
undo() {
  [ -z "$edited_security" ] || undo_security >> steps.log 2>&1
  [ -z "$edited_prometheus" ] || prometheus_undo >> steps.log 2>&1
  [ -z "$dirty" ] || apply_defaults >> steps.log 2>&1
  [ -z "$observer" ] || observer_off >> steps.log 2>&1
}
summary() {
  write_summary "the platform cluster $name" ", cluster \`$W\`, $(teams | wc -l | tr -d ' ') profiles of the teams."
}
finish() {
  pf_stop
  [ -z "$kubeconfig" ] || rm -f "$kubeconfig"
  [ "${KEEP:-}" = 1 ] || undo
  summary
}
trap finish EXIT

# case_stage N TITLE: the heading of case N, when the run holds case N
case_stage() { [ "$1" -le "$to" ] && stage "$@"; }

# require_platform: stop unless wilma is at its baseline: a plan with the defaults changes nothing
require_platform() {
  tf_plan false > baseline-before.txt 2>&1 \
    || die "wilma is not at its baseline; see $PWD/baseline-before.txt (platform.sh down and up build it again)"
}
require_platform

if case_stage 1 "an edit in one profile"; then
  records_ready
  check verify-before 0 '#700: verify passes on each record of wilma before the edit' -- verify_teams $(teams) release
  check edit-security 0 "#700: an edit in place of a value of cert-manager in $name-security, through the API" -- edit_security
  check verify-security 1 "#700: verify gives a finding for the record of $name-security" -- "$strata" verify records/security.json
  check verify-others 0 '#700: verify passes on the record of each of the other six profiles, and of the release' -- \
    verify_teams $(other_teams security)
  check verify-cluster ? '#700: verify of the record of wilma: what strata says while the edit waits for the cluster' -- \
    "$strata" verify records/cluster.json
  check undo-security 0 '#700: the undo of the edit' -- undo_security
  check verify-after 0 '#700: verify passes on each record again' -- verify_teams $(teams) release
fi

if case_stage 2 "one pack in two profiles"; then
  records_ready
  other=$(other_cert_manager)
  [ -n "$other" ] || die "the registry of cert-manager gives no second version"
  failed_before=$failures
  check network-next 0 "#700: $name-network 1.1.0 adds cert-manager $other, and no cluster runs it" -- \
    apply_case -var "network_cert_manager=$other"
  NEXT=$(out network_next_uid)
  if [ "$failures" = "$failed_before" ] && [ -n "$NEXT" ]; then
    check in-place-refused 0 "#700: Palette refuses an edit in place that adds cert-manager to $name-network 1.0.0, which $name runs: ClusterProfilePacksInConflict" -- \
      refused_apply ClusterProfilePacksInConflict -var "network_cert_manager=$other" -var network_cert_manager_in_place=true
    check in-place-unchanged 0 "#700: verify passes on the record of $name-network: the refusal changed nothing" -- \
      "$strata" verify records/network.json
    check record-network-next 0 "#700: a record of $name-network 1.1.0" -- "$strata" snapshot "$NEXT" -o records/network-next.json -q
    check validate-move ? "#700: compare --validate of the move of $name to $name-network 1.1.0, with the answer of Palette last" -- \
      "$strata" compare "cluster:$W" "$NEXT" --validate
    check validate-names-duplicate 0 '#700: that answer names DuplicateClusterPacksForbidden and cert-manager' -- \
      names_duplicate validate-move.out validate-move.err
    check move-refused 0 "#700: Palette refuses the move of $name to $name-network 1.1.0: HTTP 500 and DuplicateClusterPacksForbidden" -- \
      move_refused
    check behind-network ? "#700: behind of the record of $name-network: 1.1.0, and the answer of Palette under bump to" -- \
      "$strata" behind records/network.json
  fi
  check network-next-gone 0 "#700: $name-network 1.1.0 goes" -- apply_defaults
fi

if case_stage 3 "a pack that moves to another profile"; then
  records_ready
  failed_before=$failures
  check move-fluentbit 0 "#700: $name runs $name-observability 1.1.0 with no fluentbit, and $name-logging with fluentbit alone" -- \
    apply_case -var move_fluentbit=true
  if [ "$failures" = "$failed_before" ]; then
    check move-ready ? '#700: each pack after the move, with its condition' -- wait_ready
    check record-moved 0 "#700: records of $name-observability 1.1.0, of $name-logging, and of $name after the move" -- record_moved
    check move-roots ? '#700: the root of each record, and the node of fluentbit in each' -- move_roots
    check move-same-release ? "#700: same-release of $name-observability 1.0.0 and 1.1.0" -- \
      "$strata" same-release records/observability.json records/observability-moved.json
    check move-compare-cluster ? "#700: compare of the record of $name before the move with cluster:$name" -- \
      "$strata" compare records/cluster.json "cluster:$W"
    check move-compare-records ? "#700: compare of the records of $name before and after the move" -- \
      "$strata" compare records/cluster.json records/cluster-moved.json
  fi
  check move-back 0 "#700: $name runs the baseline again, and each pack is ready" -- baseline_ready
fi

if case_stage 4 "a variable of four profiles"; then
  records_ready
  value=edited.$(out domain)
  check domain-observer-on 0 "#700: the observer runs on $name" -- observer_on
  failed_before=$failures
  check domain-set 0 "#700: $name gives the variable domain the value $value in the four profiles" -- \
    apply_case -var "cluster_domain=$value"
  if [ "$failures" = "$failed_before" ]; then
    check domain-variables 0 "#700: the variables route of $name gives $value to the variable domain of each of the four profiles" -- \
      domain_variables "$value"
    check domain-record 0 "#700: a record of $name with the new value" -- "$strata" snapshot "cluster:$W" -o records/cluster-domain.json -q
    check domain-compare ? "#700: compare of the records of $name before and after, for the four profiles" -- \
      "$strata" compare records/cluster.json records/cluster-domain.json
    check domain-verify ? '#700: verify of the records of the four profiles: their content did not change' -- \
      verify_teams $(out domain_teams | jq -r '.[]')
    check domain-observer ? '#700: what the page of the observer says of the four profiles after two content cycles' -- \
      observer_after_cycles
  fi
  check domain-back 0 "#700: $name gives the default again" -- apply_defaults
  check domain-observer-off 0 "#700: the observer leaves $name" -- observer_off
fi

if case_stage 5 "behind at scale"; then
  records_ready
  check behind-scale ? '#700: behind on the records of the seven profiles: the requests, the seconds, and the pins behind' -- behind_scale
  check behind-rows 0 '#700: behind names a newer version for each pin that pins wrote one version behind' -- behind_rows
fi

if case_stage 6 "the glue breaks"; then
  records_ready
  failed_before=$failures
  check data-off 0 "#700: $name-data leaves $name, and cloudnative-pg with it" -- apply_case -var data_profile=false
  if [ "$failures" = "$failed_before" ]; then
    check data-crd ? '#700: kubectl reads the CRD of a Cluster of cloudnative-pg, or says that it is gone' -- \
      kc get crd clusters.postgresql.cnpg.io
    check data-identity-content 0 "#700: verify passes on the record of $name-identity: its content did not change" -- \
      "$strata" verify records/identity.json
    check data-record 0 "#700: a record of $name with no $name-data" -- "$strata" snapshot "cluster:$W" -o records/cluster-no-data.json -q
    check data-compare ? "#700: compare of the records of $name before and after" -- \
      "$strata" compare records/cluster.json records/cluster-no-data.json
    check data-pack-status ? "#700: what Palette reports for the packs of $name-identity, 10 minutes after" -- identity_status
  fi
  check data-back 0 "#700: $name runs $name-data again, and each pack is ready" -- baseline_ready
fi

if case_stage 7 "the observer at scale"; then
  records_ready
  check observer-pods 0 "#700: the observer runs on $name with the read of the pods" -- observer_on --read-pods
  check observer-requests ? "#700: the requests of a content cycle and of a versions cycle on $name (observer-requests.out)" -- \
    cycles_counted
  check observer-images ? '#700: the images that the pods run, by the host of the registry, and the images with no pack' -- \
    images_by_registry
  check prometheus-edit 0 '#700: an edit in place of a value of prometheus-operator, whose values are large' -- prometheus_edit
  check text-limit ? '#700: the detail route of the diff of prometheus-operator: textTooLarge, and the bytes of the text' -- \
    text_limit
  check prometheus-undo 0 '#700: the undo of the edit' -- prometheus_undo
  check observer-memory ? '#700: the working set of the observer beside its limit' -- observer_memory
  check observer-no-oom 0 '#700: no container of the observer restarted, and none ended with OOMKilled' -- observer_no_oom
  check observer-off 0 "#700: the observer leaves $name" -- observer_off
fi

if case_stage 8 "the Bitnami images"; then
  check bitnami-images ? '#700: for each Bitnami chart, the lines of its values that name an image, and what its pods run' -- \
    bitnami_images
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

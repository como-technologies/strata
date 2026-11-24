#!/usr/bin/env bash
# The boundary of a project (#701). wilma of platform.sh is in the project of the lab, and the
# workload cluster pebbles is in a second project. Both clusters have the label hub=wilma, and both run
# the profile of the tenant wilma-shared. pebbles also runs a copy of wilma-data and of wilma-control.
#
#   examples/live/platform-boundary.sh [N]   run case N, or each case
#
#   1  one profile in two projects: same-release of a ref in each project, and consumers of the
#      profile of the tenant, with the key of the lab and with the key of one project
#   2  a change on wilma that pebbles needs: Vault on wilma moves to another version, and the answer
#      of each cluster after the move
#   3  the observer of the set on wilma: each cluster in its project, and the profile of the tenant on
#      2 clusters in 2 projects. Then the key of the second project goes from Palette
#   4  the requests of one cycle of the observer of the set, with two clusters and two keys
#
# Case 3 deletes the key of the second project, and case 4 needs it. So a run of each case runs case 4
# before case 3. Strata answers each question for each cluster alone, and it models no dependency
# between two clusters. So the checks of case 2 record what each answer shows, and judge nothing.
#
# The key of the lab, in PALETTE_API_KEY or ~/.palette.env, reads both projects. OBSERVER_ROLE_KEY is
# the key of a user whose one role is Project Viewer in the project of the lab, as for
# observer-clusters.sh. OBSERVER_SECOND_KEY is the key of a user whose one role is Project Viewer in
# the second project. Its name in Palette is OBSERVER_SECOND_KEY_NAME, or strata-live-second-viewer.
# Case 3 deletes it, so each run of case 3 needs a new key. A key stays in the environment of one
# command at a time, and in keys/ for one cycle of the observer. With no key, the script says "could
# not check" and exits 2.
#
# Needs `just live-platform-up` with STRATA_LIVE_SECOND_PROJECT, the strata release binary, the
# observer binary (just observer-build, or OBSERVER_BIN), tofu, go, curl, and jq. Case 2 applies
# terraform-platform/ twice: the move, and the undo. Each case ends at the baseline of the platform,
# also after a failed check, and KEEP=1 skips that undo. Results go under
# runs/<UTC time>/platform-boundary/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=platform-boundary
flag=false
mode=cases from=1 to=4
case "${1:-}" in
  "") ;;
  [1-4]) from=$1 to=$1 ;;
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
[ -x "$strata" ] || die "no strata binary at $strata"
[ -n "${OBSERVER_ROLE_KEY:-}" ] && [ -n "${OBSERVER_SECOND_KEY:-}" ] \
  || die "could not check: set OBSERVER_ROLE_KEY and OBSERVER_SECOND_KEY to the keys of two users with the role Project Viewer, one in each project"

. "$live/platform-lib.sh"
[ -n "$TF_VAR_second_project" ] || die "set STRATA_LIVE_SECOND_PROJECT to the name of the second project"
second_key_name=${OBSERVER_SECOND_KEY_NAME:-strata-live-second-viewer}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/platform-boundary}
mkdir -p "$run" && cd "$run" || exit 2
[ -f ids.env ] && . ./ids.env

palette_settings
palette_target
platform_outputs
[ -n "$P" ] && [ -n "$P2" ] || die "the state holds no pebbles; run just live-platform-up with STRATA_LIVE_SECOND_PROJECT"
lab_project=$(var palette_project)
[ -n "$lab_project" ] || die "no palette_project in terraform/terraform.tfvars"
second=$TF_VAR_second_project
selector=$(out set_selector)
shared=$(out shared_profile_name)@1.0.0

# undo: give the platform its baseline again: vault at its pin, in wilma-security 1.0.0
undo() { [ -z "$dirty" ] || baseline_ready >> steps.log 2>&1; }
summary() {
  rm -rf keys
  write_summary "the boundary of a project" ", cluster $(md span "$W") in the lab project, cluster $(md span "$P") in the second project."
}
finish() {
  [ "${KEEP:-}" = 1 ] || undo
  summary
}
trap finish EXIT

require_platform

# ---- case 1 ---------------------------------------------------------------------------------------

# The copy of wilma-data, as name@version, in each of the two projects
copy=$(out copies | jq -r '.data | "\(.name)@\(.version)"')
refs=("project:$lab_project/$copy" "project:$second/$copy")

# consumers_both FILE: the consumers of FILE are wilma in the lab project and pebbles in the second
# project
consumers_both() {
  jq -e --arg w "$W" --arg a "$PALETTE_PROJECT_UID" --arg p "$P" --arg b "$P2" \
    '[.consumers[] | select((.uid == $w and .projectUid == $a) or (.uid == $p and .projectUid == $b))] | length == 2' "$1"
}

if case_stage 1 "one profile in two projects"; then
  check same-release-lab-key 0 "#701: same-release of $copy in the lab project and in the second project, with the key of the lab" -- \
    "$strata" same-release "${refs[@]}" --json
  check same-release-sides 0 "#701: the chain names the lab project, then the second project" -- \
    jq -e --arg a "$PALETTE_PROJECT_UID" --arg b "$P2" '.same and .records[0].projectUid == $a and .records[1].projectUid == $b' \
    same-release-lab-key.out
  check same-release-role-key 2 "#701: the same same-release with the key of Project Viewer in the lab project stops" -- \
    with_role "$strata" same-release "${refs[@]}" --json
  check same-release-role-error 0 '#701: the error is project-not-found: it names the second project, and lists the lab project alone' -- \
    jq -e --arg a "$lab_project" --arg b "$second" '.error.kind == "project-not-found" and .error.name == $b and .error.available == [$a]' \
    same-release-role-key.err
  check consumers-lab-key 1 "#701: consumers of $shared, the profile of the tenant, with the key of the lab" -- \
    "$strata" consumers "$shared" --json
  check consumers-both 0 "#701: the consumers are $name in the lab project and $workload in the second project" -- \
    consumers_both consumers-lab-key.out
  check consumers-role-key ? "#701: consumers of $shared with the key of Project Viewer in the lab project" -- \
    with_role "$strata" consumers "$shared" --json
  check consumers-role-incomplete 0 '#701: the answer says that Palette refused the search at tenant scope, so a project is not checked' -- \
    jq -e '.projectsRead.tenantError != null' consumers-role-key.out
fi

# ---- case 2 ---------------------------------------------------------------------------------------

# boundary_records: a record of wilma, and of pebbles in the second project, before the move
boundary_records() {
  mkdir -p records \
    && "$strata" snapshot "cluster:$W" -o records/wilma.json -q \
    && in_second "$strata" snapshot "cluster:$P" -o records/pebbles.json -q
}

if case_stage 2 "a change on wilma that pebbles needs"; then
  check move-records 0 "#701: a record of $name, and of $workload in the second project, before the move" -- boundary_records
  other=$(other_version vault)
  [ -n "$other" ] || die "the registry of vault gives no second version"
  failed_before=$failures
  check vault-move 0 "#701: $name runs $name-security 1.1.0, with vault $other" -- apply_case -var "vault_version=$other"
  if [ "$failures" = "$failed_before" ]; then
    check vault-ready ? "#701: each pack of $name after the move, with its condition" -- wait_ready
    check vault-wilma ? "#701: compare of the record of $name before the move with cluster:$name" -- \
      "$strata" compare records/wilma.json "cluster:$W"
    check vault-workload ? "#701: compare of the record of $workload before the move with cluster:$workload" -- \
      in_second "$strata" compare records/pebbles.json "cluster:$P"
    check vault-workload-verify ? "#701: verify of the record of $workload after the move of Vault on $name" -- \
      in_second "$strata" verify records/pebbles.json
  fi
  check vault-back 0 "#701: $name runs $name-security 1.0.0 with the pin of vault again, and each pack is ready" -- baseline_ready
fi

# ---- cases 3 and 4 --------------------------------------------------------------------------------

# set_ready: the records of the observer of the set, made once in a run
set_ready() {
  [ -f records/meta.json ] && return 0
  check set-records 0 "#701: a record of each profile of $name and of $workload, each read in its project" -- \
    set_records "$W:$PALETTE_PROJECT_UID" "$P:$P2"
}
# keyed_once OUT: one cycle of the observer on wilma for the selector of the set. The key of the lab
# project is OBSERVER_ROLE_KEY, and the key of the second project OBSERVER_SECOND_KEY.
keyed_once() { set_once "$1" "$selector" OBSERVER_ROLE_KEY "$P2=OBSERVER_SECOND_KEY"; }
# set_requests: the requests of one content cycle and one versions cycle of the set, behind the
# counter. The routes that took most follow the count.
set_requests() {
  local set_wrap
  counter_built || return 1
  set_wrap=(./count-requests -target "$target" -log set-requests.log --)
  keyed_once set-requests.json || return 1
  echo "set: $(wc -l < set-requests.log | tr -d ' ') requests"
  top_routes set-requests.log
}
# refused_row FILE: in FILE, the row of pebbles is not-checked with the status of the refusal, and the
# row of wilma is checked
refused_row() {
  jq -e --arg w "$W" --arg p "$P" --arg b "$P2" '
    ([.clusters[] | select(.uid == $p and .status == "not-checked" and .lastError.status != null)] | length == 1)
    and ([.clusters[] | select(.uid == $w and .status != "not-checked" and .checkedAt != null)] | length == 1)
    and ([.projects[] | select(.uid == $b and .lastError != null)] | length == 1)' "$1" > /dev/null
}
# refused_cycles: a cycle of the observer with the deleted key, at most 10 times, 30 seconds apart,
# until the row of pebbles is not-checked. The observer exits 2 when a check fails, so the row decides.
# The output gives the row of each cluster of the last cycle.
refused_cycles() {
  local _
  for _ in $(seq 1 10); do
    keyed_once once-refused.json
    refused_row once-refused.json && break
    sleep 30
  done
  jq -c '.clusters[] | {name, project: .project.uid, status, lastError, checkedAt}' once-refused.json
}

if case_stage 4 "the request counts"; then
  [ -x "$observer_bin" ] || die "no observer binary at $observer_bin; run just observer-build"
  set_ready
  check set-requests ? '#701: the requests of one content cycle and one versions cycle of the set, with two clusters and two keys' -- \
    set_requests
fi

if case_stage 3 "the observer of the set"; then
  [ -x "$observer_bin" ] || die "no observer binary at $observer_bin; run just observer-build"
  set_ready
  check set-once 0 "#701: one cycle of the observer on $name with --clusters $selector and a key of Project Viewer for each project" -- \
    keyed_once once.json
  check set-rows 0 "#701: the row of $name names the lab project, the row of $workload the second project, and each is checked" -- \
    jq -e --arg w "$W" --arg a "$PALETTE_PROJECT_UID" --arg p "$P" --arg b "$P2" \
    '[.clusters[] | select(((.uid == $w and .project.uid == $a) or (.uid == $p and .project.uid == $b))
      and .checkedAt != null and .status != "not-checked")] | length == 2' once.json
  check set-projects 0 '#701: the projects of the summary, which the project picker of the page lists, are the two projects' -- \
    jq -e --arg a "$PALETTE_PROJECT_UID" --arg b "$P2" '[.projects[].uid] | sort == ([$a, $b] | sort)' once.json
  check set-selector 0 "#701: the selector $selector selects the two clusters" -- \
    jq -e --arg s "$selector" '.selectors == [{"selector": $s, "clusters": 2}]' once.json
  check set-shared 0 "#701: #shared shows $shared, of the tenant, on 2 clusters in 2 projects" -- \
    jq -e --arg u "$(out shared_profile_uid)" --arg a "$PALETTE_PROJECT_UID" --arg b "$P2" \
    '[.shared[] | select(.uid == $u and .scope == "tenant" and .clusters == 2 and .projects == ([$a, $b] | sort))] | length == 1' \
    once.json
  failed_before=$failures
  check key-delete 0 "#701: the key $second_key_name of the second project goes from Palette, and keys/ of each cycle still holds it" -- \
    "$live/api.sh" key-delete "$second_key_name"
  if [ "$failures" = "$failed_before" ]; then
    check set-refused ? "#701: the rows of the cycles of the observer after the delete of the key" -- refused_cycles
    check set-refused-rows 0 "#701: only $workload is not-checked, with the status, and $name stays checked" -- \
      refused_row once-refused.json
  fi
  check set-no-key 0 '#701: no file of the run holds a key of a role' -- \
    bash -c '! grep -rqF -e "$OBSERVER_ROLE_KEY" -e "$OBSERVER_SECOND_KEY" .'
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

#!/usr/bin/env bash
# The projects test (#486): one tenant with a project for each environment. A copy of the release
# profile in a second project, and what strata says about the two copies in one run.
#
#   STRATA_LIVE_SECOND_PROJECT=<name> examples/live/projects.sh
#
# The lab project is the project of the other scripts: its name is palette_project of
# terraform/terraform.tfvars, and its uid the output project_uid. STRATA_LIVE_SECOND_PROJECT is
# the name of a second project of the tenant. The script finds its uid with strata list projects.
# It creates and publishes a copy of the release profile in the second project, with the name, the
# version, the packs, the values, and the manifests of the release. It records the version in each
# project, and asks same-release and compare with a project for each side. Then it deletes the
# copy. It deletes only the profile that it makes, and never a project.
#
# Needs the profiles (just live-profiles), a second project, and no cluster: no cost. It leaves
# the tenant as it found it. Needs the strata release binary, jq, and the key as verify.sh needs
# it. Results go under runs/<UTC time>/projects/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=projects
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "${1:-}" in "") ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"

load_outputs
export PALETTE_PROJECT_UID=$(out project_uid)
A=$PALETTE_PROJECT_UID
R=$(out release_profile_uid)
NAME=$(out profile_name)
[ -n "$R" ] && [ -n "$NAME" ] || die "no release profile in the outputs; run just live-profiles"
A_NAME=$(var palette_project)
B_NAME=${STRATA_LIVE_SECOND_PROJECT:-}
[ -n "$A_NAME" ] || die "no palette_project in terraform/terraform.tfvars"
[ -n "$B_NAME" ] || die "set STRATA_LIVE_SECOND_PROJECT to the name of a second project of the tenant"
[ "$A_NAME" != "$B_NAME" ] || die "STRATA_LIVE_SECOND_PROJECT names the lab project $A_NAME; name another project"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/projects}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

B=$("$strata" list projects --json | jq -r --arg n "$B_NAME" '.projects[] | select(.name == $n) | .uid')
[ -n "$B" ] || die "the key sees no project named $B_NAME"
VERSION=$("$live/api.sh" get "/v1/clusterprofiles/$R" | jq -r '.spec.version')
[ -n "$VERSION" ] && [ "$VERSION" != null ] || die "cannot read the version of $R"
T=""

summary() {
  write_summary "projects test" ": the release" \
    "\`$NAME@$VERSION\` (\`$R\`) of the lab project, and its copy (\`${T:-none}\`) in the second project."
}
# the copy goes at the end, also when a check stops the script
cleanup() {
  [ -n "$T" ] && PALETTE_PROJECT_UID=$B "$live/api.sh" delete "$T" >> steps.log 2>&1
  summary
}
trap cleanup EXIT

sides=(--project-name-a "$A_NAME" --project-name-b "$B_NAME")

echo "== a copy of the release in the second project (#486)" | tee -a steps.log
check copy-create 0 "#486: create and publish $NAME@$VERSION in the second project, with the content of the release" -- \
  "$live/api.sh" create-copy "$R" "$NAME" "$B"
T=$(cat copy-create.out)
echo "T=$T" > ids.env
[ -n "$T" ] || die "no copy was created"

echo "== a record in each project" | tee -a steps.log
check record-a 0 "#486: snapshot $NAME@$VERSION --project-name of the lab project" -- \
  "$strata" snapshot "$NAME@$VERSION" --project-name "$A_NAME" -o a.json
check record-a-project 0 '#486: the record of the lab project names the release and the lab project' -- \
  jq -e --arg r "$R" --arg a "$A" '.source.uid == $r and .source.projectUid == $a' a.json
check record-b 0 "#486: snapshot $NAME@$VERSION --project-name of the second project" -- \
  "$strata" snapshot "$NAME@$VERSION" --project-name "$B_NAME" -o b.json
check record-b-project 0 '#486: the record of the second project names the copy and the second project' -- \
  jq -e --arg t "$T" --arg b "$B" '.source.uid == $t and .source.projectUid == $b' b.json
check verify-both 0 '#486: verify reads each record in the project that it names, in one run' -- \
  "$strata" verify a.json b.json

echo "== a project for each side" | tee -a steps.log
check same-release 0 "#486: same-release $NAME@$VERSION twice, with a project for each side, passes" -- \
  "$strata" same-release "$NAME@$VERSION" "$NAME@$VERSION" "${sides[@]}" --json
check same-release-projects 0 '#486: the chain names the lab project for side A and the second project for side B' -- \
  jq -e --arg a "$A" --arg b "$B" '.same and .records[0].projectUid == $a and .records[1].projectUid == $b' same-release.out
check same-release-line 0 '#486: the verdict names the project of each side' -- \
  bash -c "\"$strata\" same-release '$NAME@$VERSION' '$NAME@$VERSION' --project-name-a '$A_NAME' --project-name-b '$B_NAME' --no-color \
    | grep -qxF '               project $A -> $B'"
check compare-projects 0 '#486: compare with a project for each side names each project and each uid, and the content is the same' -- \
  bash -c "\"$strata\" compare '$NAME@$VERSION' '$NAME@$VERSION' --project-name-a '$A_NAME' --project-name-b '$B_NAME' --json \
    | jq -e --arg a '$A' --arg b '$B' --arg r '$R' --arg t '$T' \
      '.a.projectUid == \$a and .b.projectUid == \$b and .a.uid == \$r and .b.uid == \$t and .sameContent'"
check records-same 0 '#486: same-release of the two records passes, with no request' -- \
  "$strata" same-release a.json b.json

echo "== the copy goes" | tee -a steps.log
check copy-delete 0 "#486: delete the copy $T in the second project" -- \
  env PALETTE_PROJECT_UID="$B" "$live/api.sh" delete "$T"
T=""

echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

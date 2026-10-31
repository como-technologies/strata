#!/usr/bin/env bash
# The scope test (#113): a profile of the tenant beside a profile of the project with one name
# and one version. What the listing, the search, and the document of Palette say about each, and
# what strata does with the ref that two scopes answer.
#
#   examples/live/scope.sh
#
# It creates a profile of the tenant with the name, the version, and the packs of the release
# profile, through the API with no project. It reads the profile listing and the profile search
# of the project, and the document of each version. It lists the versions with strata, asks for
# the ref that two scopes answer, and records the tenant version by its uid. Then it deletes the
# tenant profile.
#
# Needs the profiles (just live-profiles) and no cluster: no cost. It leaves the tenant as it
# found it. Needs the strata release binary, jq, and the key as verify.sh needs it. Results go
# under runs/<UTC time>/scope/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=scope
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "${1:-}" in "") ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"

load_outputs
export PALETTE_PROJECT_UID=$(out project_uid)
R=$(out release_profile_uid)
NAME=$(out profile_name)
[ -n "$R" ] && [ -n "$NAME" ] || die "no release profile in the outputs; run just live-profiles"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/scope}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

VERSION=$("$live/api.sh" get "/v1/clusterprofiles/$R" | jq -r '.spec.version')
[ -n "$VERSION" ] && [ "$VERSION" != null ] || die "cannot read the version of $R"
T=""

summary() {
  write_summary "scope test" ": the release" \
    "\`$NAME@$VERSION\` (\`$R\`) of the project, and its copy of the tenant (\`${T:-none}\`)."
}
# the tenant profile goes at the end, also when a check stops the script
cleanup() {
  [ -n "$T" ] && "$live/api.sh" delete-tenant "$T" >> steps.log 2>&1
  summary
}
trap cleanup EXIT

search='{"filter":{"scope":"tenant"},"sort":[{"field":"profileName","order":"asc"}]}'

echo "== a profile of the tenant with the name and the version of the release (#113)" | tee -a steps.log
check tenant-create 0 "#113: create and publish $NAME@$VERSION at tenant scope, with the packs of the release" -- \
  "$live/api.sh" create-tenant-copy "$R" "$NAME"
T=$(cat tenant-create.out)
echo "T=$T" > ids.env
[ -n "$T" ] || die "no tenant profile was created"

echo "== what Palette says" | tee -a steps.log
check listing-both 0 '#113: the profile listing of the project names the tenant version and the project version' -- \
  bash -c "\"$live/api.sh\" get /v1/dashboard/clusterprofiles/metadata | jq -e --arg n '$NAME' --arg v '$VERSION' --arg r '$R' --arg t '$T' \
    '[.items[] | select(.metadata.name == \$n and .spec.version == \$v) | .metadata.uid] | sort == ([\$r, \$t] | sort)'"
check search-tenant 0 '#113: the profile search of the project with scope tenant names the tenant uid, and not the project uid' -- \
  bash -c "\"$live/api.sh\" post '/v1/dashboard/clusterprofiles?limit=50' '$search' | jq -e --arg r '$R' --arg t '$T' \
    '[.items[].specSummary.versions[].uid] | (index(\$t) != null) and (index(\$r) == null)'"
check annotation-tenant 0 '#113: in the project, the document of the tenant version gives the scope tenant' -- \
  bash -c "\"$live/api.sh\" get /v1/clusterprofiles/$T | jq -e '.metadata.annotations.scope == \"tenant\"'"
check annotation-project 0 '#113: the document of the release version gives the scope project' -- \
  bash -c "\"$live/api.sh\" get /v1/clusterprofiles/$R | jq -e '.metadata.annotations.scope == \"project\"'"

echo "== what strata says" | tee -a steps.log
check list-scope 0 '#113: list --json gives scope tenant for the tenant version and project for the release version' -- \
  bash -c "\"$strata\" list profiles --json | jq -e --arg r '$R' --arg t '$T' \
    '([.profiles[] | select(.uid == \$t)][0].scope == \"tenant\") and ([.profiles[] | select(.uid == \$r)][0].scope == \"project\")'"
check list-mark 0 '#113: the row of the tenant version ends with tenant' -- \
  bash -c "\"$strata\" list profiles --no-color | grep -qE '^  $NAME@$VERSION +$T +[a-z]+ +tenant\$'"
check list-packs-mark 0 '#113: with --packs, the row of the tenant version ends with tenant too' -- \
  bash -c "\"$strata\" list profiles --packs --no-color | grep -qE '^  $NAME@$VERSION +$T .* tenant\$'"
check ambiguous 2 "#113: snapshot $NAME@$VERSION exits 2: two scopes answer the ref" -- \
  "$strata" snapshot "$NAME@$VERSION" -o ambiguous.json
check ambiguous-says 0 '#113: the error names each uid with its scope' -- \
  bash -c "grep -q \"several profiles in this scope are named '$NAME'\" ambiguous.err && grep -q '$T (tenant)' ambiguous.err && grep -q '$R (project)' ambiguous.err"
check by-uid 0 '#113: the uid records the tenant version' -- "$strata" snapshot "$T" -o tenant.json
check tenant-list 0 '#113: at tenant scope, list --json gives scope tenant for the version' -- \
  bash -c "\"$strata\" list profiles --project '' --json | jq -e --arg t '$T' '[.profiles[] | select(.uid == \$t)][0].scope == \"tenant\"'"
check tenant-list-no-mark 0 '#113: at tenant scope, the row of the version has no mark' -- \
  bash -c "\"$strata\" list profiles --project '' --no-color | grep -qE '^  $NAME@$VERSION +$T +[a-z]+\$'"

echo "== the tenant profile goes" | tee -a steps.log
check tenant-delete 0 "#113: delete the tenant profile $T" -- "$live/api.sh" delete-tenant "$T"
T=""

echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

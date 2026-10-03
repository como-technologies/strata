#!/usr/bin/env bash
# The app profile test (#78): a record of an app profile, and a plain update of it.
#
#   examples/live/app-profile.sh [FROM_STAGE]
#
#   1  an app profile with one manifest tier: a record of appprofile:<name>@<version> holds the
#      tier as a pack, with no digest; verify of it is OK
#   2  a plain update of the manifest: the uid and the version stay, and verify of the record
#      reports drift, with the tier AUTHORED; --who reads the log of the kind appprofile (#140)
#   3  two app profiles with the same two tiers, made with the install orders 2 and 5, and 5
#      and 2: a record of each holds the orders and lists the tiers by them, and compare of the
#      two records names installOrder (#112). An update of the order through the tier route
#      does not change it. Then the two app profiles deleted
#   4  the app profile deleted
#
# Needs the strata release binary, jq, and the key as verify.sh needs it. It needs no cluster, and
# costs nothing. Results go under runs/<UTC time>/app-profile/, or OUT; FROM_STAGE resumes a run in
# OUT. Exit 0 only when no check fails.
set -uo pipefail
script=app-profile
from=${1:-1}
case "$from" in 1|2|3|4) ;; *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;; esac
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/app-profile}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

N=strata-live-app

summary() {
  {
    echo "# app profile test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`, app profile"
    echo "\`$N\` \`${U:-}\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

stage() { [ "$from" -le "$1" ] && echo "== stage $1: $2" | tee -a steps.log; }

load_outputs
export PALETTE_PROJECT_UID=$(out project_uid)

if stage 1 "an app profile"; then
  check create 0 "make the app profile $N@1.0.0 through the API" -- "$live/api.sh" app-profile-create "$N"
  U=$(cat create.out)
  echo "$U" > uid
  check record 0 '#78: record appprofile:<name>@<version>' -- "$strata" snapshot "appprofile:$N@1.0.0" -o app.json
  check record-type 0 '#78: the record is of type app, counts no consumer, and holds the tier as a pack in the layer app' -- \
    jq -e '(.source.type == "app") and (.source | has("consumers") | not) and ([.packs[].key] == ["app/config"]) and (.packs[0].manifests | length == 1)' app.json
  check record-no-digest 0 '#78: a tier has no digest: the upstream of the pack is unknown' -- \
    jq -e '.packs[0].hash.upstreamSource == "none"' app.json
  check record-by-uid 0 '#78: appprofile:<uid> gives the same root' -- \
    bash -c "[ \"\$(\"$strata\" snapshot appprofile:$U | jq -r .root)\" = \"\$(jq -r .root app.json)\" ]"
  check record-unknown 2 '#78: an app profile that is not there is an error that lists the app profiles' -- \
    "$strata" snapshot "appprofile:$N@9.9.9"
  check verify 0 '#78: verify of the record' -- "$strata" verify app.json
fi

U=${U:-$(cat uid 2> /dev/null)}

if stage 2 "a plain update of the manifest"; then
  check edit 0 '#78: update the manifest of the app profile through the API' -- \
    "$live/api.sh" app-profile-edit "$U" 'edited: "no"' 'edited: "yes"'
  check edit-same-version 0 '#78: the update keeps the uid and the version: no publish step, and no new version' -- \
    bash -c "\"$live/api.sh\" get /v1/appProfiles/$U | jq -e '.spec.version == \"1.0.0\" and (.spec.versions | length == 1)'"
  check edit-verify 1 '#78: verify of the record reports drift' -- "$strata" verify app.json
  check edit-verify-says 0 '#78: the tier is AUTHORED, and the manifest is named' -- \
    bash -c "grep -Eq '^~ \\[app +\\] config .*AUTHORED' edit-verify.out && grep -q 'manifest config' edit-verify.out"
  check edit-who 1 '#140: verify --who reads the audit log of the app profile' -- "$strata" verify app.json --who --json
  check edit-who-appprofile 0 '#140: Palette logs the update under the kind appprofile: the entries name the app profile' -- \
    jq -e --arg u "$U" '.verify.who | length > 0 and all(.resource.kind == "appprofile" and .resource.uid == $u)' edit-who.out
  check edit-last-modified 0 '#140: the document of the app profile gives the time of the update, after the record' -- \
    jq -e --arg at "$(jq -r .source.fetchedAt app.json)" '.verify.lastModified >= $at' edit-who.out
fi

if stage 3 "the install order of the tiers"; then
  check order-a-create 0 "#112: make $N-order-a with tier1 at the install order 2 and tier2 at 5" -- \
    "$live/api.sh" app-profile-create "$N-order-a" 2 5
  check order-b-create 0 "#112: make $N-order-b with the same tiers at 5 and 2" -- \
    "$live/api.sh" app-profile-create "$N-order-b" 5 2
  OA=$(cat order-a-create.out) OB=$(cat order-b-create.out)
  check order-a-tiers 0 '#112: the tiers route gives the install orders of the create' -- \
    bash -c "\"$live/api.sh\" get /v1/appProfiles/$OA/tiers | jq -e '[.spec.appTiers[] | {(.metadata.name): .spec.installOrder}] | add == {tier1: 2, tier2: 5}'"
  check order-a-record 0 "#112: a record of $N-order-a" -- "$strata" snapshot "appprofile:$OA" -o order-a.json
  check order-a-params 0 '#112: the record holds each order, and lists the tiers by it: tier1, tier2' -- \
    jq -e '[.packs[] | [.key, .params.installOrder]] == [["app/tier1", 2], ["app/tier2", 5]]' order-a.json
  check order-b-record 0 "#112: a record of $N-order-b" -- "$strata" snapshot "appprofile:$OB" -o order-b.json
  check order-b-params 0 '#112: the record lists tier2 first, at 2, then tier1, at 5' -- \
    jq -e '[.packs[] | [.key, .params.installOrder]] == [["app/tier2", 2], ["app/tier1", 5]]' order-b.json
  check order-compare 1 '#112: compare of the two records: the orders differ, so the roots differ' -- \
    "$strata" compare order-a.json order-b.json
  check order-compare-says 0 '#112: and it names installOrder under a tier' -- \
    grep -Eq '^ +~ param installOrder: (2 -> 5|5 -> 2)$' order-compare.out
  check order-update 1 '#112: an update of the order of tier1 through the tier route: Palette keeps the order' -- \
    "$live/api.sh" app-profile-order "$OA" 7
  check order-update-verify 0 "#112: so the record of $N-order-a still verifies" -- "$strata" verify order-a.json
  check order-a-delete 0 "delete $N-order-a" -- "$live/api.sh" app-profile-delete "$OA"
  check order-b-delete 0 "delete $N-order-b" -- "$live/api.sh" app-profile-delete "$OB"
fi

if stage 4 "the app profile deleted"; then
  check delete 0 "delete the app profile" -- "$live/api.sh" app-profile-delete "$U"
fi

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

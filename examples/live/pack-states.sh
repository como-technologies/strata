#!/usr/bin/env bash
# The pack state test (#68): what Palette does with a pack version that Spectro Cloud deprecated,
# disabled, or deleted, and what strata reports for each. It builds no cluster and needs no
# OpenTofu. It writes to Palette only to ask for a profile that pins each pack, and deletes each
# profile that Palette creates at once, so it runs whenever the key works.
#
#   examples/live/pack-states.sh
#
#   1  packs: each state in the heading and on the version, and the tags of each version
#   2  snapshot of a pack version in each state
#   3  a new profile that pins a pack version in each state
#
# The pack registry listing gives each version `spec.annotations.system_state`. The three packs
# below are ones Spectro Cloud retired in the public registries, one for each state, and none of
# them requires another pack, so Palette refuses a profile only for the state. The fourth pack
# carries the mark `disabled: "true"` of Spectro Cloud and no state (#125): Palette serves it, and
# `packs` says `disabled`. Results go under runs/<UTC time>/pack-states/, or OUT. Exit 0 only when
# no check fails.
set -uo pipefail
script=pack-states
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

disabled_reg=$public_repo
disabled=nginx
disabled_version=1.15.1
community="Palette Community Registry"
deprecated=argo-cd
deprecated_version=3.26.7

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/pack-states}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

disabled_uid=$(registry_uid "$disabled_reg")
community_uid=$(registry_uid "$community")
[ -n "$disabled_uid" ] && [ -n "$community_uid" ] || die "the key cannot see $disabled_reg and $community"

summary() {
  {
    echo "# pack state test"
    echo
    echo "Run \`$(basename "$(dirname "$run")")\` with \`$("$strata" --version)\`: \`$disabled@$disabled_version\` and"
    echo "\`$deleted@$deleted_version\` in \`$disabled_reg\`, \`$deprecated@$deprecated_version\` in \`$community\`."
    echo
    summary_table
  } > summary.md
}
trap summary EXIT

echo "== packs shows the state of each version (#68)" | tee -a steps.log
check states-disabled 0 '#68: packs lists a registry whose versions are all disabled' -- \
  "$strata" packs "$disabled" --registry "$disabled_reg"
check states-disabled-says 0 '#120: the heading says disabled, and the states query gives the tags of 0.26.1' -- \
  bash -c "head -1 states-disabled.out | grep -q 'disabled)\$' && grep -qE '^ *0\.26\.1 .* 0\.26\.1 0\.26\.x +disabled *\$' states-disabled.out"
check states-deleted 0 '#68: packs lists a registry whose versions are all deleted' -- \
  "$strata" packs "$deleted" --registry "$disabled_reg"
check states-deleted-says 0 '#120: the heading says deleted, and 1.0.6 has the tags 1.0.6 1.0.x 1.x' -- \
  bash -c "head -1 states-deleted.out | grep -q 'deleted)\$' && grep -qE '^ *1\.0\.6 .* 1\.0\.6 1\.0\.x 1\.x +deleted *\$' states-deleted.out"
check states-deprecated 0 '#68: packs lists a registry whose versions are all deprecated' -- \
  "$strata" packs "$deprecated" --registry "$community"
check states-deprecated-says 0 '#68: the heading says deprecated, and the version keeps its tags and says deprecated' -- \
  bash -c "head -1 states-deprecated.out | grep -q 'deprecated)' && grep -qE '^ *$deprecated_version .* $deprecated_version .*deprecated *\$' states-deprecated.out"
check states-mark 0 '#125: packs lists a version with the mark disabled and no system_state' -- \
  "$strata" packs "$marked" --registry "$disabled_reg"
check states-mark-says 0 '#125: the heading says disabled, and the row keeps its tags and says disabled' -- \
  bash -c "head -1 states-mark.out | grep -q 'disabled)\$' && grep -qE '^ *$marked_version .* $marked_version .*disabled *\$' states-mark.out"

echo "== snapshot of a pack version in each state (#68)" | tee -a steps.log
check snapshot-disabled 2 '#68: a disabled version is pack-unavailable' -- \
  "$strata" snapshot "pack:$disabled_reg/$disabled@$disabled_version" -o disabled.json
check snapshot-disabled-says 0 '#68: the error names the state and the registry' -- \
  grep -q "is disabled in registry '$disabled_reg'" snapshot-disabled.err
check snapshot-deleted 2 '#68: a deleted version is pack-unavailable' -- \
  "$strata" snapshot "pack:$disabled_reg/$deleted@$deleted_version" -o deleted.json
check snapshot-deprecated 0 '#68: a deprecated version records as usual' -- \
  "$strata" snapshot "pack:$community/$deprecated@$deprecated_version" -o deprecated.json

echo "== a new profile that pins a pack version in each state (#68)" | tee -a steps.log
check profile-disabled 1 '#68: Palette refuses a profile that pins a disabled version' -- \
  "$live/api.sh" probe-profile "$disabled" "$disabled_uid" "$disabled_version"
check profile-disabled-says 0 '#68: the refusal is ClusterProfileInvalidPackState, and says the pack is disabled' -- \
  grep -q 'ClusterProfileInvalidPackState: .* is disabled' profile-disabled.out
check profile-deleted 1 '#68: Palette refuses a profile that pins a deleted version' -- \
  "$live/api.sh" probe-profile "$deleted" "$disabled_uid" "$deleted_version"
check profile-deleted-says 0 '#68: the refusal is ClusterProfileInvalidPackState, and says the pack is deleted' -- \
  grep -q 'ClusterProfileInvalidPackState: .* is deleted' profile-deleted.out
check profile-deprecated 0 '#68: Palette creates a profile that pins a deprecated version; the probe deletes it' -- \
  "$live/api.sh" probe-profile "$deprecated" "$community_uid" "$deprecated_version"

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

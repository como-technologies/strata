#!/usr/bin/env bash
# The merge gate test (#57): scripts/ci/merge-gate.sh over records of the live profiles, in a git
# repository this script builds. It needs the live profiles (just live-profiles), no cluster, and
# it writes to Palette only the in-place edit of stage 4, which stage 5 undoes.
#
#   examples/live/merge-gate.sh [FROM_STAGE]
#
#   1  record the two live profile versions and the add-on version, and build a repository whose
#      main pins the later profile version
#   2  a branch that pins the earlier version: the gate reads the merge result and goes red
#   3  the same branch with the rollback declared in its commit: the gate passes
#   4  edit the add-on version in place through the API: the gate goes red on a record that Palette
#      no longer matches, although git says nothing and the rollback is still declared
#   5  undo the edit: the gate passes again
#
# The repository is local, with a bare origin, so the fetch and the merge are real git and no
# remote host is involved. What is live is every record and every verify: the profile versions come
# from the tenant, and stage 4's drift is an edit Palette accepted. Results go under
# runs/<UTC time>/merge-gate/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0 only when no check
# fails.
set -uo pipefail
script=merge-gate
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"
gate=$(cd "$live/../.." && pwd)/scripts/ci/merge-gate.sh

profile=strata-live-profile
addon=strata-live-addon
older=1.0.0
newer=1.0.1

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/merge-gate}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1
# the repository this script builds is its own, with no user configuration behind it
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
export GIT_AUTHOR_NAME=live GIT_AUTHOR_EMAIL=live@example.com
export GIT_COMMITTER_NAME=live GIT_COMMITTER_EMAIL=live@example.com

summary() {
  write_summary "merge gate test" ", profile" \
    "\`$profile\` at \`$older\` and \`$newer\`, add-on \`$addon@1.0.0\` (\`${AD:-}\`)."
}
trap summary EXIT

save() { echo "$1=$2" >> ids.env; }
[ -f ids.env ] && . ./ids.env

work=$run/work
# gate ID EXPECTED WHAT: run the gate in the work tree against main
gate_check() {
  check "$1" "$2" "$3" -- env -C "$work" STRATA_BIN="$strata" bash "$gate" "releases/*.json" --base main
}

if stage 1 "record the live versions and build the repository"; then
  AD=$("$strata" list --json 2> /dev/null | jq -r --arg n "$addon" '.profiles[] | select(.name == $n) | .uid' | head -1)
  [ -n "$AD" ] || die "no $addon in the project; run just live-profiles"
  save AD "$AD"
  rm -rf "$work" origin.git
  git init -q --bare -b main origin.git
  git clone -q origin.git "$work" 2> /dev/null || die "could not clone the bare origin"
  # a clone of an empty repository has no branch yet, and no global config names one
  git -C "$work" checkout -q -b main
  mkdir -p "$work/releases"
  check record-newer 0 "#57: record $profile@$newer, which main pins" -- \
    "$strata" snapshot "$profile@$newer" -o newer.json
  check record-addon 0 "#57: record $addon@1.0.0, which both sides pin" -- \
    "$strata" snapshot "$addon@1.0.0" -o "$work/releases/addon.json"
  check record-older 0 "#57: record $profile@$older, which the branch pins" -- \
    "$strata" snapshot "$profile@$older" -o older.json
  # main ran the earlier version first, as an environment does, so the gate can say whether the
  # branch returns to a release that this record carried before
  ( cd "$work" &&
    cp "$run/older.json" releases/dev.json &&
    git add releases && git commit -qm "pin $profile@$older" &&
    cp "$run/newer.json" releases/dev.json &&
    git commit -qam "pin $profile@$newer" &&
    git push -q origin main ) || die "could not build the repository"
fi

if stage 2 "a branch that pins the earlier version"; then
  ( cd "$work" && git checkout -q -b pin-back && cp "$run/older.json" releases/dev.json &&
    git commit -qam "pin $profile@$older" )
  gate_check behind 1 "#57: the gate is red: the merge result pins the earlier version"
  check behind-says 0 '#57: the report names the record and both versions' -- \
    grep -q "BEHIND: releases/dev.json" behind.out
  check behind-evidence 0 '#57: the evidence says main pinned this release before' -- \
    grep -q "main pinned this release before, on" behind.out
  check behind-declares 0 '#57: the report gives the line that declares a rollback' -- \
    grep -q "Rollback: releases/dev.json to $profile@$older" behind.out
fi

if stage 3 "the rollback declared in the commit"; then
  ( cd "$work" && git commit -q --amend -m "roll dev back to $profile@$older

Rollback: releases/dev.json to $profile@$older
$newer changed the csi values, and the volumes did not come back" )
  gate_check declared 0 '#57: the gate passes once the commit declares the rollback'
  check declared-says 0 '#57: the report names it a rollback, declared in a commit' -- \
    grep -q "ROLLBACK: releases/dev.json returns to $profile@$older, declared in a commit" declared.out
fi

if stage 4 "Palette moves under a record"; then
  check api-edit 0 '#57: edit the add-on version in place through the API' -- \
    "$live/api.sh" edit "$AD" "$addon_from" "$addon_to"
  gate_check drifted 1 '#57: the gate is red on a record Palette no longer matches'
  check drifted-says 0 '#57: the report names the record that does not verify' -- \
    grep -q "DRIFTED: releases/addon.json" drifted.out
  check drifted-keeps-rollback 0 '#57: the declared rollback still passes; only the record fails' -- \
    grep -q "ROLLBACK: releases/dev.json" drifted.out
fi

if stage 5 "undo the edit"; then
  check api-undo 0 '#57: undo the edit through the API' -- \
    "$live/api.sh" edit "$AD" "$addon_to" "$addon_from"
  gate_check restored 0 '#57: the gate passes again once Palette matches the record'
fi

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]

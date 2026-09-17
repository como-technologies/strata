#!/usr/bin/env bash
# Check what a merge request does to the records, against the target branch as it is now.
#
#   scripts/merge-gate.sh <record or pattern>... [--base BRANCH] [--remote NAME] [--no-verify]
#
# A branch that sat for a day compares against a base that moved. This script merges the target
# branch into the checkout first, so every answer is about the merge result: what the records will
# be after the merge button, not what they were when the branch started. Run it as the check that
# the merge button waits for.
#
# It reports three things, and exits 1 on any of them:
#   1. The merge conflicts in a record. The report compares the target branch with the branch, so
#      you read the conflict as a release, not as lines. Merge the target branch in and settle it.
#   2. The merge result moves a pin backwards: the target branch has a later version of a pack, or
#      of the profile, than the merge result. A branch that carries an older pin replaces what
#      another merge request pinned, and a cluster built from it gets the older release.
#   3. A record of the merge result does not verify: the profile or cluster it names changed in
#      Palette, or is gone. A cluster built from it would not get what the record says. --no-verify,
#      or no PALETTE_API_KEY, leaves this check out.
#
# Exit 0 when the merge result is clear, 1 when the gate is red, 2 when strata or git could not
# answer. Needs strata (or STRATA_BIN), git, jq, and sort -V. The target branch is --base, or the
# CI variable of the merge request: CI_MERGE_REQUEST_TARGET_BRANCH_NAME on GitLab, GITHUB_BASE_REF
# on GitHub.
set -euo pipefail

die() { echo "merge-gate: $*" >&2; exit 2; }
note() { echo "merge-gate: $*" >&2; }

patterns=()
base=""
remote="${STRATA_REMOTE:-origin}"
verify=1
while [ $# -gt 0 ]; do
  case "$1" in
    --base) base=$2; shift 2 ;;
    --remote) remote=$2; shift 2 ;;
    --no-verify) verify=""; shift ;;
    -h|--help) sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --*) die "unknown option $1" ;;
    *) patterns+=("$1"); shift ;;
  esac
done
[ ${#patterns[@]} -gt 0 ] || die "usage: merge-gate.sh <record or pattern>... [--base BRANCH] [--remote NAME] [--no-verify]"

[ -n "$base" ] || base="${CI_MERGE_REQUEST_TARGET_BRANCH_NAME:-${GITHUB_BASE_REF:-}}"
[ -n "$base" ] || die "pass --base: no CI variable names the target branch"
git rev-parse --is-inside-work-tree > /dev/null 2>&1 || die "not a git checkout"

strata=${STRATA_BIN:-strata}
[ -n "${PALETTE_API_KEY:-${SPECTROCLOUD_APIKEY:-}}" ] || verify=""
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# at REV FILE OUT: write the file as that revision has it, or fail when it is not there
at() { git show "$1:$2" > "$3" 2> /dev/null; }

# compare A B FORMAT: exit 1 is a difference, which is the normal case here
compare() {
  local code=0
  "$strata" compare "$1" "$2" --format "$3" --no-color || code=$?
  [ "$code" -le 1 ] || die "strata could not compare $1 with $2"
}

# ---- the merge result --------------------------------------------------------------------------
git fetch --quiet "$remote" "$base" || die "could not fetch $base from $remote"
target="$remote/$base"
git rev-parse --verify --quiet "$target" > /dev/null || die "$remote has no branch $base"

records_matching() { # the paths of the index that match the patterns
  local p
  for p in "${patterns[@]}"; do git ls-files -- "$p"; done
}

if git merge-base --is-ancestor "$target" HEAD; then
  note "the branch already carries $target; the checkout is the merge result"
elif git merge --no-commit --no-ff "$target" > "$work/merge.log" 2>&1; then
  note "merged $target in: the checks below are about the merge result"
else
  conflicts=$(git diff --name-only --diff-filter=U || true)
  echo "CONFLICT: this branch does not merge $target cleanly"
  printf '%s\n' "$conflicts" | sed 's/^/  /'
  # say what the conflict is about: the release on the target branch against the release here
  for f in $conflicts; do
    if at "$target" "$f" "$work/target.json" && at HEAD "$f" "$work/branch.json"; then
      echo
      echo "  $f: $base as it is now, against this branch"
      compare "$work/target.json" "$work/branch.json" terminal | sed 's/^/  /'
    fi
  done
  git merge --abort > /dev/null 2>&1 || true
  echo
  echo "  merge $base into the branch and settle each record before the merge button."
  exit 1
fi

# ---- the records ---------------------------------------------------------------------------------
files=()
while IFS= read -r f; do
  if [ -n "$f" ]; then files+=("$f"); fi
done < <(records_matching)
[ ${#files[@]} -gt 0 ] || die "no record in the merge result matches: ${patterns[*]}"

# backwards DIFF: the pins that the merge result moves back, one line each
backwards() {
  jq -r '(["the profile", (.a.version // ""), (.b.version // "")] | @tsv),
         (.packs[] | select(.status == "changed")
          | [.key, (.a.version // ""), (.b.version // "")] | @tsv)' "$1" \
    | while IFS=$'\t' read -r key from to; do
        [ -n "$from" ] && [ -n "$to" ] && [ "$from" != "$to" ] || continue
        lower=$(printf '%s\n%s\n' "$from" "$to" | sort -V | sed -n 1p)
        if [ "$lower" = "$to" ]; then
          echo "$key: $base has $from, the merge result has $to"
        fi
      done || true
}

behind=() unverified=()
for f in "${files[@]}"; do
  if at "$target" "$f" "$work/target.json"; then
    compare "$work/target.json" "$f" json > "$work/diff.json"
    back=$(backwards "$work/diff.json")
    if [ -n "$back" ]; then
      behind+=("$f")
      echo "BEHIND: $f would replace what $base pinned with an older version"
      printf '%s\n' "$back" | sed 's/^/  /'
      echo "  merge $base into the branch, then read the comparison again."
    fi
  fi
  if [ -n "$verify" ]; then
    code=0
    "$strata" verify "$f" --quiet > "$work/verify.txt" 2>&1 || code=$?
    case "$code" in
      0) ;;
      1) unverified+=("$f")
         echo "DRIFTED: $f does not verify; a cluster built from it would not get what it says"
         sed 's/^/  /' "$work/verify.txt" ;;
      *) unverified+=("$f")
         echo "ERROR: $f could not be checked"
         sed 's/^/  /' "$work/verify.txt" ;;
    esac
  fi
done

note "${#files[@]} records: ${#behind[@]} behind $base, ${#unverified[@]} did not verify"
if [ ${#behind[@]} -gt 0 ] || [ ${#unverified[@]} -gt 0 ]; then
  note "the merge button must wait: settle the records above, then push again"
  exit 1
fi
note "the merge result is clear against $base"
exit 0

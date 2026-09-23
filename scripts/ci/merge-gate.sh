#!/usr/bin/env bash
# Check what a merge request does to the records, against the target branch as it is now.
#
#   scripts/ci/merge-gate.sh <record or pattern>... [--base BRANCH] [--remote NAME]
#                         [--no-verify] [--allow-rollback]
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
#      A rollback moves a pin backwards too, and only the author knows which one it is, so the
#      author says so. A commit on the branch carries the line
#
#        Rollback: <record> to <name>@<version>
#
#      which must name the record and the release that the merge result pins, or the job runs
#      with --allow-rollback, which is what a job does for a merge request with the rollback
#      label. A declared rollback is reported and passes. Every other backwards pin is red, and
#      the report gives the evidence: whether the target branch pinned that release before, and
#      whether the branch wrote its record before or after the target branch moved.
#   3. A record of the merge result does not verify: the profile or cluster it names changed in
#      Palette, or is gone. A cluster built from it would not get what the record says. --no-verify,
#      or no PALETTE_API_KEY, leaves this check out.
#
# Exit 0 when the merge result is clear, 1 when the gate is red, and 2 when the script could not
# run at all. A record that strata could not check is red, not 2: the gate fails closed.
# Needs strata (or STRATA_BIN), git, jq, and sort -V. The target branch is --base, or the
# CI variable of the merge request: CI_MERGE_REQUEST_TARGET_BRANCH_NAME on GitLab, GITHUB_BASE_REF
# on GitHub.
set -euo pipefail

die() { echo "merge-gate: $*" >&2; exit 2; }
note() { echo "merge-gate: $*" >&2; }

patterns=()
base=""
remote="${STRATA_REMOTE:-origin}"
verify=1
allow_rollback=""
while [ $# -gt 0 ]; do
  case "$1" in
    --base) base=$2; shift 2 ;;
    --remote) remote=$2; shift 2 ;;
    --no-verify) verify=""; shift ;;
    --allow-rollback) allow_rollback=1; shift ;;
    -h|--help) sed -n '2,37p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --*) die "unknown option $1" ;;
    *) patterns+=("$1"); shift ;;
  esac
done
[ ${#patterns[@]} -gt 0 ] || die "usage: merge-gate.sh <record or pattern>... [--base BRANCH] [--remote NAME] [--no-verify] [--allow-rollback]"

[ -n "$base" ] || base="${CI_MERGE_REQUEST_TARGET_BRANCH_NAME:-${GITHUB_BASE_REF:-}}"
[ -n "$base" ] || die "pass --base: no CI variable names the target branch"
git rev-parse --is-inside-work-tree > /dev/null 2>&1 || die "not a git checkout"

strata=${STRATA_BIN:-strata}
[ -n "${PALETTE_API_KEY:-${SPECTROCLOUD_APIKEY:-}}" ] || verify=""
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# at REV FILE OUT: write the file as that revision has it, or fail when it is not there
at() { git show "$1:$2" > "$3" 2> /dev/null; }

# same_release A B: 0 when the two files are one release, 1 when they are not
same_release() {
  local code=0
  "$strata" same-release "$1" "$2" --quiet > /dev/null 2>&1 || code=$?
  [ "$code" -le 1 ] || die "strata could not read $1 or $2"
  return "$code"
}

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

# pinned_before FILE: the date and the commit where the target branch last pinned the release
# that the merge result pins. A rollback returns to a release that this record carried before.
pinned_before() {
  local f=$1 c
  for c in $(git log --format=%H -n 50 "$target" -- "$f"); do
    at "$c" "$f" "$work/old.json" || continue
    if same_release "$work/old.json" "$f"; then
      echo "$(git log -1 --format=%cs "$c") in $(git rev-parse --short "$c")"
      return 0
    fi
  done
  return 1
}

# wrote_after_target FILE: 0 when the branch wrote its record after the target branch moved, so
# the author had the later release in front of them. A merge commit does not count: settling a
# conflict by keeping one side is the accident this gate is here for.
wrote_after_target() {
  local f=$1 theirs mine
  theirs=$(git log -1 --format=%H "$target" -- "$f")
  mine=$(git log -1 --no-merges --format=%H HEAD --not "$target" -- "$f")
  [ -n "$theirs" ] && [ -n "$mine" ] || return 1
  git merge-base --is-ancestor "$theirs" "$mine"
}

# declared FILE RELEASE: 0 when a commit of this branch says that this is a rollback.
# The messages are read into a variable first. `git log | grep -q` looks right and is a race:
# grep -q leaves at the first match, which is the newest commit, and git log then dies of SIGPIPE
# with 141, which `set -o pipefail` makes the answer of the whole pipeline. A declared rollback
# then reads as no declaration, and only when the log is long enough that git had more to write.
declared() {
  local messages
  messages=$(git log --format=%B HEAD --not "$target")
  grep -qiE "^Rollback:[[:space:]]+$1[[:space:]]+to[[:space:]]+$2([[:space:]]+.*)?$" <<< "$messages"
}

behind=() rolled=() unverified=()
for f in "${files[@]}"; do
  if at "$target" "$f" "$work/target.json"; then
    compare "$work/target.json" "$f" json > "$work/diff.json"
    back=$(backwards "$work/diff.json")
    if [ -n "$back" ]; then
      release=$(jq -r '"\(.b.name // "")@\(.b.version // "")"' "$work/diff.json")
      theirs=$(jq -r '"\(.a.name // "")@\(.a.version // "")"' "$work/diff.json")
      # the evidence that tells a rollback from a branch that did not look
      evidence=""
      if where=$(pinned_before "$f"); then
        evidence="  $base pinned this release before, on $where"
      else
        evidence="  $base never pinned this release in the last 50 commits of $f"
      fi
      if wrote_after_target "$f"; then
        evidence="$evidence
  the branch wrote this record after $base moved to $theirs"
      else
        evidence="$evidence
  the branch wrote this record before $base moved to $theirs"
      fi
      if [ -n "$allow_rollback" ]; then
        rolled+=("$f")
        echo "ROLLBACK: $f returns to $release, by the label on the merge request"
        printf '%s\n' "$back" | sed 's/^/  /'
        printf '%s\n' "$evidence"
      elif declared "$f" "$release"; then
        rolled+=("$f")
        echo "ROLLBACK: $f returns to $release, declared in a commit of the branch"
        printf '%s\n' "$back" | sed 's/^/  /'
        printf '%s\n' "$evidence"
      else
        behind+=("$f")
        echo "BEHIND: $f would replace what $base pinned with an older version"
        printf '%s\n' "$back" | sed 's/^/  /'
        printf '%s\n' "$evidence"
        echo "  a rollback says so, in a commit of the branch:"
        echo "    Rollback: $f to $release"
        echo "  or in the rollback label, which gives the job --allow-rollback."
      fi
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

plural() { # plural COUNT SINGULAR: "1 record", "2 records", "no records"
  case "$1" in 0) echo "no ${2}s" ;; 1) echo "1 $2" ;; *) echo "$1 ${2}s" ;; esac
}
note "$(plural ${#files[@]} record): ${#behind[@]} behind $base, ${#rolled[@]} declared rollback, ${#unverified[@]} did not verify"
if [ ${#behind[@]} -gt 0 ] || [ ${#unverified[@]} -gt 0 ]; then
  note "the merge button must wait: settle the records above, then push again"
  exit 1
fi
note "the merge result is clear against $base"
exit 0

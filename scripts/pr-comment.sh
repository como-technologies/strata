#!/usr/bin/env bash
# Post a strata comparison as one pull request comment and set a label from its impact.
#
#   scripts/pr-comment.sh <A> <B> [--pr N] [--repo OWNER/REPO] [--label NAME] [--threshold LEVEL] [--dry-run]
#
# A and B are refs strata understands: snapshot files, uids, or name@version. A ref that is not a
# file is snapshotted once, so both renderings read files and Palette is asked once. The comment starts
# with the marker <!-- strata -->; a later run finds the comment by that marker and updates it, so
# a pull request carries one strata comment that follows each push. The label (default repave) is
# added when summary.maxImpact is at or above the threshold (default repave) and removed otherwise.
# --dry-run prints the comment, says what it would do with the label, and calls no host.
#
# Needs strata (or STRATA_BIN), jq, and gh logged in. Every host call is one of the functions
# below, so another code host is another set of the same functions.
set -euo pipefail

die() { echo "pr-comment: $*" >&2; exit 2; }
note() { echo "pr-comment: $*" >&2; }

# ---- host: GitHub through gh -----------------------------------------------------------------
host_repo() { gh repo view --json nameWithOwner | jq -r .nameWithOwner; }
host_pr_number() { gh pr view --json number | jq -r .number; }
host_find_comment() {
  gh api "repos/$repo/issues/$pr/comments" --paginate \
    | jq -r '.[] | select(.body | startswith("<!-- strata -->")) | .id' | sed -n 1p
}
host_post_comment() { gh pr comment "$pr" --repo "$repo" --body-file "$1"; }
host_update_comment() { gh api -X PATCH "repos/$repo/issues/comments/$1" -F "body=@$2" >/dev/null; }
host_labels() { gh pr view "$pr" --repo "$repo" --json labels | jq -r '.labels[].name'; }
host_repo_labels() { gh label list --repo "$repo" --limit 1000 --json name | jq -r '.[].name'; }
# gh adds labels by name and fails on a name the repository does not have yet
host_create_label() {
  gh label create "$1" --repo "$repo" --color d93f0b \
    --description "strata: the maximum impact reached the threshold" >/dev/null
}
host_add_label() { gh pr edit "$pr" --repo "$repo" --add-label "$1"; }
host_remove_label() { gh pr edit "$pr" --repo "$repo" --remove-label "$1"; }

# ---- arguments -------------------------------------------------------------------------------
refs=()
pr=""
repo="${GITHUB_REPOSITORY:-}"
label="repave"
threshold="repave"
dry=""
while [ $# -gt 0 ]; do
  case "$1" in
    --pr) pr=$2; shift 2 ;;
    --repo) repo=$2; shift 2 ;;
    --label) label=$2; shift 2 ;;
    --threshold) threshold=$2; shift 2 ;;
    --dry-run) dry=1; shift ;;
    -h|--help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --*) die "unknown option $1" ;;
    *) refs+=("$1"); shift ;;
  esac
done
[ ${#refs[@]} -eq 2 ] || die "usage: pr-comment.sh <A> <B> [--pr N] [--repo OWNER/REPO] [--label NAME] [--threshold LEVEL] [--dry-run]"

# Impact levels in the order strata defines them; the label follows the threshold.
rank() {
  case "$1" in
    none) echo 0 ;; sync) echo 1 ;; in-place) echo 2 ;; cluster-wide) echo 3 ;;
    may-repave) echo 4 ;; repave) echo 5 ;;
    *) die "unknown impact level '$1'" ;;
  esac
}
threshold_rank=$(rank "$threshold")

# ---- the comparison --------------------------------------------------------------------------
strata=${STRATA_BIN:-strata}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# A live ref is fetched once; both renderings then read files.
files=()
for i in 0 1; do
  if [ -f "${refs[$i]}" ]; then
    files+=("${refs[$i]}")
  else
    "$strata" snapshot "${refs[$i]}" -o "$work/ref$i.json" || exit 2
    files+=("$work/ref$i.json")
  fi
done

# Exit 1 is a difference, which is the normal case; 2 means strata could not answer.
compare() {
  local code=0
  "$strata" compare "${files[0]}" "${files[1]}" --format "$1" > "$2" || code=$?
  [ "$code" -le 1 ] || exit 2
}
compare json "$work/report.json"
compare markdown "$work/report.md"
{ echo "<!-- strata -->"; cat "$work/report.md"; } > "$work/comment.md"

max=$(jq -r .summary.maxImpact "$work/report.json")
if [ "$(rank "$max")" -ge "$threshold_rank" ]; then want_label=1; else want_label=0; fi

if [ -n "$dry" ]; then
  cat "$work/comment.md"
  if [ "$want_label" = 1 ]; then
    note "max impact: $max; would add label $label"
  else
    note "max impact: $max; would remove label $label if present"
  fi
  exit 0
fi

# ---- the host --------------------------------------------------------------------------------
[ -n "$repo" ] || repo=$(host_repo)
[ -n "$pr" ] || pr=$(host_pr_number)
[ -n "$repo" ] && [ -n "$pr" ] || die "could not determine the repository and pull request; pass --repo and --pr"

existing=$(host_find_comment)
if [ -n "$existing" ]; then
  host_update_comment "$existing" "$work/comment.md"
  comment_note="comment $existing updated"
else
  host_post_comment "$work/comment.md"
  comment_note="comment posted"
fi

# Read the listings into variables first: grep -q exits early, and under pipefail the writer's
# broken pipe would read as "no such label".
pr_labels=$(host_labels)
if grep -Fxq "$label" <<<"$pr_labels"; then have_label=1; else have_label=0; fi
if [ "$want_label" = 1 ] && [ "$have_label" = 0 ]; then
  repo_labels=$(host_repo_labels)
  grep -Fxq "$label" <<<"$repo_labels" || host_create_label "$label"
  host_add_label "$label"
  label_note="label $label added"
elif [ "$want_label" = 0 ] && [ "$have_label" = 1 ]; then
  host_remove_label "$label"
  label_note="label $label removed"
else
  label_note="label $label unchanged"
fi
note "max impact: $max; $comment_note; $label_note"

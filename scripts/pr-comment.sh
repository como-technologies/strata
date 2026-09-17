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
# Hosts: GitHub through `gh`, and GitLab through the REST API v4 with `curl`. --host picks one;
# without it, the CI variables do: GITLAB_CI or CI_PROJECT_ID means GitLab, else GitHub. On GitLab
# the merge request is CI_MERGE_REQUEST_IID, the project CI_PROJECT_ID or --repo, the API
# CI_API_V4_URL, and the token GITLAB_TOKEN: a job token cannot write notes.
#
# Needs strata (or STRATA_BIN), jq, and the host's tool: gh logged in, or curl and GITLAB_TOKEN.
# Every host call goes through `host`, so another code host is another set of the same functions.
set -euo pipefail

die() { echo "pr-comment: $*" >&2; exit 2; }
note() { echo "pr-comment: $*" >&2; }

# ---- the hosts ---------------------------------------------------------------------------------
# One function for each thing the script needs from a code host. `host <name> [args]` calls the
# set of the chosen host.

github_repo() { gh repo view --json nameWithOwner | jq -r .nameWithOwner; }
github_pr_number() { gh pr view --json number | jq -r .number; }
github_find_comment() {
  gh api "repos/$repo/issues/$pr/comments" --paginate \
    | jq -r '.[] | select(.body | startswith("<!-- strata -->")) | .id' | sed -n 1p
}
github_post_comment() { gh pr comment "$pr" --repo "$repo" --body-file "$1"; }
github_update_comment() { gh api -X PATCH "repos/$repo/issues/comments/$1" -F "body=@$2" >/dev/null; }
github_labels() { gh pr view "$pr" --repo "$repo" --json labels | jq -r '.labels[].name'; }
github_repo_labels() { gh label list --repo "$repo" --limit 1000 --json name | jq -r '.[].name'; }
# gh adds labels by name and fails on a name the repository does not have yet
github_create_label() {
  gh label create "$1" --repo "$repo" --color d93f0b \
    --description "strata: the maximum impact reached the threshold" >/dev/null
}
github_add_label() { gh pr edit "$pr" --repo "$repo" --add-label "$1"; }
github_remove_label() { gh pr edit "$pr" --repo "$repo" --remove-label "$1"; }

# GitLab: the REST API v4. The project is a number or a path with its slashes encoded.
gitlab_api() { # METHOD PATH [curl arguments...]
  local method=$1 path=$2 status body
  shift 2
  body=$(mktemp)
  status=$(curl -sS -o "$body" -w '%{http_code}' -X "$method" \
    -H "PRIVATE-TOKEN: ${GITLAB_TOKEN:-}" "$api/projects/$project$path" "$@")
  case "$status" in
    2*) cat "$body"; rm -f "$body" ;;
    *) echo "pr-comment: GitLab answered HTTP $status for $method $path: $(head -c 300 "$body")" >&2
       rm -f "$body"; return 1 ;;
  esac
}
gitlab_repo() { printf '%s' "${CI_PROJECT_PATH:-$project}"; }
gitlab_pr_number() { printf '%s' "${CI_MERGE_REQUEST_IID:-}"; }
gitlab_find_comment() {
  gitlab_api GET "/merge_requests/$pr/notes?per_page=100&sort=asc" \
    | jq -r '.[] | select(.body | startswith("<!-- strata -->")) | .id' | sed -n 1p
}
gitlab_post_comment() {
  gitlab_api POST "/merge_requests/$pr/notes" --data-urlencode "body@$1" >/dev/null
}
gitlab_update_comment() {
  gitlab_api PUT "/merge_requests/$pr/notes/$1" --data-urlencode "body@$2" >/dev/null
}
gitlab_labels() { gitlab_api GET "/merge_requests/$pr" | jq -r '.labels[]'; }
gitlab_repo_labels() { gitlab_api GET "/labels?per_page=100" | jq -r '.[].name'; }
gitlab_create_label() {
  gitlab_api POST "/labels" --data-urlencode "name=$1" --data-urlencode "color=#d93f0b" \
    --data-urlencode "description=strata: the maximum impact reached the threshold" >/dev/null
}
gitlab_add_label() {
  gitlab_api PUT "/merge_requests/$pr" --data-urlencode "add_labels=$1" >/dev/null
}
gitlab_remove_label() {
  gitlab_api PUT "/merge_requests/$pr" --data-urlencode "remove_labels=$1" >/dev/null
}

host() { local what=$1; shift; "${host_kind}_$what" "$@"; }

# ---- arguments -------------------------------------------------------------------------------
refs=()
pr=""
repo="${GITHUB_REPOSITORY:-}"
label="repave"
threshold="repave"
dry=""
host_kind=""
while [ $# -gt 0 ]; do
  case "$1" in
    --pr) pr=$2; shift 2 ;;
    --repo) repo=$2; shift 2 ;;
    --host) host_kind=$2; shift 2 ;;
    --label) label=$2; shift 2 ;;
    --threshold) threshold=$2; shift 2 ;;
    --dry-run) dry=1; shift ;;
    -h|--help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --*) die "unknown option $1" ;;
    *) refs+=("$1"); shift ;;
  esac
done
[ ${#refs[@]} -eq 2 ] || die "usage: pr-comment.sh <A> <B> [--pr N] [--repo OWNER/REPO] [--host github|gitlab] [--label NAME] [--threshold LEVEL] [--dry-run]"

# The CI variables name the host when nothing else does.
if [ -z "$host_kind" ]; then
  if [ -n "${GITLAB_CI:-}${CI_PROJECT_ID:-}" ]; then host_kind=gitlab; else host_kind=github; fi
fi
case "$host_kind" in
  github|gitlab) ;;
  *) die "unknown host '$host_kind': github or gitlab" ;;
esac
if [ "$host_kind" = gitlab ]; then
  api=${CI_API_V4_URL:-https://gitlab.com/api/v4}
  project=${CI_PROJECT_ID:-}
  # a path such as group/sub/project addresses a project too, with its slashes encoded
  [ -n "$project" ] || project=$(printf '%s' "${repo:-}" | sed 's#/#%2F#g')
  [ -n "${GITLAB_TOKEN:-}" ] || die "set GITLAB_TOKEN: a job token cannot write notes"
fi

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
[ -n "$repo" ] || repo=$(host repo)
[ -n "$pr" ] || pr=$(host pr_number)
[ -n "$repo" ] && [ -n "$pr" ] || die "could not determine the repository and pull request; pass --repo and --pr"

existing=$(host find_comment)
if [ -n "$existing" ]; then
  host update_comment "$existing" "$work/comment.md"
  comment_note="comment $existing updated"
else
  host post_comment "$work/comment.md"
  comment_note="comment posted"
fi

# Read the listings into variables first: grep -q exits early, and under pipefail the writer's
# broken pipe would read as "no such label".
pr_labels=$(host labels)
if grep -Fxq "$label" <<<"$pr_labels"; then have_label=1; else have_label=0; fi
if [ "$want_label" = 1 ] && [ "$have_label" = 0 ]; then
  repo_labels=$(host repo_labels)
  grep -Fxq "$label" <<<"$repo_labels" || host create_label "$label"
  host add_label "$label"
  label_note="label $label added"
elif [ "$want_label" = 0 ] && [ "$have_label" = 1 ]; then
  host remove_label "$label"
  label_note="label $label removed"
else
  label_note="label $label unchanged"
fi
note "max impact: $max; $comment_note; $label_note"

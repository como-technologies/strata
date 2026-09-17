#!/usr/bin/env bash
# Report the records a scheduled `strata verify` finds drifted, or cannot check, as issues.
#
#   scripts/verify-report.sh <record or pattern>... [--repo OWNER/REPO] [--label NAME] [--dry-run]
#
# Runs `strata verify --summary --format json` over the records; strata expands a quoted pattern
# such as 'releases/**/*.json'. Each record that did not verify is checked again on its own for its
# markdown report, and that check decides. A record that drifted, or could not be checked, has one
# open issue labeled NAME (default strata-drift), found by the marker
# <!-- strata-verify: <path> --> on the first line of its body: opened on the first run that finds
# it, updated with the latest report on each later run, and closed with a comment on the first run
# where the record verifies again. Issues of records not given are left alone. --dry-run prints
# each issue body and what would happen, and calls no host.
#
# Exit 0 when every record verified, 1 when any drifted, 2 when any could not be checked or verify
# checked nothing; the issues are handled first, so a scheduled job shows the state too. A host
# call that fails stops the script with a nonzero status.
#
# Needs strata (or STRATA_BIN), jq, and gh logged in. Every host call is one of the functions
# below, so another code host is another set of the same functions.
set -euo pipefail

die() { echo "verify-report: $*" >&2; exit 2; }
note() { echo "verify-report: $*" >&2; }

# ---- host: GitHub through gh -----------------------------------------------------------------
host_repo() { gh repo view --json nameWithOwner | jq -r .nameWithOwner; }
host_open_issues() {
  gh issue list --repo "$repo" --label "$label" --state open --limit 1000 --json number,title,body
}
host_repo_labels() { gh label list --repo "$repo" --limit 1000 --json name | jq -r '.[].name'; }
host_create_label() {
  gh label create "$1" --repo "$repo" --color d93f0b \
    --description "strata: a record that drifted or could not be checked" >/dev/null
}
# prints the new issue's number
host_open_issue() {
  local url
  url=$(gh issue create --repo "$repo" --title "$1" --body-file "$2" --label "$label")
  echo "${url##*/}"
}
host_update_issue() { gh issue edit "$1" --repo "$repo" --title "$2" --body-file "$3" >/dev/null; }
host_close_issue() { gh issue close "$1" --repo "$repo" --comment "$2" >/dev/null; }

# ---- arguments -------------------------------------------------------------------------------
patterns=()
repo="${GITHUB_REPOSITORY:-}"
label="strata-drift"
dry=""
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) repo=$2; shift 2 ;;
    --label) label=$2; shift 2 ;;
    --dry-run) dry=1; shift ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --*) die "unknown option $1" ;;
    *) patterns+=("$1"); shift ;;
  esac
done
[ ${#patterns[@]} -gt 0 ] || die "usage: verify-report.sh <record or pattern>... [--repo OWNER/REPO] [--label NAME] [--dry-run]"

strata=${STRATA_BIN:-strata}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# ---- the check -------------------------------------------------------------------------------
code=0
"$strata" verify --summary --quiet --format json "${patterns[@]}" > "$work/summary.json" 2> "$work/summary.err" || code=$?
if ! jq -e '.schema == "strata-verify-summary/1"' "$work/summary.json" > /dev/null 2>&1; then
  cat "$work/summary.err" >&2
  die "strata verify checked nothing (exit $code); no issue was touched"
fi

if [ -z "$dry" ]; then
  [ -n "$repo" ] || repo=$(host_repo)
  [ -n "$repo" ] || die "could not determine the repository; pass --repo"
  host_open_issues > "$work/issues.json"
fi

# issue_for PATH -> the lowest open issue number whose body starts with PATH's marker
issue_for() {
  jq -r --arg m "<!-- strata-verify: $1 -->" \
    '[.[] | select(((.body // "") | split("\n")[0] | rtrimstr("\r")) == $m) | .number] | min // empty' \
    "$work/issues.json"
}

label_ready=""
ensure_label() {
  [ -z "$label_ready" ] || return 0
  local labels
  labels=$(host_repo_labels)
  grep -Fxq "$label" <<<"$labels" || host_create_label "$label"
  label_ready=1
}

unchanged=0 drifted=0 errors=0
actions=()
# the records come on descriptor 3, so a host call that reads stdin cannot take them
while IFS= read -r record <&3; do
  path=$(jq -r .path <<<"$record")
  status=$(jq -r .status <<<"$record")
  subject=$(jq -r 'if (.name // "") == "" then .path else "\(.name)@\(.version)" end' <<<"$record")
  body="$work/body.md"
  if [ "$status" != unchanged ]; then
    rc=0
    "$strata" verify "$path" --quiet --format markdown > "$work/report.md" 2> "$work/report.err" || rc=$?
    case "$rc" in
      0) status=unchanged ;;
      1) status=drifted ;;
      *) status=error ;;
    esac
  fi
  case "$status" in
    unchanged)
      unchanged=$((unchanged + 1))
      recorded=$(jq -r '.recordedAt // ""' <<<"$record")
      if [ -n "$dry" ]; then
        note "would close the open issue for $path, if any"
        continue
      fi
      number=$(issue_for "$path")
      [ -n "$number" ] || continue
      host_close_issue "$number" "\`$path\` verifies again: $subject is unchanged since $recorded (checked $now)."
      actions+=("closed #$number for $path")
      continue
      ;;
    drifted)
      drifted=$((drifted + 1))
      title="strata verify: $subject drifted ($path)"
      {
        echo "<!-- strata-verify: $path -->"
        echo "\`strata verify $path\` reports drift, checked $now."
        echo
        cat "$work/report.md"
      } > "$body"
      ;;
    *)
      errors=$((errors + 1))
      title="strata verify: $path could not be checked"
      {
        echo "<!-- strata-verify: $path -->"
        echo "\`strata verify $path\` could not check the record, checked $now: $(jq -r 'if .error then "\(.error.kind): \(.error.message)" else "see the output below" end' <<<"$record")"
        echo
        echo '```'
        cat "$work/report.err"
        echo '```'
      } > "$body"
      ;;
  esac
  if [ -n "$dry" ]; then
    cat "$body"
    echo
    note "would open or update the issue for $path: $title"
    continue
  fi
  number=$(issue_for "$path")
  if [ -n "$number" ]; then
    host_update_issue "$number" "$title" "$body"
    actions+=("updated #$number for $path")
  else
    ensure_label
    number=$(host_open_issue "$title" "$body")
    actions+=("opened #$number for $path")
  fi
done 3< <(jq -c '.records[]' "$work/summary.json")

line="$((unchanged + drifted + errors)) records: $unchanged unchanged, $drifted drifted, $errors could not be checked"
for a in "${actions[@]+"${actions[@]}"}"; do
  line="$line; $a"
done
note "$line"
if [ "$errors" -gt 0 ]; then exit 2; fi
if [ "$drifted" -gt 0 ]; then exit 1; fi
exit 0

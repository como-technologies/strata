#!/usr/bin/env bash
# Report the records a scheduled `strata verify` finds drifted, unverified, or cannot check, as
# issues.
#
#   scripts/ci/verify-report.sh <record or pattern>... [--repo OWNER/REPO] [--host github|gitlab]
#                               [--label NAME] [--allow-no-digest] [--who] [--dry-run]
#
# Runs `strata verify --summary --format json` over the records; strata expands a quoted pattern
# such as 'releases/**/*.json'. Each record that did not verify is checked again on its own for its
# markdown report, and that check decides. A record that drifted, is unverified (it holds a pack
# with no upstream digest, which --allow-no-digest accepts), or could not be checked, has one open
# issue labeled NAME (default strata-drift), found by the marker
# <!-- strata-verify: <path> --> on the first line of its body: opened on the first run that finds
# it, updated with the latest report on each later run, and closed with a comment on the first run
# where the record verifies again. Issues of records not given are left alone. --who adds who
# changed a record that drifted, and when: `strata verify --who` reads the audit log of Palette,
# and the issue body lists each write since the record. --dry-run prints each issue body and what
# would happen, and calls no host.
#
# Exit 0 when every record verified, 1 when any drifted or is unverified, 2 when any could not be
# checked or verify checked nothing; the issues are handled first, so a scheduled job shows the
# state too. A host call that fails stops the script with a nonzero status and what the host said.
# GitHub lists a new or closed issue a few seconds later, so two runs that close together can open
# a second issue for one record: schedule runs minutes apart.
#
# Hosts: GitHub through `gh`, and GitLab through the REST API v4 with `curl`. --host picks one;
# without it, the CI variables do: GITLAB_CI or CI_PROJECT_ID means GitLab, else GitHub. On GitLab
# the project is CI_PROJECT_ID or --repo, the API CI_API_V4_URL, and the token GITLAB_TOKEN: a job
# token cannot write issues.
#
# Needs strata (or STRATA_BIN), jq, and the host's tool: gh logged in, or curl and GITLAB_TOKEN.
# Every host call goes through `host`, so another code host is another set of the same functions.
set -euo pipefail

script=verify-report
die() { echo "$script: $*" >&2; exit 2; }
note() { echo "$script: $*" >&2; }

# quietly COMMAND...: run a host call and keep what it prints to itself, unless it fails
quietly() {
  local out
  out=$("$@" 2>&1) || { printf '%s\n' "$out" >&2; return 1; }
}

# ---- the hosts ---------------------------------------------------------------------------------
# One function for each thing the script needs from a code host. `host <name> [args]` calls the
# set of the chosen host. Each set prints issues as [{number, title, body}].

github_repo() { gh repo view --json nameWithOwner | jq -r .nameWithOwner; }
github_open_issues() {
  gh issue list --repo "$repo" --label "$label" --state open --limit 1000 --json number,title,body
}
github_repo_labels() { gh label list --repo "$repo" --limit 1000 --json name | jq -r '.[].name'; }
github_create_label() {
  quietly gh label create "$1" --repo "$repo" --color d93f0b \
    --description "$label_description"
}
# prints the new issue's number
github_open_issue() {
  local url
  url=$(gh issue create --repo "$repo" --title "$1" --body-file "$2" --label "$label")
  echo "${url##*/}"
}
github_update_issue() { quietly gh issue edit "$1" --repo "$repo" --title "$2" --body-file "$3"; }
github_close_issue() { quietly gh issue close "$1" --repo "$repo" --comment "$2"; }

# GitLab: the REST API v4. The project is a number or a path with its slashes encoded.
gitlab_api() { # METHOD PATH [curl arguments...]
  local method=$1 path=$2 status body
  shift 2
  body=$(mktemp)
  status=$(curl -sS -o "$body" -w '%{http_code}' -X "$method" \
    -H "PRIVATE-TOKEN: ${GITLAB_TOKEN:-}" "$api/projects/$project$path" "$@")
  case "$status" in
    2*) cat "$body"; rm -f "$body" ;;
    *) echo "$script: GitLab answered HTTP $status for $method $path: $(head -c 300 "$body")" >&2
       rm -f "$body"; return 1 ;;
  esac
}
gitlab_repo() { printf '%s' "${CI_PROJECT_PATH:-$project}"; }
gitlab_open_issues() {
  gitlab_api GET "/issues?labels=$label&state=opened&per_page=100" \
    | jq '[.[] | {number: .iid, title: .title, body: .description}]'
}
gitlab_repo_labels() { gitlab_api GET "/labels?per_page=100" | jq -r '.[].name'; }
gitlab_create_label() {
  quietly gitlab_api POST "/labels" --data-urlencode "name=$1" --data-urlencode "color=#d93f0b" \
    --data-urlencode "description=$label_description"
}
gitlab_open_issue() {
  local answer number
  answer=$(gitlab_api POST "/issues" --data-urlencode "title=$1" --data-urlencode "description@$2" \
    --data-urlencode "labels=$label")
  number=$(jq -r '.iid // empty' <<<"$answer")
  # without a number the next run cannot find the issue again, so say so instead of printing null
  [ -n "$number" ] || die "GitLab opened no issue for $1: $(head -c 200 <<<"$answer")"
  printf '%s' "$number"
}
gitlab_update_issue() {
  quietly gitlab_api PUT "/issues/$1" --data-urlencode "title=$2" --data-urlencode "description@$3"
}
gitlab_close_issue() {
  quietly gitlab_api POST "/issues/$1/notes" --data-urlencode "body=$2"
  quietly gitlab_api PUT "/issues/$1" --data-urlencode "state_event=close"
}

host() { local what=$1; shift; "${host_kind}_$what" "$@"; }

# ---- arguments -------------------------------------------------------------------------------
patterns=()
repo="${GITHUB_REPOSITORY:-}"
label="strata-drift"
label_description="strata: a record that drifted or could not be checked"
dry=""
host_kind=""
allow=()
who=()
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) repo=$2; shift 2 ;;
    --host) host_kind=$2; shift 2 ;;
    --label) label=$2; shift 2 ;;
    --dry-run) dry=1; shift ;;
    --allow-no-digest) allow=(--allow-no-digest); shift ;;
    --who) who=(--who); shift ;;
    -h|--help) sed -n '2,/^set -euo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --*) die "unknown option $1" ;;
    *) patterns+=("$1"); shift ;;
  esac
done
[ ${#patterns[@]} -gt 0 ] || die "usage: verify-report.sh <record or pattern>... [--repo OWNER/REPO] [--host github|gitlab] [--label NAME] [--allow-no-digest] [--who] [--dry-run]"

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
  [ -n "$dry" ] || [ -n "${GITLAB_TOKEN:-}" ] || die "set GITLAB_TOKEN: a job token cannot write issues"
fi

strata=${STRATA_BIN:-strata}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# ---- the check -------------------------------------------------------------------------------
code=0
"$strata" verify --summary --quiet --format json "${allow[@]+"${allow[@]}"}" "${patterns[@]}" > "$work/summary.json" 2> "$work/summary.err" || code=$?
# -s reads the file as an array: a jq older than 1.7 exits with 0 for `jq -e` on an empty file
if ! jq -es '.[0].schema == "strata-verify-summary/1"' "$work/summary.json" > /dev/null 2>&1; then
  cat "$work/summary.err" >&2
  die "strata verify checked nothing (exit $code); no issue was touched"
fi

if [ -z "$dry" ]; then
  [ -n "$repo" ] || repo=$(host repo)
  [ -n "$repo" ] || die "could not determine the project; pass --repo"
  host open_issues > "$work/issues.json"
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
  labels=$(host repo_labels)
  grep -Fxq "$label" <<<"$labels" || host create_label "$label"
  label_ready=1
}

unchanged=0 drifted=0 unverified=0 errors=0
actions=()
# the records come on descriptor 3, so a host call that reads stdin cannot take them
while IFS= read -r record <&3; do
  path=$(jq -r .path <<<"$record")
  status=$(jq -r .status <<<"$record")
  subject=$(jq -r 'if (.name // "") == "" then .path else "\(.name)@\(.version)" end' <<<"$record")
  body="$work/body.md"
  if [ "$status" != unchanged ]; then
    rc=0
    # --who only here: it reads the audit log, and only a record that did not verify has an issue
    "$strata" verify "$path" --quiet --format markdown "${allow[@]+"${allow[@]}"}" "${who[@]+"${who[@]}"}" > "$work/report.md" 2> "$work/report.err" || rc=$?
    # exit 1 is drift or unverified; the summary says which, unless the second check disagrees
    case "$rc" in
      0) status=unchanged ;;
      1) [ "$status" = unverified ] || status=drifted ;;
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
      host close_issue "$number" "\`$path\` verifies again: $subject is unchanged since $recorded (checked $now)."
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
    unverified)
      unverified=$((unverified + 1))
      title="strata verify: $subject cannot be verified ($path)"
      {
        echo "<!-- strata-verify: $path -->"
        echo "\`strata verify $path\` cannot verify the record, checked $now: a pack has no upstream digest, so a change of its archive does not show. \`--allow-no-digest\` accepts it."
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
    host update_issue "$number" "$title" "$body"
    actions+=("updated #$number for $path")
  else
    ensure_label
    number=$(host open_issue "$title" "$body")
    actions+=("opened #$number for $path")
  fi
done 3< <(jq -c '.records[]' "$work/summary.json")

total=$((unchanged + drifted + unverified + errors))
case "$total" in 1) records="1 record" ;; *) records="$total records" ;; esac
line="$records: $unchanged unchanged, $drifted drifted"
[ "$unverified" -eq 0 ] || line="$line, $unverified unverified"
line="$line, $errors could not be checked"
for a in "${actions[@]+"${actions[@]}"}"; do
  line="$line; $a"
done
note "$line"
if [ "$errors" -gt 0 ]; then exit 2; fi
if [ "$drifted" -gt 0 ] || [ "$unverified" -gt 0 ]; then exit 1; fi
exit 0

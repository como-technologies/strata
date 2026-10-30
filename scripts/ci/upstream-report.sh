#!/usr/bin/env bash
# Report the pack versions that records pin and that a registry offers a newer version of, as
# issues: a scheduled `strata behind`.
#
#   scripts/ci/upstream-report.sh <record or pattern>... [--repo OWNER/REPO] [--label NAME]
#                                 [--no-impact] [--save FILE] [--dry-run]
#
# Runs `strata behind --format json` over the records; strata expands a quoted pattern such as
# 'releases/**/*.json'. Each pack of a registry that a record pins behind the newest version it can
# move to has one open issue labeled NAME (default strata-behind), found by the marker
# <!-- strata-behind: <pack> <registry uid> --> on the first line of its body: opened on the first
# run that finds the pack behind, updated on each later run, and closed with a comment on the first
# run where each pin of the pack is current. The second line of the body keeps the pins as JSON,
# and the next run passes them to `strata behind --since`, so the issue names each version that
# arrived since its last update; the issue that a run opens says that this run is the first to find
# the pack behind. A pack that strata could not check keeps its issue as it is.
# Issues of packs that no record pins are left alone. --no-impact skips the comparison of each pin
# with the newest version. --save FILE keeps the report of the run. --dry-run prints each issue
# body and what would happen, and calls no host.
#
# Exit 0 when no pin is behind, 1 when one is, 2 when a pin could not be checked or strata checked
# nothing; the issues are handled first, so a scheduled job shows the state too. Behind is a
# decision and not an error: run this job apart from the drift report.
#
# Hosts: GitHub through `gh`, and GitLab through the REST API v4 with `curl`. --host picks one;
# without it, the CI variables do: GITLAB_CI or CI_PROJECT_ID means GitLab, else GitHub. On GitLab
# the project is CI_PROJECT_ID or --repo, the API CI_API_V4_URL, and the token GITLAB_TOKEN: a job
# token cannot write issues.
#
# Needs strata (or STRATA_BIN), jq, and the host's tool: gh logged in, or curl and GITLAB_TOKEN.
# The host functions are the ones of verify-report.sh.
set -euo pipefail

script=upstream-report
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
label="strata-behind"
label_description="strata: a pack version that a newer version of the registry replaces"
dry=""
host_kind=""
impact=()
save=""
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) repo=$2; shift 2 ;;
    --host) host_kind=$2; shift 2 ;;
    --label) label=$2; shift 2 ;;
    --dry-run) dry=1; shift ;;
    --no-impact) impact=(--no-impact); shift ;;
    --save) save=$2; shift 2 ;;
    -h|--help) sed -n '2,31p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --*) die "unknown option $1" ;;
    *) patterns+=("$1"); shift ;;
  esac
done
[ ${#patterns[@]} -gt 0 ] || die "usage: upstream-report.sh <record or pattern>... [--repo OWNER/REPO] [--host github|gitlab] [--label NAME] [--no-impact] [--save FILE] [--dry-run]"

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

# ---- the issues, and the previous run they keep ------------------------------------------------
echo "[]" > "$work/issues.json"
if [ -z "$dry" ]; then
  [ -n "$repo" ] || repo=$(host repo)
  [ -n "$repo" ] || die "could not determine the project; pass --repo"
  host open_issues > "$work/issues.json"
fi

# the second line of each issue of this script holds {generatedAt, pins} of its last update
jq '[.[] | (.body // "") | split("\n") | select((.[0] | rtrimstr("\r") | startswith("<!-- strata-behind: ")))
      | (.[1] // "" | rtrimstr("\r")) | select(startswith("<!-- strata-behind-state: "))
      | ltrimstr("<!-- strata-behind-state: ") | rtrimstr(" -->") | (try fromjson catch empty)]' \
  "$work/issues.json" > "$work/states.json"
since=()
if jq -e 'length > 0' "$work/states.json" > /dev/null; then
  jq '{schema: "strata-behind/1", tool: {name: "strata", version: ""},
       generatedAt: ([.[].generatedAt] | min), records: [], pins: [.[].pins[]],
       summary: {pins: 0, behind: 0, floating: 0, errors: 0, arrived: 0}}' \
    "$work/states.json" > "$work/previous.json"
  since=(--since "$work/previous.json")
fi

# ---- the check -------------------------------------------------------------------------------
code=0
"$strata" behind --quiet --format json "${impact[@]+"${impact[@]}"}" "${since[@]+"${since[@]}"}" \
  "${patterns[@]}" > "$work/report.json" 2> "$work/report.err" || code=$?
# -s reads the file as an array: a jq older than 1.7 exits with 0 for `jq -e` on an empty file
if ! jq -es '.[0].schema == "strata-behind/1"' "$work/report.json" > /dev/null 2>&1; then
  cat "$work/report.err" >&2
  die "strata behind checked nothing (exit $code); no issue was touched"
fi
[ -z "$save" ] || cp "$work/report.json" "$save"

# issue_for PACK REGISTRY -> the lowest open issue number whose body starts with their marker
issue_for() {
  jq -r --arg m "<!-- strata-behind: $1 $2 -->" \
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

behind=0 current=0 errors=0
actions=()
# one group for each pack of a registry, on descriptor 3, so a host call that reads stdin cannot
# take them
while IFS= read -r group <&3; do
  name=$(jq -r .name <<<"$group")
  uid=$(jq -r .registryUid <<<"$group")
  registry=$(jq -r 'if .registry == "" then .registryUid else .registry end' <<<"$group")
  what="$name in $registry"
  if jq -e '.pins | any(.error)' <<<"$group" > /dev/null && ! jq -e '.pins | any(.behind)' <<<"$group" > /dev/null; then
    errors=$((errors + 1))
    note "could not check $what: $(jq -r '[.pins[] | .error.message // empty] | first // "see strata behind"' <<<"$group")"
    continue
  fi
  if ! jq -e '.pins | any(.behind)' <<<"$group" > /dev/null; then
    current=$((current + 1))
    if [ -n "$dry" ]; then
      note "would close the open issue for $what, if any"
      continue
    fi
    number=$(issue_for "$name" "$uid")
    [ -n "$number" ] || continue
    host close_issue "$number" "Each pin of \`$name\` in $registry is at the newest version it can move to (checked $now)."
    actions+=("closed #$number for $what")
    continue
  fi
  behind=$((behind + 1))
  jq -e '.pins | any(.error)' <<<"$group" > /dev/null && errors=$((errors + 1))
  newest=$(jq -r '[.pins[] | select(.behind) | .newer[0].version] | first' <<<"$group")
  pinned=$(jq -r '[.pins[] | select(.behind) | .version] | join(", ")' <<<"$group")
  title="strata behind: $name $pinned behind $newest in $registry"
  body="$work/body.md"
  {
    echo "<!-- strata-behind: $name $uid -->"
    echo "<!-- strata-behind-state: $(jq -c --arg at "$(jq -r .generatedAt "$work/report.json")" \
      '{generatedAt: $at, pins: [.pins[] | del(.arrived)]}' <<<"$group") -->"
    echo "\`strata behind\` finds \`$name\` behind in $registry, checked $now."
    echo
    echo "| pinned | newer | bump | arrived | pinned by |"
    echo "|---|---|---|---|---|"
    jq -r '.pins[] | "| `\(.version)`\(if .floating then " (floating tag `\(.tag)`)" else "" end) | \(
        if .behind then [.newer[] | .version + ([(.state // empty), (.drops // [] | select(length > 0) | "drops \(join(" and "))"), (.listed // "" | select(. != "") | "listed \(.[0:10])")]
          | if length > 0 then " (\(join(", ")))" else "" end)] | join(", ")
        elif .error then "not checked: \(.error.message)" else "current" end) | \(
        if .bump then "\(.bump.to): \(.bump.impact) \(.bump.flags | join(" "))" else "-" end) | \(
        if .arrived == null then "-" elif (.arrived | length) == 0 then "none" else (.arrived | join(", ")) end) | \(
        [.pinnedBy[] | "`\(.path)` (\(.subject))"] | join("<br>")) |"' <<<"$group"
    echo
    if jq -e '.pins | any(.arrived != null)' <<<"$group" > /dev/null; then
      echo "**arrived** names the versions that arrived since the last update of this issue."
    else
      echo "No earlier issue holds this pack: this run is the first to find it behind."
    fi
  } > "$body"
  if [ -n "$dry" ]; then
    cat "$body"
    echo
    note "would open or update the issue for $what: $title"
    continue
  fi
  number=$(issue_for "$name" "$uid")
  if [ -n "$number" ]; then
    host update_issue "$number" "$title" "$body"
    actions+=("updated #$number for $what")
  else
    ensure_label
    number=$(host open_issue "$title" "$body")
    actions+=("opened #$number for $what")
  fi
done 3< <(jq -c '.pins | group_by([.name, .registryUid])[] | {name: .[0].name, registryUid: .[0].registryUid, registry: .[0].registry, pins: .}' "$work/report.json")

total=$((behind + current + errors))
case "$total" in 1) packs="1 pack" ;; *) packs="$total packs" ;; esac
line="$packs: $behind behind, $current current, $errors could not be checked"
for a in "${actions[@]+"${actions[@]}"}"; do
  line="$line; $a"
done
note "$line"
if [ "$errors" -gt 0 ]; then exit 2; fi
if [ "$behind" -gt 0 ]; then exit 1; fi
exit 0

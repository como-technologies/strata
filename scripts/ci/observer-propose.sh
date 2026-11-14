#!/usr/bin/env bash
# Open one pull request with the first-seen records of the observer, so that a review approves them.
#
#   scripts/ci/observer-propose.sh [--namespace NAME] [--dir DIR] [--base BRANCH] [--branch NAME]
#                                  [--repo OWNER/REPO] [--host github|gitlab] [--dry-run]
#
# Reads the four Secrets of the records with kubectl, in the namespace NAME (default
# strata-observer). When meta.json says first-seen, it writes each record to DIR (default
# releases/observer) as <profile>_<version>.json. When the files are not the files of the branch
# BASE (default: the default branch of the repository), it commits them on the branch NAME (default
# strata-observer-records), pushes that branch to origin, and opens one pull request to BASE. On
# GitLab, it opens a merge request. While that request is open, a later run changes nothing, so a
# change that a reviewer pushes to the branch stays. When meta.json says approved, the script
# changes nothing, because a pipeline published the records. --dry-run writes the files and prints
# the change. It does not commit, push, or call a host.
#
# After the merge, a job publishes the records of the merge, with the revision of the merge:
#   observer-records.sh --revision REV <record.json>... | kubectl replace -f -
#
# Exit 0 when the request is open or there is nothing to propose. Exit 2 when kubectl, git, or the
# host fails, with what it said. The script removes no file of DIR.
#
# Hosts: GitHub through `gh`, and GitLab through the REST API v4 with `curl`. --host picks one;
# without it, the CI variables do: GITLAB_CI or CI_PROJECT_ID means GitLab, else GitHub. On GitLab
# the project is CI_PROJECT_ID or --repo, the API CI_API_V4_URL, and the token GITLAB_TOKEN. The
# remote origin must take a push: the job that runs the script sets it.
#
# Needs kubectl that can get the four Secrets, jq, base64, gzip, git, and the tool of the host: gh
# logged in, or curl and GITLAB_TOKEN.
set -euo pipefail

script=observer-propose
die() { echo "$script: $*" >&2; exit 2; }
note() { echo "$script: $*" >&2; }

# quietly COMMAND...: run a host call and keep what it prints to itself, unless it fails
quietly() {
  local out
  out=$("$@" 2>&1) || { printf '%s\n' "$out" >&2; return 1; }
}

# The jq functions that write a text into markdown, as strata writes it. `html` is
# `text::html_text`: each `&`, `<`, and `>` is an entity, so a tag of HTML shows as text. `cell` is
# `text::markdown_cell`: `marks` puts a `\` before each mark of markdown, and `table` escapes each
# `|` and writes a line break as a space. So a link, an emphasis, or a code span of the text shows
# as text. A run of `_` with a letter or a digit on each side starts no emphasis, so `marks` keeps
# it as it is. `mentions` puts a zero width space after each `@` that can start a mention of
# GitHub, so a `@name` of the text notifies nobody. `span` is `text::code_span`: its fence is
# longer than each run of backticks in the text. `code` is `text::code_cell`, a `span` in a cell
# of a table. `fence` is the fence of `text::code_block`, so that no line of the text ends the
# block.
markdown_jq='def html: split("&") | join("&amp;") | split("<") | join("&lt;") | split(">") | join("&gt;");
  def marks: gsub("(?<m>[\\\\`*\\[\\]~]|(?<![[:alnum:]_])_+|_+(?![[:alnum:]_]))"; .m | explode | map(92, .) | implode);
  def table: split("|") | join("\\|") | split("\n") | join(" ");
  def mentions: gsub("(?<![A-Za-z0-9])@(?=[A-Za-z0-9])"; "@&#8203;");
  def cell: marks | html | mentions | table;
  def backticks($n): [range($n)] | map("`") | join("");
  def ticks: [scan("`+") | length] | max // 0;
  def span: backticks(ticks + 1) as $f | (if startswith("`") or endswith("`") then " " else "" end) as $p
    | $f + $p + . + $p + $f;
  def code: span | table;
  def fence: backticks([ticks + 1, 3] | max);'

# md FUNCTION TEXT: TEXT through the function FUNCTION of markdown_jq, such as `cell` or `span`
md() { jq -rn --arg t "$2" "$markdown_jq \$t | $1"; }

# ---- the hosts ---------------------------------------------------------------------------------
# One function for each thing the script needs from a code host. `host <name> [args]` calls the
# set of the chosen host.

github_default_branch() {
  gh repo view "$repo" --json defaultBranchRef --jq .defaultBranchRef.name
}
# prints the number of the open request from the branch to the base, or nothing
github_find_request() {
  gh pr list --repo "$repo" --head "$branch" --base "$base" --state open --json number \
    --jq '.[0].number // empty'
}
# prints the address of the new request
github_open_request() {
  gh pr create --repo "$repo" --base "$base" --head "$branch" --title "$1" --body-file "$2"
}

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
gitlab_default_branch() { gitlab_api GET "" | jq -r '.default_branch // empty'; }
gitlab_find_request() {
  gitlab_api GET "/merge_requests?state=opened&source_branch=$branch&target_branch=$base" \
    | jq -r '.[0].iid // empty'
}
gitlab_open_request() {
  local answer address
  answer=$(gitlab_api POST "/merge_requests" --data-urlencode "source_branch=$branch" \
    --data-urlencode "target_branch=$base" --data-urlencode "title=$1" \
    --data-urlencode "description@$2" --data-urlencode "remove_source_branch=true")
  address=$(jq -r '.web_url // empty' <<<"$answer")
  [ -n "$address" ] || die "GitLab opened no merge request: $(head -c 200 <<<"$answer")"
  printf '%s\n' "$address"
}

host() { local what=$1; shift; "${host_kind}_$what" "$@"; }

# ---- arguments -------------------------------------------------------------------------------
namespace=strata-observer
dir=releases/observer
base=""
branch=strata-observer-records
repo="${GITHUB_REPOSITORY:-}"
host_kind=""
dry=""
while [ $# -gt 0 ]; do
  case "$1" in
    --namespace) namespace=${2:?--namespace needs a value}; shift 2 ;;
    --dir) dir=${2:?--dir needs a value}; shift 2 ;;
    --base) base=${2:?--base needs a value}; shift 2 ;;
    --branch) branch=${2:?--branch needs a value}; shift 2 ;;
    --repo) repo=${2:?--repo needs a value}; shift 2 ;;
    --host) host_kind=${2:?--host needs a value}; shift 2 ;;
    --dry-run) dry=1; shift ;;
    -h|--help) sed -n '2,/^set -euo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument $1" ;;
  esac
done

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
  [ -n "$dry" ] || [ -n "${GITLAB_TOKEN:-}" ] || die "set GITLAB_TOKEN: a job token cannot open a merge request"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# ---- the records of the cluster ----------------------------------------------------------------
for i in 0 1 2 3; do
  kubectl -n "$namespace" get secret "strata-observer-records-$i" -o json \
    > "$work/secret-$i.json" 2> "$work/kubectl.err" \
    || die "kubectl cannot get the Secret strata-observer-records-$i in the namespace $namespace: $(head -c 300 "$work/kubectl.err")"
done
meta=$(jq -r '.data["meta.json"] // empty' "$work/secret-0.json" | base64 -d)
[ -n "$meta" ] || { note "the Secrets hold no record yet, so there is nothing to propose"; exit 0; }
baseline=$(jq -r '.baseline // empty' <<<"$meta")
if [ "$baseline" != first-seen ]; then
  note "the records are $baseline, at the revision $(jq -r '.revision // "none"' <<<"$meta"), so there is nothing to propose"
  exit 0
fi
cluster=$(jq -r '.firstSeenFrom // "unknown"' <<<"$meta")

if [ -z "$dry" ]; then
  [ -n "$base" ] || base=$(host default_branch) || die "the host gave no default branch: give --base"
  [ -n "$base" ] || die "the host gave no default branch: give --base"
  # an open request keeps its branch as the review left it
  number=$(host find_request) || die "the host cannot list the requests of the branch $branch"
  if [ -n "$number" ]; then
    note "the request $number of the branch $branch is open, so the script changes nothing: merge it, or close it for a new one"
    exit 0
  fi
  quietly git fetch --quiet origin "$base" || die "git cannot fetch the branch $base from origin"
  quietly git checkout --quiet -B "$branch" FETCH_HEAD || die "git cannot make the branch $branch"
fi

mkdir -p "$dir"
files=()
for i in 0 1 2 3; do
  while IFS= read -r key; do
    file="$dir/${key%.gz}"
    jq -r --arg k "$key" '.data[$k]' "$work/secret-$i.json" | base64 -d | gzip -dc > "$file" \
      || die "the key $key of the Secret strata-observer-records-$i is not a gzipped record"
    jq -e '.schema == "strata-snapshot/1"' "$file" >/dev/null 2>&1 \
      || die "the key $key of the Secret strata-observer-records-$i is not a record of strata"
    files+=("$file")
  done < <(jq -r '.data // {} | keys[] | select(endswith(".json.gz"))' "$work/secret-$i.json")
done
[ "${#files[@]}" -gt 0 ] || { note "meta.json says first-seen, and no Secret holds a record"; exit 0; }

changed=$(git status --porcelain --untracked-files=all -- "${files[@]}")
if [ -z "$changed" ]; then
  note "the branch holds these ${#files[@]} records already, so there is nothing to propose"
  exit 0
fi
if [ -n "$dry" ]; then
  note "dry run: these records are new or changed, and a run opens a request for them:"
  printf '%s\n' "$changed"
  exit 0
fi

title="Approve the first-seen records of the observer"
{
  printf '<!-- strata-observer-records -->\n'
  printf 'The observer of the cluster %s recorded what each of its profiles holds, ' "$(md span "$cluster")"
  printf 'when it first saw the cluster. No person approved that content.\n\n'
  printf 'Review each record. Merge this request to approve the records. Then the pipeline '
  printf 'publishes them with the revision of the merge, and the page of the observer removes '
  printf 'the notice.\n\nThe records:\n\n'
  for f in "${files[@]}"; do printf -- '- %s\n' "$(md span "$f")"; done
} > "$work/body.md"

quietly git add -- "${files[@]}" || die "git cannot add the records"
quietly git commit --quiet -m "$title" || die "git cannot commit the records: set user.name and user.email"
quietly git push --quiet --force origin "$branch" || die "git cannot push the branch $branch to origin"
address=$(host open_request "$title" "$work/body.md") || die "the host cannot open a request"
note "opened $address with the ${#files[@]} records"

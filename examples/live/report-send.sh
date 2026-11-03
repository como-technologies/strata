#!/usr/bin/env bash
# The send of the report (#397): strata posts the report of one unknown value to the endpoint that
# runs on Cloud Run, and the endpoint merges the reports into one issue of the label unknown-value.
#
#   just live-report-send URL [--repo OWNER/REPO]
#
# URL is the address that deploy/report/deploy.sh printed. The check reads and writes no Palette
# tenant. It serves the tenant of the chapter unknown-value of the book on this machine: a profile
# with a profile type that strata does not know. It runs strata against that tenant with
# STRATA_REPORT=1 and STRATA_REPORT_URL=URL, in a state folder of its own. So the send is real, and
# the value is the made-up value of the book. The check opens or changes the issue of that value in
# OWNER/REPO through the endpoint. The default repository is the one that report-endpoint/Cargo.toml
# names, which is the one that the endpoint writes to. It needs a gh login to read the issue, jq,
# curl, the strata release binary, and the xtask binary: just build and just stand-in.
# The run changes HOME, so the script keeps the login of gh: it sets GH_CONFIG_DIR to the folder of
# the login before the change. A GH_TOKEN in the environment works too. When gh cannot read the
# issues, the script stops with exit 2 before the first send.
#
#   1  the first send: the run posts the report, and its output and exit code are those of a run
#   2  the issue: one issue holds the marker of the value, and has the label
#   3  the second send: the same issue holds the count of both sends
#   4  the same report twice, with one key: the issue counts it one time
#
# The endpoint answers 202 and then writes the issue, and GitHub lists a change some seconds later.
# So the script waits for the issue: WAIT_TRIES tries (default 24), WAIT_SECONDS apart (default 5).
# The check leaves the issue open. Close it with a comment after you read it. Results go under
# runs/<UTC time>/report-send/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=report-send
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
. "$live/github.sh"

url=""
# the label of the issues of the endpoint, and the repository of its manifest: the endpoint has the
# same two texts as its defaults, and a test of the endpoint makes sure that they agree
label=unknown-value
repo=$(sed -n 's|^repository = "https://github.com/\(.*\)"$|\1|p' "$live/../../report-endpoint/Cargo.toml")
[ -n "$repo" ] || die "report-endpoint/Cargo.toml names no repository"
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) [ $# -ge 2 ] || die "--repo needs a value"; repo=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    -*) usage >&2; die "unknown option $1" ;;
    *) [ -z "$url" ] || die "one address only: $url and $1"; url=$1; shift ;;
  esac
done
[ -n "$url" ] || { usage >&2; die "give the address of the endpoint"; }
[ -x "$strata" ] || die "no strata binary at $strata"
xtask=${XTASK_BIN:-$live/../../target/debug/xtask}
[ -x "$xtask" ] || die "no xtask binary at $xtask; run just stand-in"
command -v gh > /dev/null || die "gh is not installed"
command -v jq > /dev/null || die "jq is not installed"
command -v curl > /dev/null || die "curl is not installed"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/report-send}
mkdir -p "$run" && cd "$run" || exit 2
summary() {
  write_summary "report send test" ": the endpoint at \`$url\`, the issues of \`$repo\`."
}
trap summary EXIT

# the stand-in tenant of the chapter unknown-value; the coprocess holds the standard input of the
# server open, and the server stops when this script ends
"$xtask" harvest tenants > harvest.log 2>&1 || die "xtask harvest failed; see $PWD/harvest.log"
[ -d tenants/unknown-value ] || die "the harvest has no chapter unknown-value; see $PWD/harvest.log"
coproc STAND_IN { "$xtask" serve tenants/unknown-value port 2> serve.log; }
for _ in $(seq 1 100); do [ -s port ] && break; sleep 0.1; done
[ -s port ] || die "the stand-in did not start; see $PWD/serve.log"

# gh finds its login in the folder gh of the config folder of the home. The next line changes HOME,
# so the folder stays where it is now. gh reads GH_TOKEN from the environment, and this script keeps it.
export GH_CONFIG_DIR=${GH_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/gh}
# the run sends from a state folder and a home of its own, so it reads no setting of this machine
export XDG_STATE_HOME=$run/state HOME=$run/home
mkdir -p "$HOME"
export PALETTE_API="http://127.0.0.1:$(cat port)" PALETTE_API_KEY=live-check PALETTE_PROJECT=dev
unset PALETTE_PROJECT_UID STRATA_QUIET CI
export STRATA_REPORT=1 STRATA_REPORT_URL="$url"

# the checks read the issues with gh, so a send with no login can only fail them
gh issue list --repo "$repo" --label "$label" --limit 1 > /dev/null 2> gh.err \
  || die "gh cannot read the issues of $repo; log in with gh auth login, or set GH_TOKEN; see $PWD/gh.err"

marker='<!-- strata-report: values ProfileType later-type -->'
# issues_of_value: the issues of the label of the endpoint whose first line is the marker, as JSON
issues_of_value() {
  gh issue list --repo "$repo" --label "$label" --state all --limit 500 --json number,state,body \
    | jq --arg m "$marker" "$first_line_jq"'[.[] | select(first_line == $m)]'
}
# lines_of_issue: the number in the row "Lines" of the one issue of the value
lines_of_issue() {
  issues_of_value | jq -r '.[0].body | split("\n")[] | select(startswith("| Lines | ")) | split(" ")[3]'
}

# wait_for COMMAND WANT: wait until COMMAND prints WANT. The endpoint writes after its answer.
wait_for() {
  local command=$1 want=$2 try
  for try in $(seq 1 "${WAIT_TRIES:-24}"); do
    [ "$($command 2> /dev/null)" = "$want" ] && { echo "waited $try tries for $command = $want" >> steps.log; return 0; }
    sleep "${WAIT_SECONDS:-5}"
  done
  echo "gave up after ${WAIT_TRIES:-24} tries for $command = $want" >> steps.log
  return 1
}

heading() { echo "== $*" | tee -a steps.log; }

heading "1: the first send"
before=$(issues_of_value | jq 'length')
before_lines=$(lines_of_issue 2> /dev/null || echo 0)
check send-1 0 '#397: a run with STRATA_REPORT=1 posts the report and exits as a run with no send' -- \
  bash -c "set -o pipefail; \"$strata\" list profiles --packs --json | jq -e '.profiles[] | select(.name == \"aws-stack\")'"
check send-1-says 0 '#397: the run says that it sent the report, and the log holds one line' -- \
  bash -c "grep -q '^strata: sent the report of the unknown values\$' send-1.err && [ \$(wc -l < \"$XDG_STATE_HOME/strata/strata-unknown.jsonl\") -eq 1 ]"
check send-1-marked 0 '#397: the endpoint took the report, so strata marked the line as sent' -- \
  bash -c "[ \"\$(cat \"$XDG_STATE_HOME/strata/strata-unknown.sent\")\" = 1 ]"
wait_for lines_of_issue $((before_lines + 1)) || true

heading "2: the issue"
check issue 0 '#397: one issue holds the marker of the value, it has the label unknown-value, and it has the row Lines' -- \
  bash -c "$(declare -f issues_of_value); marker='$marker'; repo='$repo'; label='$label'; issues_of_value | jq -e 'length == 1 and (.[0].body | contains(\"| Lines | \"))'"
if [ "$before" -eq 0 ]; then
  check issue-new 0 '#397: the first send opened the issue, and its count is 1' -- \
    bash -c "$(declare -f issues_of_value lines_of_issue); marker='$marker'; repo='$repo'; label='$label'; [ \"\$(lines_of_issue)\" = 1 ]"
fi
number=$(issues_of_value | jq -r '.[0].number // empty')
echo "the issue is $repo#$number" | tee -a steps.log

heading "3: the second send"
after_first=$(lines_of_issue 2> /dev/null || echo 0)
check send-2 0 '#397: a second run posts the report again' -- \
  bash -c "set -o pipefail; \"$strata\" list profiles --packs --json | jq -e '.profiles[] | select(.name == \"aws-stack\")'"
wait_for lines_of_issue $((after_first + 1)) || true
check issue-2 0 '#397: still one issue for the value, and its count grew by the line of the second run' -- \
  bash -c "$(declare -f issues_of_value lines_of_issue); marker='$marker'; repo='$repo'; label='$label'; \
    [ \$(issues_of_value | jq length) -eq 1 ] && [ \"\$(lines_of_issue)\" -eq $((after_first + 1)) ]"
echo "lines before the check: $before_lines; after the first send: $after_first" | tee -a steps.log

heading "4: the same report twice"
# a key that is new in each run, so that the first try counts and the second does not
key=$(printf 'report-send %s' "$run" | sha256sum | cut -d ' ' -f 1)
version=$("$strata" --version | awk '{print $2}')
repeat_body=$(jq -nc --arg v "$version" '{schema: "strata-report/1", unknown: [{strata: $v, set: "ProfileType", value: "later-type", route: "/v1/clusterprofiles/{uid}", count: 1}]}')
after_second=$(lines_of_issue 2> /dev/null || echo 0)
# post_report NAME: post the report of the made-up value with the key, and print the HTTP status
post_report() {
  curl -sS -o "$1.json" -w '%{http_code}' -H 'Content-Type: application/json' -H "Idempotency-Key: $key" -d "$repeat_body" "$url"
}
check repeat-1 0 'The endpoint takes a report with a key and answers 202, before it writes the issue' -- \
  bash -c "$(declare -f post_report); key='$key'; repeat_body='$repeat_body'; url='$url'; [ \"\$(post_report repeat-1)\" = 202 ]"
wait_for lines_of_issue $((after_second + 1)) || true
check repeat-2 0 'The same report with the same key gets 200 and duplicate, because the endpoint counted it' -- \
  bash -c "$(declare -f post_report); key='$key'; repeat_body='$repeat_body'; url='$url'; \
    [ \"\$(post_report repeat-2)\" = 200 ] && jq -e '.duplicate == true' repeat-2.json"
check repeat-count 0 'The issue counted the report one time: its lines grew by one and not by two' -- \
  bash -c "$(declare -f issues_of_value lines_of_issue); marker='$marker'; repo='$repo'; label='$label'; [ \"\$(lines_of_issue)\" -eq $((after_second + 1)) ]"

[ "$failures" -eq 0 ] || exit 1
exit 0

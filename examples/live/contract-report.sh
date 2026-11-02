#!/usr/bin/env bash
# Report the facts that a contract run found different, the values that it found unknown, and the
# releases that it found new, as
# issues of the repository.
#
#   examples/live/contract-report.sh <contract.json> [--repo OWNER/REPO] [--label NAME]
#                                     [--cloud-label NAME] [--milestone NAME] [--dry-run]
#
# Reads the document strata-contract/1 that contract.sh writes. A fact with the outcome differs
# has one open issue labeled NAME (default palette-drift). The script finds the issue by the marker
# <!-- strata-contract: <fact id> --> on the first line of its body. The first run that finds the
# difference opens the issue in the milestone NAME (default Backlog). Each later run updates the
# issue. The first run where the fact is the same again closes it with a comment. A fact that the
# run skipped or could not check keeps its issue as it is. Issues of facts that the document does
# not list are left alone.
#
# A fact whose answer holds a value of a closed set that neither the fact nor the stand-in names
# has one open issue for each such set. The title is "Palette has a new value of <set> in <fact
# id>", and the issue has the label NAME too. The marker is <!-- strata-contract: values <fact id>
# <set> -->. The script updates the issue while the value is there. It closes the issue when the
# run checked the fact and found no such value. A set is the field that the fact names in
# reads.values.
#
# A fact of the kind cloud is a call of the tool of a cloud. Its issue has the label of
# --cloud-label (default cloud-drift) and the marker of its id. It says that the answer of the cloud
# differs from the answers that the fixtures of the rules hold, or lacks a value of a table.
#
# A release with new set to true has one issue, which the script opens once. The marker is
# <!-- strata-contract: release <repo> <tag> -->. The issue asks to prove the facts again on the
# new release. A person closes it.
#
# Each label is created when the repository has none. --dry-run prints each issue body and what
# the script does, and calls no host.
#
# Exit 0 when no fact differs, no value is unknown, and no release is new. Exit 1 when one is: the
# issues are handled.
# Exit 3 when the document holds a fact or a release that the run could not check. The issues are
# handled first, so a scheduled job shows the state too, and this exit says that they are done.
# Exit 2 when the document is not a strata-contract/1 document, or when a host call fails. The
# script prints what the host said. The weekly job keeps the tags of the releases when the exit is
# not 2, so a release issue that a person closed does not open again.
#
# The host layer is the one of scripts/ci/verify-report.sh. This script reads its functions from
# that file, so a change to the host layer reaches this script. This script is not shipped, so it
# can read a file of the repository, which a shipped script cannot. Only GitHub is a host here.
#
# Needs jq, and gh logged in to the repository.
set -Eeuo pipefail
# a command that fails, such as a host call, ends the script with 2, never with the 1 of a difference
trap 'exit 2' ERR

script=contract-report
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# ---- the host layer ----------------------------------------------------------------------------
# From `die() {` to `host() {` of verify-report.sh: die, note, quietly, and a set of functions for
# each host. It needs $repo, $label, and $label_description, which are set below.
layer=$(sed -n '/^die() {/,/^host() /p' "$here/../../scripts/ci/verify-report.sh")
case "$layer" in
  *github_open_issue*host\(\)*) ;;
  *) echo "$script: scripts/ci/verify-report.sh has no host layer to read" >&2; exit 2 ;;
esac
eval "$layer"

# ---- arguments ---------------------------------------------------------------------------------
document=""
repo="${GITHUB_REPOSITORY:-}"
label="palette-drift"
label_description="strata: a fact of Palette that differs from the stand-in"
cloud_label="cloud-drift"
cloud_description="strata: a call of a cloud whose answer differs from the fixtures of the rules"
milestone="Backlog"
dry=""
host_kind=github
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) repo=$2; shift 2 ;;
    --label) label=$2; shift 2 ;;
    --cloud-label) cloud_label=$2; shift 2 ;;
    --milestone) milestone=$2; shift 2 ;;
    --dry-run) dry=1; shift ;;
    -h|--help) sed -n '2,/^set -Eeuo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --*) die "unknown option $1" ;;
    *) document=$1; shift ;;
  esac
done
[ -n "$document" ] || die "usage: contract-report.sh <contract.json> [--repo OWNER/REPO] [--label NAME] [--cloud-label NAME] [--milestone NAME] [--dry-run]"
[ -f "$document" ] || die "$document is not a file"
command -v jq > /dev/null || die "jq is not installed"
# -s reads the file as an array: a jq older than 1.7 exits with 0 for `jq -e` on an empty file
jq -es '.[0].schema == "strata-contract/1"' "$document" > /dev/null 2>&1 \
  || die "$document is not a strata-contract/1 document; no issue was touched"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
checked=$(jq -r .checkedAt "$document")
target=$(jq -r .target "$document")

# use_label KIND: the host functions read $label and $label_description, so the label of the kind of
# the fact goes there before a call
palette_label=$label
palette_description=$label_description
use_label() {
  if [ "$1" = cloud ]; then
    label=$cloud_label
    label_description=$cloud_description
  else
    label=$palette_label
    label_description=$palette_description
  fi
}

# remote_repo -> OWNER/REPO of the origin remote of the checkout in the current directory, when the
# remote is on GitHub, or nothing. It reads a local file, so a dry run still calls no host.
remote_repo() {
  git remote get-url origin 2> /dev/null \
    | sed -nE 's#^(https?|ssh|git)://([^/@]+@)?github\.com/([^/]+/[^/]+)$#\3#p; s#^[^/@:]+@github\.com:([^/]+/[^/]+)$#\1#p' \
    | sed -E 's#\.git$##' || true
}

if [ -z "$dry" ]; then
  [ -n "$repo" ] || repo=$(host repo)
  [ -n "$repo" ] || die "could not determine the project; pass --repo"
  host open_issues > "$work/issues.json"
  # the issues of the calls of the clouds have their own label, so they are listed apart
  if jq -e '[.facts[]? | select(.kind == "cloud" and (.outcome == "differs" or .outcome == "same"))] | length > 0' "$document" > /dev/null; then
    use_label cloud
    host open_issues > "$work/cloud-issues.json"
    use_label request
    jq -s 'add' "$work/issues.json" "$work/cloud-issues.json" > "$work/all-issues.json"
    mv "$work/all-issues.json" "$work/issues.json"
  fi
else
  # a dry run asks no host, so the links name the project of the checkout
  [ -n "$repo" ] || repo=$(remote_repo)
  [ -n "$repo" ] || note "dry run: the project is unknown, so the links name OWNER/REPO; pass --repo"
fi

page="https://github.com/${repo:-OWNER/REPO}/blob/main/docs/src/development/facts.md#the-weekly-contract-run"

# issue_for MARKER -> the lowest open issue number whose body starts with MARKER
issue_for() {
  jq -r --arg m "$1" \
    '[.[] | select(((.body // "") | split("\n")[0] | rtrimstr("\r")) == $m) | .number] | min // empty' \
    "$work/issues.json"
}

# the labels that the repository has, as far as this run knows
labels_ready=" "
ensure_label() {
  case "$labels_ready" in *" $label "*) return 0 ;; esac
  local labels
  labels=$(host repo_labels)
  grep -Fxq "$label" <<<"$labels" || host create_label "$label"
  labels_ready="$labels_ready$label "
}

actions=()
# open_or_update MARKER TITLE BODYFILE WHAT: the issue of MARKER gets the title and the body; an
# issue that does not exist is opened in the milestone
open_or_update() {
  local marker=$1 title=$2 body=$3 what=$4 number
  if [ -n "$dry" ]; then
    cat "$body"
    echo
    note "dry run: opens or updates the issue for $what: $title"
    return 0
  fi
  number=$(issue_for "$marker")
  if [ -n "$number" ]; then
    host update_issue "$number" "$title" "$body"
    actions+=("updated #$number for $what")
  else
    ensure_label
    number=$(host open_issue "$title" "$body")
    actions+=("opened #$number for $what")
  fi
}

# ---- the unknown values ------------------------------------------------------------------------
values_page="https://github.com/${repo:-OWNER/REPO}/blob/main/docs/src/development/facts.md#what-to-do-with-a-new-value"

# values_issues ID ENTRY: for each closed set of the fact ID that holds a value which neither the
# fact nor the stand-in names, open or update one issue. For each set that holds no such value now,
# close its open issue, when the run checked the fact. $unknown counts each value.
values_issues() {
  local id=$1 entry=$2 outcome prefix set_entry field number
  local -a current=()
  prefix="<!-- strata-contract: values $id "
  while IFS= read -r set_entry <&4; do
    field=$(jq -r .set <<<"$set_entry")
    current+=("$field")
    {
      echo "$prefix$field -->"
      jq -r --arg checked "$checked" --arg target "$target" --arg page "$values_page" --arg id "$id" \
        --arg request "$(jq -r .request <<<"$entry")" --argjson live "$(jq -c .live <<<"$entry")" \
        --argjson pages "$(jq -c .pages <<<"$entry")" '
        "The contract run of \($checked) found that the tenant `\($target)` gives new values for `\(.set)` in the answer to `\($request)`. Neither the fact `\($id)` nor the stand-in names them.",
        "",
        "| New value |",
        "| --- |",
        (.values[] | "| `\(.)` |"),
        "",
        "The fact and the stand-in name: \(.known | join(", ")).",
        "",
        "Strata does not know these values, so its answers for them can be wrong.",
        "Prove the fact again with its live checks: \(if ($live | length) > 0 then ($live | map("`\(.)`") | join(", ")) else "the fact has none" end).",
        "The pages of the book that state the fact: \($pages | map("`docs/src/\(.)`") | join(", ")).",
        "",
        "Then add each value to the closed set of strata, or write why not. [What to do with a new value](\($page)) says how."
      ' <<<"$set_entry"
    } > "$work/body.md"
    open_or_update "$prefix$field -->" "Palette has a new value of \`$field\` in \`$id\`" "$work/body.md" "$id $field"
  done 4< <(jq -c '.unknownValues[]?' <<<"$entry")
  [ -z "$dry" ] || return 0
  outcome=$(jq -r .outcome <<<"$entry")
  case "$outcome" in same|differs) ;; *) return 0 ;; esac
  # the open issues of this fact, as: number, a tab, and the set that the marker names
  while IFS=$'\t' read -r number field <&4; do
    case " ${current[*]-} " in *" $field "*) continue ;; esac
    host close_issue "$number" "No answer of the tenant holds a new value for \`$field\` in \`$id\` now (checked $checked)."
    actions+=("closed #$number for $id $field")
  done 4< <(jq -r --arg p "$prefix" '
    .[] | ((.body // "") | split("\n")[0] | rtrimstr("\r")) as $first
    | select($first | startswith($p))
    | "\(.number)\t\($first | ltrimstr($p) | rtrimstr(" -->"))"' "$work/issues.json")
}

# ---- the facts ---------------------------------------------------------------------------------
# palette_body ENTRY, cloud_body ENTRY: the body of the issue of a fact that differs, after its marker
palette_body() {
  jq -r --arg checked "$checked" --arg target "$target" --arg page "$page" '
    "The contract run of \($checked) found that the tenant `\($target)` answers `\(.request)` in another way than the stand-in.",
    "",
    "| Difference | Field | Expected | Found |",
    "| --- | --- | --- | --- |",
    (.differences[] | "| \(.kind) | `\(.path)` | \(.expected | join(", ")) | \(.found | join(", ")) |"),
    "",
    "Expected is what the stand-in has. For a `name`, it is the names that strata needs and the answer lacks. Found is what the tenant has.",
    "",
    "Prove the fact again with its live checks: \(if (.live | length) > 0 then (.live | map("`\(.)`") | join(", ")) else "the fact has none" end).",
    "The pages of the book that state the fact: \(.pages | map("`docs/src/\(.)`") | join(", ")).",
    "",
    "Then change the stand-in and the page, or record a fault of Palette in the local findings log. [The weekly contract run](\($page)) says how."
  ' <<<"$1"
}
cloud_body() {
  jq -r --arg checked "$checked" --arg page "$page" '
    "The contract run of \($checked) sent the call `\(.request)` in the lab account. The answer differs from the answers that the fixtures of the rules hold, or lacks a value that a rule holds in a table.",
    "",
    "| Difference | Field | The rules expect | The cloud answers with |",
    "| --- | --- | --- | --- |",
    (.differences[] | "| \(.kind) | `\(.path)` | \(.expected | join(", ")) | \(.found | join(", ")) |"),
    "",
    "Send the call by hand in the lab account, and read the answer. The rules of the cloud tier that read the fact: \((.rules // []) | map("`\(.)`") | join(", ")).",
    "The pages of the book that state the fact: \(.pages | map("`docs/src/\(.)`") | join(", ")).",
    "",
    "Then change the rules, the fixtures, the table, and the page together, so that they agree with the cloud. [The weekly contract run](\($page)) says how."
  ' <<<"$1"
}

while IFS= read -r entry <&3; do
  id=$(jq -r .id <<<"$entry")
  marker="<!-- strata-contract: $id -->"
  kind=$(jq -r '.kind // "request"' <<<"$entry")
  use_label "$kind"
  values_issues "$id" "$entry"
  case "$(jq -r .outcome <<<"$entry")" in
    differs)
      if [ "$kind" = cloud ]; then
        { echo "$marker"; cloud_body "$entry"; } > "$work/body.md"
        title="Cloud drift: \`$id\` differs from the fixtures of the rules"
      else
        { echo "$marker"; palette_body "$entry"; } > "$work/body.md"
        title="Palette drift: \`$id\` differs from the stand-in"
      fi
      open_or_update "$marker" "$title" "$work/body.md" "$id"
      ;;
    same)
      if [ -n "$dry" ]; then
        note "dry run: closes the open issue for $id, if any"
        continue
      fi
      number=$(issue_for "$marker")
      [ -n "$number" ] || continue
      if [ "$kind" = cloud ]; then against="the fixtures of the rules"; else against="the stand-in"; fi
      host close_issue "$number" "\`$id\` is the same as $against again (checked $checked)."
      actions+=("closed #$number for $id")
      ;;
  esac
done 3< <(jq -c '.facts[]' "$document")
use_label request

# ---- the releases ------------------------------------------------------------------------------
news=0 release_errors=0
while IFS= read -r release <&3; do
  repository=$(jq -r .repo <<<"$release")
  tag=$(jq -r .tag <<<"$release")
  seen=$(jq -r .seen <<<"$release")
  news=$((news + 1))
  marker="<!-- strata-contract: release $repository $tag -->"
  if [ -z "$dry" ] && [ -n "$(issue_for "$marker")" ]; then
    continue
  fi
  cat > "$work/body.md" <<EOF
$marker
The latest release of \`$repository\` is \`$tag\`. The last contract run saw \`$seen\`. The contract run of $checked found the new tag.

A new release can change what Palette does. Prove the facts of Palette again on this release:

1. Read the last contract run, and the issues of the label \`$label\`.
2. Run the live example, and the live scripts that the facts name.
3. Change the stand-in and the page for each fact that Palette changed.

[The weekly contract run]($page) says how. Close this issue when the facts hold on the release.
EOF
  open_or_update "$marker" "Prove the facts again on $repository $tag" "$work/body.md" "release $repository $tag"
done 3< <(jq -c '.releases[]? | select(.new == true)' "$document")
release_errors=$(jq '[.releases[]? | select(.error != null)] | length' "$document")

# the document holds the count of each outcome in `summary`, so the script counts nothing again
read -r differs errors unknown < <(jq -r '.summary | [.differs, .error, (.unknownValues // 0)] | @tsv' "$document")
line=$(jq -r -f "$here/contract-summary.jq" "$document")
[ "$news" -eq 0 ] || line="$line; $news new release"
for a in "${actions[@]+"${actions[@]}"}"; do
  line="$line; $a"
done
note "$line"
if [ "$errors" -gt 0 ] || [ "$release_errors" -gt 0 ]; then exit 3; fi
if [ "$differs" -gt 0 ] || [ "$unknown" -gt 0 ] || [ "$news" -gt 0 ]; then exit 1; fi
exit 0

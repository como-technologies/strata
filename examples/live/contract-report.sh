#!/usr/bin/env bash
# Report the facts that a contract run found different, and the releases that it found new, as
# issues of the repository.
#
#   examples/live/contract-report.sh <contract.json> [--repo OWNER/REPO] [--label NAME]
#                                     [--milestone NAME] [--dry-run]
#
# Reads the document strata-contract/1 that contract.sh writes. A fact with the outcome differs
# has one open issue labeled NAME (default palette-drift). The script finds the issue by the marker
# <!-- strata-contract: <fact id> --> on the first line of its body. The first run that finds the
# difference opens the issue in the milestone NAME (default Backlog). Each later run updates the
# issue. The first run where the fact is the same again closes it with a comment. A fact that the
# run skipped or could not check keeps its issue as it is. Issues of facts that the document does
# not list are left alone.
#
# A release with new set to true has one issue, which the script opens once. The marker is
# <!-- strata-contract: release <repo> <tag> -->. The issue asks to prove the facts again on the
# new release. A person closes it.
#
# The label is created when the repository has none. --dry-run prints each issue body and what
# the script does, and calls no host.
#
# Exit 0 when no fact differs and no release is new. Exit 1 when one does: the issues are handled.
# Exit 2 when the document is not a strata-contract/1 document, or holds a fact or a release that
# the run could not check, or when a host call fails. The script prints what the host said. The
# issues are handled first, so a scheduled job shows the state too. The weekly job keeps the tags
# of the releases only when the exit is not 2.
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

# github_open_issue TITLE BODYFILE: the function of the host layer, with the milestone. A new issue is
# in the milestone from its first moment, and gh opens no issue when the milestone does not exist.
# It prints the new number.
github_open_issue() {
  local url
  url=$(gh issue create --repo "$repo" --title "$1" --body-file "$2" --label "$label" --milestone "$milestone")
  echo "${url##*/}"
}

# ---- arguments ---------------------------------------------------------------------------------
document=""
repo="${GITHUB_REPOSITORY:-}"
label="palette-drift"
label_description="strata: a fact of Palette that differs from the stand-in"
milestone="Backlog"
dry=""
host_kind=github
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) repo=$2; shift 2 ;;
    --label) label=$2; shift 2 ;;
    --milestone) milestone=$2; shift 2 ;;
    --dry-run) dry=1; shift ;;
    -h|--help) sed -n '2,/^set -Eeuo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --*) die "unknown option $1" ;;
    *) document=$1; shift ;;
  esac
done
[ -n "$document" ] || die "usage: contract-report.sh <contract.json> [--repo OWNER/REPO] [--label NAME] [--milestone NAME] [--dry-run]"
[ -f "$document" ] || die "$document is not a file"
command -v jq > /dev/null || die "jq is not installed"
# -s reads the file as an array: a jq older than 1.7 exits with 0 for `jq -e` on an empty file
jq -es '.[0].schema == "strata-contract/1"' "$document" > /dev/null 2>&1 \
  || die "$document is not a strata-contract/1 document; no issue was touched"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
checked=$(jq -r .checkedAt "$document")
target=$(jq -r .target "$document")

if [ -z "$dry" ]; then
  [ -n "$repo" ] || repo=$(host repo)
  [ -n "$repo" ] || die "could not determine the project; pass --repo"
  host open_issues > "$work/issues.json"
fi

page="https://github.com/${repo:-OWNER/REPO}/blob/main/docs/src/development/facts.md#the-weekly-contract-run"

# issue_for MARKER -> the lowest open issue number whose body starts with MARKER
issue_for() {
  jq -r --arg m "$1" \
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

# ---- the facts ---------------------------------------------------------------------------------
differs=0 same=0 skipped=0 errors=0
while IFS= read -r entry <&3; do
  id=$(jq -r .id <<<"$entry")
  marker="<!-- strata-contract: $id -->"
  case "$(jq -r .outcome <<<"$entry")" in
    differs)
      differs=$((differs + 1))
      {
        echo "$marker"
        jq -r --arg checked "$checked" --arg target "$target" --arg page "$page" '
          "The contract run of \($checked) found that the tenant `\($target)` answers `\(.request)` in another way than the stand-in.",
          "",
          "| Difference | Field | The stand-in has | The tenant has |",
          "| --- | --- | --- | --- |",
          (.differences[] | "| \(.kind) | `\(.path)` | \(.expected | join(", ")) | \(.found | join(", ")) |"),
          "",
          "Prove the fact again with its live checks: \(if (.live | length) > 0 then (.live | map("`\(.)`") | join(", ")) else "the fact has none" end).",
          "The pages of the book that state the fact: \(.pages | map("`docs/src/\(.)`") | join(", ")).",
          "",
          "Then change the stand-in and the page, or record a fault of Palette in the local findings log. [The weekly contract run](\($page)) says how."
        ' <<<"$entry"
      } > "$work/body.md"
      open_or_update "$marker" "Palette drift: \`$id\` differs from the stand-in" "$work/body.md" "$id"
      ;;
    same)
      same=$((same + 1))
      if [ -n "$dry" ]; then
        note "dry run: closes the open issue for $id, if any"
        continue
      fi
      number=$(issue_for "$marker")
      [ -n "$number" ] || continue
      host close_issue "$number" "\`$id\` is the same as the stand-in again (checked $checked)."
      actions+=("closed #$number for $id")
      ;;
    skipped) skipped=$((skipped + 1)) ;;
    *) errors=$((errors + 1)) ;;
  esac
done 3< <(jq -c '.facts[]' "$document")

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

line="$((differs + same + skipped + errors)) facts: $same same, $differs differ, $skipped skipped, $errors could not be checked"
[ "$news" -eq 0 ] || line="$line; $news new release"
for a in "${actions[@]+"${actions[@]}"}"; do
  line="$line; $a"
done
note "$line"
if [ "$errors" -gt 0 ] || [ "$release_errors" -gt 0 ]; then exit 2; fi
if [ "$differs" -gt 0 ] || [ "$news" -gt 0 ]; then exit 1; fi
exit 0

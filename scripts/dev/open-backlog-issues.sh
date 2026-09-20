#!/usr/bin/env bash
# Open one GitHub issue per section of docs/backlog.md. Idempotent: a section whose title already
# exists as an open issue is skipped. Needs `gh` logged in and a GitHub remote on this repository.
set -euo pipefail

here=$(cd "$(dirname "$0")/../.." && pwd)
backlog="$here/docs/backlog.md"
xtask=${XTASK_BIN:-$here/target/debug/xtask}
[ -x "$xtask" ] || { echo "open-backlog-issues: no xtask at $xtask. Run: just stand-in" >&2; exit 2; }

gh repo view --json nameWithOwner --jq .nameWithOwner >/dev/null 2>&1 \
  || { echo "open-backlog-issues: this repository has no GitHub remote that gh can see" >&2; exit 2; }

# title<TAB>number for every issue, so existing ones are updated in place
existing=$(gh issue list --state all --limit 200 --json number,title --jq '.[] | "\(.title)\t\(.number)"')

# Labels used in the backlog; created when the repository does not have them yet.
declare -A LABEL_COLORS=(
  [feature]=1d76db [investigation]=fbca04 [ci]=0e8a16 [robustness]=d93f0b
  [ux]=c5def5 [design]=5319e7 [library]=bfd4f2
)
have_labels=$(gh label list --limit 200 --json name --jq '.[].name')
for name in "${!LABEL_COLORS[@]}"; do
  grep -Fxq "$name" <<<"$have_labels" || gh label create "$name" --color "${LABEL_COLORS[$name]}" --force >/dev/null
done

body_dir=$(mktemp -d); trap 'rm -rf "$body_dir"' EXIT
"$xtask" backlog "$backlog" "$body_dir" | while IFS=$'\t' read -r title labels body_file; do
  number=$(awk -F'\t' -v t="$title" '$1 == t { print $2; exit }' <<<"$existing")
  if [ -n "$number" ]; then
    args=(--body-file "$body_file")
    [ -n "$labels" ] && args+=(--add-label "$labels")
    gh issue edit "$number" "${args[@]}" >/dev/null
    echo "updated #$number $title"
  else
    args=(--title "$title" --body-file "$body_file")
    [ -n "$labels" ] && args+=(--label "$labels")
    url=$(gh issue create "${args[@]}")
    echo "opened $title -> $url"
  fi
done

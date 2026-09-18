#!/usr/bin/env bash
# Open one GitHub issue per section of docs/backlog.md. Idempotent: a section whose title already
# exists as an open issue is skipped. Needs `gh` logged in and a GitHub remote on this repository.
set -euo pipefail

here=$(cd "$(dirname "$0")/../.." && pwd)
backlog="$here/docs/backlog.md"

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

python3 - "$backlog" <<'PY' | while IFS=$'\t' read -r title labels body_file; do
import re, sys, tempfile, pathlib
text = pathlib.Path(sys.argv[1]).read_text()
sections = re.split(r"^## ", text, flags=re.M)[1:]
for sec in sections:
    title, _, rest = sec.partition("\n")
    m = re.search(r"^labels:\s*(.+)$", rest, flags=re.M)
    labels = m.group(1).strip() if m else ""
    body = re.sub(r"^labels:.*\n", "", rest, flags=re.M).strip() + "\n"
    body = re.sub(r"^### ", "## ", body, flags=re.M)  # file uses ### so issue titles stay unique at ##
    f = tempfile.NamedTemporaryFile("w", delete=False, suffix=".md"); f.write(body); f.close()
    print(f"{title.strip()}\t{labels}\t{f.name}")
PY
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
  rm -f "$body_file"
done

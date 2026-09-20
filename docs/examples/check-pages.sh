#!/usr/bin/env bash
# The rules a use case page must keep, which no other check can see. `just docs-check` runs this.
#
# A rule here is a rule the book states about itself. Add one when a walk finds the same defect
# twice: docs/examples/checklist.md is the procedure, and this file is what stops the drift.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
fail=0

note() {
  echo "check-pages: $1" >&2
  fail=1
}

# A release tag that a reader copies -- an install command, or a workflow ref -- must be this
# release. A tag from the release before sends them to the wrong binary, and nothing else notices.
#
# Two files keep older tags on purpose, and are not read here: development/live-runs.md records
# what each live run used, and development/release.md states that rule. Prose that names an older
# release for what it first carried is a fact about that release, so only the shapes below count.
version=v$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
copyable='(--version|strata-version:|STRATA_VERSION:|v=)[ ]*(v[0-9]+\.[0-9]+\.[0-9]+)|uses:.*@(v[0-9]+\.[0-9]+\.[0-9]+)'
while IFS= read -r hit; do
  case "$hit" in *"$version"*) continue ;; esac
  note "a reader copies this line, and this crate is $version: $hit"
done < <(cd "$root" && grep -rnE "$copyable" docs/src --include='*.md' |
  grep -v '^docs/src/development/live-runs.md:' |
  grep -v '^docs/src/development/release.md:' || true)

cd "$root/docs/src/use-cases"

# A reader follows a page from the top. The line that says how to serve the tenant must come
# before the first command, or the first thing the page asks for is a server it has not named.
for page in *.md; do
  # grep exits 1 when a page has no such line, and pipefail would stop the script
  first_block=$(grep -n '{{#include \.\./output/' "$page" | head -1 | cut -d: -f1) || true
  [ -n "$first_block" ] || continue
  follow=$(grep -n '\*\*Follow along\.\*\*' "$page" | head -1 | cut -d: -f1) || true
  if [ -z "$follow" ]; then
    note "$page has output blocks and no '**Follow along.**' line: a reader cannot run it"
  elif [ "$follow" -gt "$first_block" ]; then
    note "$page asks for a command at line $first_block, before '**Follow along.**' at line $follow"
  fi
done

[ "$fail" = 0 ] || exit 1
echo "check-pages: $(ls ./*.md | wc -l) use case pages keep the rules"

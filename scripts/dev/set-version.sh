#!/usr/bin/env bash
# Set the version of this repository: scripts/dev/set-version.sh <version>
#
# The version is in Cargo.toml, and it is also in the README badge, the install commands, the CI
# drop-ins, and the pages that show them. One command moves all of them and commits the move, so a
# release is a version and a tag, not an edit of a dozen files:
#
#   just version 1.2.3     # rewrites and commits
#   just release           # tags the commit
#
# It refuses when a file it must rewrite already has uncommitted changes, so the commit it writes
# is the version and nothing else.
#
# Recorded evidence is never touched, because it says which build produced a result:
#   examples/live/runs/                   the raw output of each live run
#   docs/src/development/live-runs.md     the page that reports those runs
#   docs/book/                            the built book
#
# `just test` checks the result: the CI drop-ins and the README badge must name the version in
# Cargo.toml.
#
# Write an example of this command with a version this repository will never be at, or a run
# rewrites the example too: `just version 1.2.3`, not `just version <the version now>`.
set -euo pipefail

here=$(cd "$(dirname "$0")/../.." && pwd)
cd "$here"

new=${1:-}
case "$new" in
  "" | -h | --help)
    sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
case "$new" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "set-version: '$new' is not a version of the form X.Y.Z" >&2; exit 2 ;;
esac

old=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
[ -n "$old" ] || { echo "set-version: no version line in Cargo.toml" >&2; exit 2; }
if [ "$old" = "$new" ]; then
  echo "set-version: already at $new"
  exit 0
fi

# every tracked file that names the old version, less the recorded runs and the built book
mapfile -t files < <(git grep -l -F "$old" -- \
  ':!examples/live/runs' \
  ':!docs/src/development/live-runs.md' \
  ':!docs/book' \
  ':!Cargo.lock' || true)

# the commit must hold the version and nothing else
dirty=()
for f in "${files[@]}" Cargo.lock; do
  git diff --quiet -- "$f" && git diff --cached --quiet -- "$f" || dirty+=("$f")
done
if [ ${#dirty[@]} -gt 0 ]; then
  echo "set-version: these hold uncommitted changes, and this command must rewrite them:" >&2
  printf '  %s\n' "${dirty[@]}" >&2
  echo "set-version: commit or stash them first" >&2
  exit 1
fi

for f in "${files[@]}"; do
  # v<old> first, so the tag form keeps its v
  sed -i "s/v${old}/v${new}/g; s/${old}/${new}/g" "$f"
  echo "set-version: $f"
done

# Cargo.lock carries the version of this crate; cargo writes it
cargo update --offline --package strata --precise "$new" >/dev/null 2>&1 \
  || cargo check --quiet >/dev/null 2>&1 || true

git add -- "${files[@]}" Cargo.lock
git commit -q -m "chore: release $new"
echo "set-version: $old -> $new in ${#files[@]} files, committed as 'chore: release $new'"
echo "set-version: tag it with: just release" >&2
